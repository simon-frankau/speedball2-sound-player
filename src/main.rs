//
// Speedball 2 Sound player
//
// main.rs: Main entry point.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::path::PathBuf;

use clap::Parser;

mod sound_model;

/// Player of Speedball II sounds
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The file to load sound data from.
    #[arg(long)]
    file: PathBuf,
}

fn main() {
    let args = Args::parse();

    let name = args.file;
    let data = std::fs::read(name).unwrap();
    let sound_bank = sound_model::SoundBank::new(data);

    println!("{:?}", sound_bank);
}
