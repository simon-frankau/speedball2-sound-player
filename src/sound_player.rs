//
// Speedball 2 Sound player
//
// sound_player.rs: Wrapped around raw input data to make the sound
// data accessible.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::fmt;
use std::sync::{Arc, Mutex};

use cpal::Sample;

use egui::{Button, CollapsingHeader, Color32, DragValue, Ui};

use crate::cpal_wrapper;
use crate::sound_data;

// TODO: Not in the data, afaict.
const NUM_SEQUENCES: usize = 78;
const NUM_INSTRUMENTS: usize = 43;

// TODO: Implement 000138b6 - 000145c6

////////////////////////////////////////////////////////////////////////
// Utilities

fn word(data: &[u8], addr: usize) -> u16 {
    (data[addr] as u16) << 8 | (data[addr + 1] as u16)
}

fn long(data: &[u8], addr: usize) -> u32 {
    (data[addr] as u32) << 24
        | (data[addr + 1] as u32) << 16
        | (data[addr + 2] as u32) << 8
        | (data[addr + 3] as u32)
}

////////////////////////////////////////////////////////////////////////
// Instrument definition

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instrument {
    is_one_shot: bool,
    loop_offset: u16,
    sample_len: u16,
    sample_addr: usize,
    base_octave: usize,
}

impl Instrument {
    const SIZE: usize = 14;

    fn new(data: &[u8]) -> Instrument {
        Instrument {
            is_one_shot: word(data, 0) == 1,
            loop_offset: word(data, 2),
            sample_len: word(data, 4),
            sample_addr: long(data, 6) as usize,
            base_octave: long(data, 10) as usize,
        }
    }
}

////////////////////////////////////////////////////////////////////////
// And put it all together!

pub struct SoundBank {
    // Raw memory data.
    pub data: Vec<u8>,
    // Instrment data scraped into structs.
    pub instruments: Vec<Instrument>,
    // Sequence definitions don't include length, so we just store
    // starting points.
    pub sequences: Vec<usize>,
}

// Skip data.
impl fmt::Debug for SoundBank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundBank")
            .field("sequences", &self.sequences)
            .field("instruments", &self.instruments)
            .finish()
    }
}

impl SoundBank {
    pub fn new(data: Vec<u8>) -> SoundBank {
        let sequence_table_offset = long(&data, 0) as usize;
        let sequences = (0..NUM_SEQUENCES)
            .map(|idx| long(&data, sequence_table_offset + idx * 4) as usize)
            .collect();

        let instrument_table_offset = long(&data, 4) as usize;
        let instruments = (0..NUM_INSTRUMENTS)
            .map(|idx| Instrument::new(&data[(instrument_table_offset + idx * Instrument::SIZE)..]))
            .collect();

        SoundBank {
            data,
            sequences,
            instruments,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, channel: &mut SoundChannel) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (idx, instrument) in self.instruments.iter().enumerate() {
                CollapsingHeader::new(format!("Instrument {:02x}", idx))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add(Button::new("Play").fill(Color32::DARK_RED))
                                .clicked()
                            {
                                channel.play_instr(instrument);
                            }
                            ui.label(&format!("{:?}", instrument));
                        });
                    });
            }

            for (idx, addr) in self.sequences.iter().enumerate() {
                CollapsingHeader::new(format!("Sequence {:02x}", idx))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if ui
                                .add(Button::new("Play").fill(Color32::DARK_RED))
                                .clicked()
                            {
                                println!("Playing sequence {:x}", idx);
                                channel.play_seq(*addr);
                            }
                            ui.label(&format!("0x{:06x}", addr));
                        });
                    });
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////
// Emulations of the low-level "play a sample" functionality provided
// by Amiga hardware and the sound interrupt routine.
//

struct SampleChannel {
    bank: Arc<Mutex<SoundBank>>,
    instr: Option<Instrument>,
    volume: f32,
    pitch: usize,
    phase: f32,
    step: f32,
    lerp: bool,
}

impl SampleChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SampleChannel {
        SampleChannel {
            bank,
            instr: None,
            volume: 1.0,
            pitch: 48 * 4,
            phase: 0.0,
            step: 0.0,
            lerp: true,
        }
    }

    // New sounds are triggered immediately.
    pub fn play(&mut self, instr: &Instrument) {
        self.instr = Some(instr.clone());
        self.phase = 0.0;
        self.set_step(instr.base_octave);
    }

    // Running sounds are stopped at a convenient point.
    pub fn stop(&mut self) {
        if let Some(current_instr) = &mut self.instr {
            // Stop at next loop.
            current_instr.is_one_shot = true;
        }
    }

    // Running sounds are stopped immediately.
    pub fn stop_hard(&mut self) {
        self.instr = None;
    }

    pub fn set_volume(&mut self, volume: u16) {
        const MAX_VOLUME: f32 = 64.0;
        self.volume = volume as f32 / MAX_VOLUME;
    }

    // Takes a note number, as used by sequences.
    pub fn set_pitch(&mut self, pitch: usize) {
        self.pitch = pitch;
        if let Some(instr) = &self.instr {
            // Already playing, update step.
            self.set_step(instr.base_octave);
        }
    }

    fn set_step(&mut self, base_octave: usize) {
        // This is PAL. 0.279365 for NTSC.
        const CLOCK_INTERVAL_S: f32 = 0.281937e-6;

        // For some reason, the lowest base is one octave above the
        // lowest note.
        let base_note = (base_octave + 1) * sound_data::OCTAVE_SIZE;
        let period_ticks = sound_data::PITCHES[base_note + self.pitch];
        self.step = period_ticks as f32 * CLOCK_INTERVAL_S;
    }
}

