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

use egui::{Button, CollapsingHeader, Color32, Ui};

use crate::cpal_wrapper;
use crate::sound_data;

// TODO: Not in the data, afaict.
const NUM_SEQUENCES: usize = 78;
const NUM_INSTRUMENTS: usize = 43;

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
                                channel.play(instrument);
                            }
                            ui.label(&format!("{:?}", instrument));
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
}

impl SampleChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SampleChannel {
        SampleChannel {
            bank,
            instr: None,
            volume: 1.0,
	    pitch: 0,
            phase: 0.0,
            step: 0.0,
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
	// Pitch table is in quarter semi-tones.
	let offset = self.pitch * 4;
	let period_ticks = sound_data::PITCHES[base_note + offset];
	self.step = period_ticks as f32 * CLOCK_INTERVAL_S;
	println!("Step: {}", self.step);
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
		let tmp = (mem[instrument.sample_addr + idx_int] as f32 / 128.0);
                *elt = tmp.to_sample::<T>();
		println!("{}", tmp);
            }
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
}

impl SoundChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SoundChannel {
	let sample_channel = SampleChannel::new(bank.clone());
        SoundChannel { bank, sample_channel }
    }

    pub fn play(&mut self, instr: &Instrument) {
	self.sample_channel.set_volume(64);
	self.sample_channel.set_pitch(0);
	self.sample_channel.play(instr);
    }

    pub fn stop(&mut self) {
	self.sample_channel.stop();
    }
}

impl cpal_wrapper::SoundSource for SoundChannel {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    ) {
	self.sample_channel.fill_buffer(num_channels, sample_rate, data);
    }
}
