//
// Speedball 2 Sound player
//
// main.rs: Main entry point.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, ValueEnum};

use eframe::{App, Frame, NativeOptions};
use egui::{CentralPanel, Context};

mod cpal_wrapper;
mod sound_data;
mod sound_player;

#[derive(Clone, Debug, Parser, ValueEnum)]
enum Bank {
    /// Sounds and music from the intro sequence
    Intro,
    /// Sound effects used by the main game
    Game,
}

/// Player of Speedball II sounds
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The sound bank to load
    #[arg(value_enum)]
    bank: Bank,
}

struct Config {
    file: &'static str,
    num_sequences: usize,
    num_instruments: usize,
}

const INTRO_CONF: Config = Config {
    file: "data/intro.bin",
    num_sequences: 27,
    num_instruments: 40,
};

const GAME_CONF: Config = Config {
    file: "data/main.bin",
    num_sequences: 78,
    num_instruments: 43,
};

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
        // Cheap way of ensuring GUI catches the sounds finishing,
        // without having the sound-players hold a reference to the
        // GUI.
        ctx.request_repaint_after(Duration::from_millis(100));
    }
}

fn main() {
    let args = Args::parse();

    let conf = match args.bank {
        Bank::Intro => INTRO_CONF,
        Bank::Game => GAME_CONF,
    };

    let data = std::fs::read(conf.file).unwrap();
    let sound_bank = sound_player::SoundBank::new(data, conf.num_sequences, conf.num_instruments);
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