impl cpal_wrapper::SoundSource for SampleChannel {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    ) {
        // Simple base case.
        for elt in data.iter_mut() {
            *elt = Sample::EQUILIBRIUM;
        }

        if let Some(instrument) = &mut self.instr {
            // Treating multiple channels as single channel at a
            // higher frequency is wrong, but will do until I write
            // the mixer.
            let rate = sample_rate as f32 * num_channels as f32;
            let step = 1.0 / (self.step * rate);

            let mem = &self.bank.lock().unwrap().data;
            for elt in data.iter_mut() {
                self.phase += step;
                let mut idx_int = self.phase as usize;

                if idx_int >= instrument.sample_len as usize * 2 {
                    if instrument.is_one_shot {
                        self.instr = None;
                        break;
                    } else {
                        self.phase -= (instrument.sample_len * 2 - instrument.loop_offset) as f32;
                        idx_int = self.phase as usize;
                    }
                }

                let val = if self.lerp {
                    let left = mem[instrument.sample_addr + idx_int] as i8 as f32;
                    let right_idx = instrument.sample_addr + idx_int + 1;
                    let right = if right_idx
                        == instrument.sample_addr + instrument.sample_len as usize * 2
                    {
                        if instrument.is_one_shot {
                            0
                        } else {
                            mem[instrument.sample_addr + instrument.loop_offset as usize]
                        }
                    } else {
                        mem[right_idx]
                    } as i8 as f32;
                    let x = self.phase.fract();
                    left * (1.0 - x) + right * x
                } else {
                    mem[instrument.sample_addr + idx_int] as i8 as f32
                };

                *elt = (val / 128.0).to_sample::<T>();
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////
// Sequence of commands for playing sounds, along with the state to do
// so.
//

pub struct Sequence {
    addr: usize,
    frames_per_beat: usize,
    transposition: isize,
    instrument_idx: usize,
    note_len: usize,
    ttl: usize,
}

#[derive(Eq, PartialEq)]
enum EvalResult {
    Done, // Equivalent to falling through to `sound_op_cont`.
    Cont, // Equivalent to jumping to `sound_next_command`.
    Stop, // Actually equiv to clearing current sound, then Done.
}

impl Sequence {
    pub fn new(addr: usize) -> Sequence {
        Sequence {
            addr,
            frames_per_beat: 0,
            transposition: 0,
            instrument_idx: 0,
            note_len: 0,
            ttl: 0,
        }
    }

    // Run a single command in the command sequence. Implements
    // `sound_next_command`.
    fn eval(&mut self, bank: &SoundBank, channel: &mut SampleChannel) -> EvalResult {
        let code = bank.data[self.addr];
        self.addr += 1;

        if code < 0x80 {
            // TODO: Reinitialise envelope.
            // TODO: Reinitialise tremolo and vibrato.
            if cfg!(debug) {
                println!("Note {}", code);
            }
            channel.set_pitch((code as usize * 4).wrapping_add_signed(self.transposition));
            channel.play(&bank.instruments[self.instrument_idx]);
            self.ttl = self.note_len;
            return EvalResult::Done;
        }

        match code {
            0x80 => {
                // Set volume
                let volume = bank.data[self.addr];
                self.addr += 1;
                // TODO: Should be chained with other processing.
                if cfg!(debug) {
                    println!("Vol: {}", volume);
                }
                channel.set_volume(volume as u16);
            }
            0x8c => {
                // Set note length
                let note_len = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Len: {}", note_len);
                }
                self.note_len = note_len as usize * self.frames_per_beat;
            }
            0x90 => {
                // Rest.
                if cfg!(debug) {
                    println!("Rest");
                }
                // TODO: Should stop playing if loop-to-zero (!).
                return EvalResult::Done;
            }
            0x94 => {
                // Set tempo
                let bpm = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Tempo: {} bpm", bpm);
                }
                self.frames_per_beat = 750 / bpm as usize;
            }
            0x9c => {
                // Set effect
                let effect = bank.data[self.addr];
                self.addr += 1;
                // TODO: Actually apply effect.
                println!("Effect: {} (NYI)", effect);
            }
            0xa8 => {
                // Loop flags
                let loop_flags = bank.data[self.addr];
                self.addr += 1;
                println!("Loop: {} (NYI)", loop_flags);
            }
            0xac => {
                // Stop
                if cfg!(debug) {
                    println!("Stop");
                }
                return EvalResult::Stop;
            }
            0xbc => {
                // Set transposition
                let transposition = bank.data[self.addr] as i8;
                self.addr += 1;
                if cfg!(debug) {
                    println!("Trans: {}", transposition);
                }
                self.transposition = transposition as isize;
            }
            0xd0 => {
                // Set instrument
                let instr_idx = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Instrument: {}", instr_idx);
                }
                self.instrument_idx = instr_idx as usize;
            }
            0xd4 => {
                // Jump
                let seq_idx = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Jump: {}", seq_idx);
                }
                self.addr = bank.sequences[seq_idx as usize];
            }
            unknown => {
                println!("Unknown code: {:02x}. Bailing.", unknown);
                return EvalResult::Stop;
            }
        }

