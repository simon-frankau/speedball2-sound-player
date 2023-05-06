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


fn main() {
    let args = Args::parse();

    let name = args.file;
    let data = std::fs::read(name).unwrap();
    let sound_bank = sound_model::SoundBank::new(data);

    println!("{:?}", sound_bank);

    let options = NativeOptions::default();
    let app = PlayerApp {
	bank: Arc::new(Mutex::new(sound_bank)),
    };
    eframe::run_native(
        "Speedball II Sound Player",
        options,
        Box::new(|_cc| Box::new(app)),
    )
    .unwrap();
}
