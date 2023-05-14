//
// Speedball 2 Sound player
//
// cpal_wrapper.rs: Encapsulate all the CPAL stuff, give me a simple interface.
//
// (C) Copyright 2023 Simon Frankau. All Rights Reserved, see LICENSE.
//

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, Stream};

pub trait SoundSource {
    fn fill_buffer<T: Sample + cpal::FromSample<f32> + std::ops::Add<Output = T>>(
        &mut self,
        num_channels: u16,
        sample_rate: u32,
        data: &mut [T],
    );
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
