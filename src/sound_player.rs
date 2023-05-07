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
                                .add(Button::new("Trigger").fill(Color32::DARK_RED))
                                .clicked()
                            {
                                channel.trigger(instrument);
                            }
                            ui.label(&format!("{:?}", instrument));
                        });
                    });
            }
        });
    }
}

////////////////////////////////////////////////////////////////////////
// Sound channel capable of playing a sound.
//

pub struct SoundChannel {
    bank: Arc<Mutex<SoundBank>>,
    sound: Option<(Instrument, f32)>,
}

impl SoundChannel {
    pub fn new(bank: Arc<Mutex<SoundBank>>) -> SoundChannel {
        SoundChannel { bank, sound: None }
    }

    pub fn trigger(&mut self, instr: &Instrument) {
        if let Some((current_instr, _)) = &self.sound {
            if current_instr == instr {
                // Already playing. Stop.
                self.sound = None;
                return;
            }
        }

        self.sound = Some((instr.clone(), 0.0));
    }
}

impl cpal_wrapper::SoundSource for SoundChannel {
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

        if let Some((instrument, idx)) = &mut self.sound {
            /*
               // TODO: let phase_per_sample = self.freq / (sample_rate as f32);
               for (idx, elt) in data.iter_mut().enumerate() {
                   let phase =
                       (self.phase + phase_per_sample * (idx / num_channels as usize) as f32).fract();
                   let val = if phase > 0.5 { 0.5 } else { -0.5 };
                   *elt = val.to_sample::<T>();
               }
               self.phase = (self.phase
                   + phase_per_sample * (data.len() / num_channels as usize) as f32)
               .fract();
            */
            let downsample = sample_rate as f32 / 11025.0; // TODO: Fixed frequency.
            let rate = downsample * num_channels as f32;

            let mem = &self.bank.lock().unwrap().data;
            for elt in data.iter_mut() {
                *idx += 1.0 / rate;
                let mut idx_int = *idx as usize;

                if idx_int > instrument.sample_len as usize * 2 {
                    if instrument.is_one_shot {
                        self.sound = None;
                        break;
                    } else {
                        *idx -= (instrument.sample_len * 2 - instrument.loop_offset) as f32;
                        idx_int = *idx as usize;
                    }
                }

                *elt = (mem[instrument.sample_addr + idx_int] as f32 / 128.0).to_sample::<T>();
            }
        }
    }
}
