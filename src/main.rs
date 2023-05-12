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

use eframe::{App, Frame, NativeOptions};
use egui::{CentralPanel, Context};

mod cpal_wrapper;
mod sound_data;
mod sound_player;

/// Player of Speedball II sounds
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The file to load sound data from.
    #[arg(long)]
    file: PathBuf,
}

struct PlayerApp {
    bank: Arc<Mutex<sound_player::SoundBank>>,
    synth: Arc<Mutex<sound_player::Synth>>,
}

impl PlayerApp {
    fn new(bank: sound_player::SoundBank) -> PlayerApp {
        let bank = Arc::new(Mutex::new(bank));
        let synth = Arc::new(Mutex::new(sound_player::Synth::new(bank.clone())));
        PlayerApp { bank, synth }
    }
}

impl App for PlayerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        CentralPanel::default().show(ctx, |ui| {
            let mut synth = self.synth.lock().unwrap();
	    let mut bank = self.bank.lock().unwrap();
            synth.ui(&mut bank, ui);
        });
    }
}

fn main() {
    let args = Args::parse();

    let name = args.file;
    let data = std::fs::read(name).unwrap();
    let sound_bank = sound_player::SoundBank::new(data);
    let options = NativeOptions::default();
    let app = PlayerApp::new(sound_bank);
    let _stream = cpal_wrapper::sound_init(app.synth.clone());

    eframe::run_native(
        "Speedball II Sound Player",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .unwrap();
}