        // Default to processing next item.
        EvalResult::Cont
    }

    // Perform a timestep of the sequence, usually synchronised with a
    // vertical blanking interval. Returns whether the sequence
    // continues.
    fn step_frame(&mut self, bank: &SoundBank, channel: &mut SampleChannel) -> bool {
        if self.ttl > 0 {
            self.ttl -= 1;
            // TODO: Actually, still need to do non-command updates!
            return true;
        }

        // TODO: Terminate sounds with loop offset of zero immediately (see `sound_update_chnanel`).

        let mut result = EvalResult::Cont;
        while result == EvalResult::Cont {
            result = self.eval(bank, channel);
        }

        if result == EvalResult::Done {
            self.ttl = self.note_len;
            // TODO: Fall through to LAB_000142c0.
            self.ttl -= 1;
            // TODO: Common with the non-command updates.
            true
        } else {
            channel.stop_hard();
            false
        }
    }
}

////////////////////////////////////////////////////////////////////////
// Sound channel capable of playing a sound.
//

// TODO: Features to emulate:
// * sound_effects and sound_op_effect
// * sound_envelopes and sound_op_set_envelope
// * sound_update every 50th of a second.
//   * sound_update_hardware_channel for basic move-along.
// * Mixing together the multiple channels, etc.

pub struct SoundChannel {
    bank: Arc<Mutex<SoundBank>>,
    sample_channel: SampleChannel,
    samples_remaining: usize,
    sequence: Option<Sequence>,
}

impl SoundChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SoundChannel {
        let sample_channel = SampleChannel::new(bank.clone());
        SoundChannel {
            bank,
            sample_channel,
            samples_remaining: 0,
            sequence: None,
        }
    }

    pub fn play_instr(&mut self, instr: &Instrument) {
        self.sample_channel.play(instr);
    }

    pub fn play_seq(&mut self, seq: usize) {
        self.sequence = Some(Sequence::new(seq));
    }

    pub fn stop(&mut self) {
        self.sample_channel.stop();
        self.sequence = None;
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if ui
                .add(Button::new("Stop").fill(Color32::DARK_RED))
                .clicked()
            {
                self.stop();
            }
            ui.checkbox(&mut self.sample_channel.lerp, "Linear interpolation");
            ui.label("Volume");
            ui.add(DragValue::new(&mut self.sample_channel.volume));
            ui.label("Pitch");
            if ui
                .add(DragValue::new(&mut self.sample_channel.pitch))
                .changed()
            {
                // Messy. Need to force recalculation of derived period.
                self.sample_channel.set_pitch(self.sample_channel.pitch);
            }
        });
    }
}

impl cpal_wrapper::SoundSource for SoundChannel {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    ) {
        // Not going to try to do sub-sample accuracy.
        const FRAMES_PER_SECOND: usize = 50;
        let samples_per_frame = sample_rate as usize / FRAMES_PER_SECOND;
        let ch = num_channels as usize;

        let mut data = data;
        // Fill buffer until we hit a new frame, repeat.
        while data.len() / ch as usize >= self.samples_remaining {
            self.sample_channel.fill_buffer(
                num_channels,
                sample_rate,
                &mut data[..self.samples_remaining * ch as usize],
            );

            if let Some(sequence) = &mut self.sequence {
                if !sequence.step_frame(&self.bank.lock().unwrap(), &mut self.sample_channel) {
                    self.sequence = None;
                }
            }

            data = &mut data[self.samples_remaining * ch..];
            self.samples_remaining = samples_per_frame;
        }

        // And fill any leftover.
        self.sample_channel
            .fill_buffer(num_channels, sample_rate, data);
        self.samples_remaining -= data.len() / ch;
    }
}
