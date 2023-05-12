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

use egui::{Button, CollapsingHeader, Color32, DragValue, RichText, Ui};

use crate::cpal_wrapper;
use crate::sound_data::*;

// TODO: Not in the data, afaict.
// const NUM_SEQUENCES: usize = 78;
// const NUM_INSTRUMENTS: usize = 43;

// TODO: From the intro.
const NUM_SEQUENCES: usize = 27;
const NUM_INSTRUMENTS: usize = 40;

const MAX_VOLUME: f32 = 64.0;

// TODO: Implement 000138b6 - 000145c6
//  * sound_play

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
        CollapsingHeader::new(format!("Instruments"))
            .default_open(false)
            .show(ui, |ui| {
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
            });

        CollapsingHeader::new(format!("Sequences"))
            .default_open(false)
            .show(ui, |ui| {
                for (idx, addr) in self.sequences.iter().enumerate() {
                    CollapsingHeader::new(format!("Sequence {:02x}", idx))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(Button::new("Play").fill(Color32::DARK_RED))
                                    .clicked()
                                {
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
    volume_adjust: f32,
    pitch: usize,
    pitch_adjust: i16,
    phase: f32,
    lerp: bool,
}

impl SampleChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SampleChannel {
        SampleChannel {
            bank,
            instr: None,
            volume: 1.0,
            volume_adjust: 0.0,
            pitch: 48 * 4,
            pitch_adjust: 0,
            phase: 0.0,
            lerp: true,
        }
    }

    // New sounds are triggered immediately.
    pub fn play(&mut self, instr: &Instrument) {
        self.instr = Some(instr.clone());
        self.phase = 0.0;
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

    // Special case: Stop the sound if the loop start is at zero. Why,
    // I have no idea.
    pub fn stop_loop(&mut self) {
        if let Some(instrument) = &self.instr {
            if instrument.loop_offset == 0 {
                self.stop_hard();
            }
        }
    }

    fn calc_time_step(&self) -> f32 {
        if let Some(instrument) = &self.instr {
            // This is PAL. 0.279365 for NTSC.
            const CLOCK_INTERVAL_S: f32 = 0.281937e-6;

            // For some reason, the lowest base is one octave above the
            // lowest note.
            let base_note = (instrument.base_octave + 1) * OCTAVE_SIZE;
            let period_tick =
                PITCHES[base_note + self.pitch].wrapping_add_signed(self.pitch_adjust);
            period_tick as f32 * CLOCK_INTERVAL_S
        } else {
            0.0
        }
    }

    fn fill_buffer(&mut self, num_channels: u16, sample_rate: u32, data: &mut [f32]) {
        // Simple base case.
        for elt in data.iter_mut() {
            *elt = Sample::EQUILIBRIUM;
        }

        // Treating multiple channels as single channel at a
        // higher frequency is wrong, but will do until I write
        // the mixer.
        let rate = sample_rate as f32 * num_channels as f32;
        let time_step = self.calc_time_step();
        let step = 1.0 / (time_step * rate);

        let vol = self.volume + self.volume_adjust;

        if let Some(instrument) = &mut self.instr {
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

                *elt = vol * val / 128.0;
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////
// Implementation of the tremolo/vibrato effects.
//

#[derive(Clone, Copy)]
pub struct BendState {
    pause_count: u8,
    length_count: u8,
}

impl BendState {
    fn new() -> BendState {
        BendState {
            pause_count: 0,
            length_count: 0,
        }
    }

    fn from(bend: &Bend) -> BendState {
        BendState {
            pause_count: bend.pause,
            length_count: bend.length,
        }
    }
}

pub struct EffectState {
    tremolos: [BendState; 2],
    vibratos: [BendState; 3],
    tremolo_loops: bool,
    vibrato_loops: bool,
    vol_adjust: i16,
    period_adjust: i16,
}

impl EffectState {
    // Used to initialise state when setting a new effect.
    fn new() -> EffectState {
        EffectState {
            tremolos: [BendState::new(); 2],
            vibratos: [BendState::new(); 3],
            tremolo_loops: false,
            vibrato_loops: false,
            vol_adjust: 0,
            period_adjust: 0,
        }
    }

    // Used to reset state when playing new notes.
    fn reset(&mut self, effect: &Effect) {
        // NB: Keeps existing flags.
        self.tremolos = effect.tremolos.map(|x| BendState::from(&x));
        self.vibratos = effect.vibratos.map(|x| BendState::from(&x));
        self.vol_adjust = 0;
        self.period_adjust = 0;
    }

    // Steps a sequence of bends, returns the delta to be applied to
    // the relevant variable.
    fn step(bends: &[Bend], bend_states: &mut [BendState], loops: bool) -> i16 {
        for (fx, fx_state) in bends.iter().zip(bend_states.iter_mut()) {
            if fx_state.pause_count > 0 {
                fx_state.pause_count -= 1;
                continue;
            }

            if fx_state.length_count == 0 {
                continue;
            }
            fx_state.length_count -= 1;
            fx_state.pause_count = fx.pause;
            return fx.rate;
        }

        // Once we've reached the end, loop if the flag's set.
        if loops {
            for (dst, src) in bend_states.iter_mut().zip(bends.iter()) {
                *dst = BendState::from(src);
            }
        }
        0
    }

    fn step_tremolo(&mut self, effect: &Effect) {
        self.period_adjust +=
            EffectState::step(&effect.vibratos, &mut self.vibratos, self.vibrato_loops);
    }

    fn step_vibrato(&mut self, effect: &Effect) {
        self.vol_adjust +=
            EffectState::step(&effect.tremolos, &mut self.tremolos, self.tremolo_loops);
    }
}

////////////////////////////////////////////////////////////////////////
// Sequence of commands for playing sounds, along with the state to do
// so.
//

pub struct Sequence {
    addr: usize,
    start_addr: usize,
    frames_per_beat: usize,
    transposition: isize,
    instrument_idx: usize,
    note_len: usize,
    ttl: usize,
    effect: Effect,
    effect_state: EffectState,
    loop_stack: Vec<(u8, usize)>,
}

#[derive(Eq, PartialEq)]
enum EvalResult {
    Done, // Equivalent to falling through to `sound_op_cont`.
    Cont, // Equivalent to jumping to `sound_next_command`.
    Stop, // Actually equiv to clearing current sound, then Done.
}

impl Sequence {
    pub fn new(addr: usize) -> Sequence {
        let no_effect = EFFECTS[0];
        Sequence {
            addr,
            start_addr: addr,
            frames_per_beat: 0,
            transposition: 0,
            instrument_idx: 0,
            note_len: 0,
            ttl: 0,
            effect: no_effect,
            effect_state: EffectState::new(),
            loop_stack: Vec::new(),
        }
    }

    // Run a single command in the command sequence. Implements
    // `sound_next_command`.
    fn eval(
        &mut self,
        bank: &SoundBank,
        channel: &mut SampleChannel,
        options: &Options,
    ) -> EvalResult {
        let code = bank.data[self.addr];
        self.addr += 1;

        if code < 0x80 {
            if cfg!(debug) {
                println!("Note {}", code);
            }

            // If envelopes were implemented, they would be
            // reinitialised here.

            // New notes reset tremolo/vibrato state.
            self.effect_state.reset(&self.effect);
            channel.pitch = (code as usize * 4).wrapping_add_signed(self.transposition);
            channel.play(&bank.instruments[self.instrument_idx]);
            self.ttl = self.note_len;
            return EvalResult::Done;
        }

        match code {
            0x80 => {
                // Set volume
                let volume = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Vol: {}", volume);
                }
                channel.volume = volume as f32 / MAX_VOLUME;
            }
            0x88 => {
                // Go back to start
                if cfg!(debug) {
                    println!("Restart");
                }
                if !options.repeats {
                    return EvalResult::Done;
                }
                self.addr = self.start_addr;
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
                channel.stop_loop();
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
                if cfg!(debug) {
                    println!("Effect: {}", effect);
                }
                self.effect = EFFECTS[effect as usize];
                self.effect_state = EffectState::new();
            }
            0xa8 => {
                // Effects looping flags
                let loop_flags = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Loop: {}", loop_flags);
                }
                self.effect_state.tremolo_loops = loop_flags & 1 != 0;
                self.effect_state.vibrato_loops = loop_flags & 2 != 0;
            }
            0xac => {
                // Stop
                if cfg!(debug) {
                    println!("Stop");
                }
                return EvalResult::Stop;
            }
            0xb0 => {
                // Call
                let seq_idx = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("Call: {}", seq_idx);
                }
                self.loop_stack.push((0, self.addr));
                self.addr = bank.sequences[seq_idx as usize];
            }
            0xb4 => {
                // Return
                if cfg!(debug) {
                    println!("Return");
                }
                if let Some((i, ret_addr)) = self.loop_stack.pop() {
                    assert_eq!(i, 0, "Return doesn't match call");
                    self.addr = ret_addr;
                } else {
                    // Treat a return on a sequence that we've played
                    // directly as end-of-sequence.
                    return EvalResult::Stop;
                }
            }
            0xb8 => {
                // Add transposition
                let transposition = bank.data[self.addr] as i8;
                self.addr += 1;
                if cfg!(debug) {
                    println!("TransRel: {}", transposition);
                }
                if transposition == 0 {
                    self.transposition = 0;
                } else {
                    self.transposition += transposition as isize;
                }
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
            0xc0 => {
                // For loop
                let count = bank.data[self.addr];
                self.addr += 1;
                if cfg!(debug) {
                    println!("For: {}", count);
                }
                self.loop_stack.push((count, self.addr));
            }
            0xc4 => {
                // Next
                if cfg!(debug) {
                    println!("Next");
                }
                let (count, loop_addr) = self.loop_stack.last_mut().unwrap();
                if *count == 0 {
                    self.loop_stack.pop();
                } else {
                    *count -= 1;
                    self.addr = *loop_addr;
                }
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
    fn update(&mut self, bank: &SoundBank, channel: &mut SampleChannel, options: &Options) -> bool {
        if self.ttl > 0 {
            return true;
        }

        let mut result = EvalResult::Cont;
        while result == EvalResult::Cont {
            result = self.eval(bank, channel, options);
        }

        self.ttl = self.note_len;

        if result == EvalResult::Done {
            true
        } else {
            channel.stop_hard();
            false
        }
    }

    fn step_frame(
        &mut self,
        bank: &SoundBank,
        channel: &mut SampleChannel,
        options: &Options,
    ) -> bool {
        let running = self.update(bank, channel, options);
        if running {
            self.ttl -= 1;
            // If envelope were implemented, it would go here, and
            // based on the assembly code, an envelope would disable
            // the effects.
            if options.tremolo {
                self.effect_state.step_tremolo(&self.effect);
                channel.pitch_adjust = self.effect_state.period_adjust;
            }
            if options.vibrato {
                self.effect_state.step_vibrato(&self.effect);
                channel.volume_adjust = self.effect_state.vol_adjust as f32 / MAX_VOLUME;
            }
        }
        running
    }
}

////////////////////////////////////////////////////////////////////////
// Sound channel capable of playing a sound.
//

// TODO: Features to emulate:
// * Mixing together the multiple channels, etc.

pub struct Options {
    tremolo: bool,
    vibrato: bool,
    repeats: bool,
}

impl Options {
    fn new() -> Options {
        Options {
            tremolo: true,
            vibrato: true,
            repeats: true,
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.tremolo, "Tremolo");
        ui.checkbox(&mut self.vibrato, "Vibrato");
        ui.checkbox(&mut self.repeats, "Repeats");
    }
}

pub struct SoundChannel {
    bank: Arc<Mutex<SoundBank>>,
    sample_channel: SampleChannel,
    samples_remaining: usize,
    sequence: Option<Sequence>,
    options: Options,
}

impl SoundChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SoundChannel {
        let sample_channel = SampleChannel::new(bank.clone());
        SoundChannel {
            bank,
            sample_channel,
            samples_remaining: 0,
            sequence: None,
            options: Options::new(),
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
            let stop_colour = if self.sequence.is_some() || self.sample_channel.instr.is_some() {
                Color32::DARK_RED
            } else {
                Color32::DARK_GRAY
            };
            if ui.add(Button::new("Stop").fill(stop_colour)).clicked() {
                self.stop();
            }
            ui.checkbox(&mut self.sample_channel.lerp, "Linear interpolation");
            ui.label("Volume");
            ui.add(DragValue::new(&mut self.sample_channel.volume));
            ui.label("Pitch");
            ui.add(DragValue::new(&mut self.sample_channel.pitch));
            ui.checkbox(&mut self.sample_channel.lerp, "Linear interpolation");

            self.options.ui(ui);
        });
    }

    fn fill_buffer(&mut self, num_channels: u16, sample_rate: u32, data: &mut [f32]) {
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
                if !sequence.step_frame(
                    &self.bank.lock().unwrap(),
                    &mut self.sample_channel,
                    &self.options,
                ) {
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

////////////////////////////////////////////////////////////////////////
// 4-channel synthesiser

pub struct Synth {
    pub channels: [SoundChannel; 4],
    stereo: bool,
}

impl Synth {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> Synth {
        Synth {
            // Simplest way I could find to do this!
            channels: [(); 4].map(|()| SoundChannel::new(bank.clone())),
            stereo: false,
        }
    }

    pub fn play_sound(&mut self, bank: &SoundBank, sound: &Sound) {
	for (channel, seq) in self.channels.iter_mut().zip(sound.sequences.iter()) {
	    if *seq != 0 {
		channel.play_seq(bank.sequences[*seq]);
	    }
	}
    }

    pub fn sound_ui(&mut self, bank: &SoundBank, ui: &mut Ui) {
        CollapsingHeader::new(format!("Sounds"))
            .default_open(true)
            .show(ui, |ui| {
                for (idx, sound) in SOUNDS.iter().enumerate() {
                    CollapsingHeader::new(format!("Sound {:02x}", idx))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui
                                    .add(Button::new("Play").fill(Color32::DARK_RED))
                                    .clicked()
                                {
				    self.play_sound(bank, sound);
                                }
                                ui.label(&format!("{:?}", sound));
                            });
                        });
                }
            });
    }

    pub fn ui(&mut self, bank: &mut SoundBank, ui: &mut Ui) {
	for (idx, channel) in self.channels.iter_mut().enumerate() {
	    // Cheap alignment.
	    ui.label(RichText::new(format!("Ch {}", idx)).monospace());
            channel.ui(ui);
	}

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Instruments and Sequences - use channel 0.
                bank.ui(ui, &mut self.channels[0]);
		// And sounds
		self.sound_ui(bank, ui);
            });
    }
}

impl cpal_wrapper::SoundSource for Synth {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    ) {
        data.fill(Sample::EQUILIBRIUM);

        let mixer_scale = 1.0 / self.channels.len() as f32;

        // TODO: Really dull way to do this. Need to do stereo.
        let mut tmp = vec![0.0; data.len()];
        for channel in self.channels.iter_mut() {
            channel.fill_buffer(num_channels, sample_rate, &mut tmp);
            for (dst, src) in data.iter_mut().zip(tmp.iter()) {
                *dst = dst.add_amp((mixer_scale * src).to_sample::<T>().to_signed_sample());
            }
        }
    }
}
