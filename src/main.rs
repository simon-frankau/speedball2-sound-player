//
// Speedball 2 Sound player
//
// main.rs: Main entry point.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use clap::Parser;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};


use eframe::{App, Frame, NativeOptions};
use egui::{Button, CentralPanel, CollapsingHeader, Color32, Context, Label, RichText, Ui};

mod sound_model;

/// Player of Speedball II sounds
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The file to load sound data from.
    #[arg(long)]
    file: PathBuf,
}

struct PlayerApp {
    bank: Arc<Mutex<sound_model::SoundBank>>,
}

// IHNI, Fix later!
fn sm_ui(model: &sound_model::SoundBank, ui: &mut Ui) {
    egui::ScrollArea::vertical().show(ui, |ui| {
	for (idx, instrument) in model.instruments.iter().enumerate() {
	    CollapsingHeader::new(&format!("Instrument {:02x}", idx))
		.default_open(true)
		.show(ui, |ui| {
		    ui.horizontal(|ui| {
			if ui
                            .add(Button::new("Trigger").fill(Color32::DARK_RED))
                            .clicked()
			{
                            // self.trigger();
			}
			ui.label(&format!("{:?}", instrument));
		    });
		});
	}
    });
}

impl App for PlayerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        CentralPanel::default().show(ctx, |ui| {
            sm_ui(&self.bank.lock().unwrap(), ui);
        });
    }
}

trait SoundSource {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
	num_channels: u16,
	sample_rate: u32,
        data: &mut [T],
    );
}

struct Square {
    freq: f32,
    phase: f32,
}

impl Square {
    fn new() -> Square {
	Square {
	    freq: 440.0,
	    phase: 0.0,
	}
    }
}

impl SoundSource for Square {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
	num_channels: u16,
	sample_rate: u32,
        data: &mut [T],
    ) {
	let phase_per_sample = self.freq / (sample_rate as f32);
        for (idx, elt) in data.iter_mut().enumerate() {
	    let phase = (self.phase + phase_per_sample * (idx / num_channels as usize) as f32).fract();
	    let val = if phase > 0.5 { 0.5 } else { -0.5 };
            *elt = val.to_sample::<T>();
        }
	self.phase = (self.phase + phase_per_sample * (data.len() / num_channels as usize) as f32).fract();
    }
}

fn sound_init<S>(source: Arc<Mutex<S>>) -> Stream where S: SoundSource + Send + 'static {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no output device available");
    let mut supported_configs_range = device
        .supported_output_configs()
        .expect("error while querying configs");
    let supported_config = supported_configs_range
        .next()
        .expect("no supported config?!")
        .with_max_sample_rate();
    let err_fn = |err| eprintln!("an error occurred on the output audio stream: {}", err);
    let sample_format = supported_config.sample_format();
    let num_channels = supported_config.channels();
    let sample_rate = supported_config.sample_rate().0;
    let config = supported_config.into();

    let stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                source.lock().unwrap().fill_buffer::<f32>(num_channels, sample_rate, data);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _info: &cpal::OutputCallbackInfo| {
                source.lock().unwrap().fill_buffer::<i16>(num_channels, sample_rate, data);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _info: &cpal::OutputCallbackInfo| {
                source.lock().unwrap().fill_buffer::<u16>(num_channels, sample_rate, data);
            },
            err_fn,
            None,
        ),
        sample_format => panic!("Unsupported sample format '{sample_format}'"),
    }
    .expect("couldn't build output stream");

    stream.play().expect("couldn't play");
    stream
}

fn main() {
    let args = Args::parse();

    let name = args.file;
    let data = std::fs::read(name).unwrap();
    let sound_bank = sound_model::SoundBank::new(data);

    let options = NativeOptions::default();
    let app = PlayerApp {
	bank: Arc::new(Mutex::new(sound_bank)),
    };

    let sound_gen = Arc::new(Mutex::new(Square::new()));
    let stream = sound_init(sound_gen.clone());

    eframe::run_native(
        "Speedball II Sound Player",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .unwrap();
}
