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
use egui::{Button, CentralPanel, Color32, Context};

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
    channel: Arc<Mutex<sound_player::SoundChannel>>,
}

impl PlayerApp {
    fn new(bank: sound_player::SoundBank) -> PlayerApp {
        let bank = Arc::new(Mutex::new(bank));
        let channel = Arc::new(Mutex::new(sound_player::SoundChannel::new(bank.clone())));
        PlayerApp { bank, channel }
    }
}

impl App for PlayerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        CentralPanel::default().show(ctx, |ui| {
            let mut channel = self.channel.lock().unwrap();
	    if ui
                .add(Button::new("Stop").fill(Color32::DARK_RED))
                .clicked()
            {
                channel.stop();
            }
            self.bank.lock().unwrap().ui(ui, &mut channel);
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
    let _stream = cpal_wrapper::sound_init(app.channel.clone());

    eframe::run_native(
        "Speedball II Sound Player",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .unwrap();
}
