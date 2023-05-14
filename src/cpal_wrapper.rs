//
// Speedball 2 Sound player
//
// cpal_wrapper.rs: Encapsulate all the CPAL stuff, give me a simple interface.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::fs::File;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};

use rfd::FileDialog;

use wav::{bit_depth::BitDepth, header, Header};

pub trait SoundSource {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    );

    // Once the stream ends, this should return true, although
    // fill_buffer should continue to work.
    fn stream_done(&self) -> bool;
}

// Given a sound source, play it to speakers.
pub fn sound_init<S>(source: Arc<Mutex<S>>) -> Stream
where
    S: SoundSource + Send + 'static,
{
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
                source
                    .lock()
                    .unwrap()
                    .fill_buffer::<f32>(num_channels, sample_rate, data);
            },
            err_fn,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _info: &cpal::OutputCallbackInfo| {
                source
                    .lock()
                    .unwrap()
                    .fill_buffer::<i16>(num_channels, sample_rate, data);
            },
            err_fn,
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _info: &cpal::OutputCallbackInfo| {
                source
                    .lock()
                    .unwrap()
                    .fill_buffer::<u16>(num_channels, sample_rate, data);
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

// Given a sound source, and a config, write it to a .wav file.
pub fn write_wav<Source>(source: &mut Source, stereo: bool, max_time_s: f32)
where
    Source: SoundSource + Send + 'static,
{
    let file_name = FileDialog::new()
        .add_filter("Wave", &["wav"])
        .set_file_name("speedball2.wav")
        .save_file();

    if let Some(name) = file_name {
        let num_channels = if stereo { 2 } else { 1 };
        // Everyone loves CD quality. :p
        const SAMPLING_RATE: u32 = 44_100;
        const BITS_PER_SAMPLE: u16 = 16;
        let header = Header::new(
            header::WAV_FORMAT_PCM,
            num_channels,
            SAMPLING_RATE,
            BITS_PER_SAMPLE,
        );
        let max_samples = (max_time_s * SAMPLING_RATE as f32 * num_channels as f32) as usize;
        // Choose a size that isn't too much overhead, but means we
        // don't chuck in too much unnecesary silence.`
        const BATCH_SIZE: usize = 441;
        let batch = BATCH_SIZE * num_channels as usize;
        let mut data: Vec<i16> = Vec::new();
        while data.len() < max_samples && source.stream_done() {
            let old_len = data.len();
            data.resize(old_len + batch, 0);
            source.fill_buffer(num_channels, SAMPLING_RATE, &mut data[old_len..]);
        }
        let mut out_file =
            File::create(&name).expect(&format!("Couldn't create file '{}'", name.display()));
        wav::write(header, &BitDepth::Sixteen(data), &mut out_file)
            .expect("Couldn't write wav file");
    }
}
