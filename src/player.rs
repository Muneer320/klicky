use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rodio::cpal::traits::HostTrait;
use rodio::cpal::BufferSize;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

const BUFFER_SIZES: [u32; 3] = [256, 512, 1024];

pub struct Player {
    sink: MixerDeviceSink,
    volume: f32,
}

#[derive(Clone)]
struct PcmSource {
    samples: Arc<Vec<i16>>,
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    pos: usize,
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = self.samples.get(self.pos)?;
        self.pos += 1;
        Some(*sample as f32 / i16::MAX as f32)
    }
}

impl Source for PcmSource {
    fn channels(&self) -> NonZeroU16 {
        self.channels
    }

    fn sample_rate(&self) -> NonZeroU32 {
        self.sample_rate
    }

    fn current_span_len(&self) -> Option<usize> {
        Some(self.samples.len())
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Player {
    pub fn new() -> Result<Self> {
        let sink = open_low_latency_sink()?;
        eprintln!("[klicky] audio output: {:?}", sink.config());
        Ok(Self { sink, volume: 0.8 })
    }

    pub fn play(&mut self, samples: &Arc<Vec<i16>>, channels: u16, sample_rate: u32) {
        self.play_with_volume(samples, channels, sample_rate, self.volume);
    }

    pub fn play_with_volume(
        &mut self,
        samples: &Arc<Vec<i16>>,
        channels: u16,
        sample_rate: u32,
        volume: f32,
    ) {
        let Some(channels) = NonZeroU16::new(channels) else {
            return;
        };
        let Some(sample_rate) = NonZeroU32::new(sample_rate) else {
            return;
        };

        self.sink.mixer().add(
            PcmSource {
                samples: Arc::clone(samples),
                channels,
                sample_rate,
                pos: 0,
            }
            .amplify(volume),
        );
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }
}

fn open_low_latency_sink() -> Result<MixerDeviceSink> {
    let device = rodio::cpal::default_host()
        .default_output_device()
        .context("no default audio output device")?;

    for buffer_size in BUFFER_SIZES {
        let builder = DeviceSinkBuilder::from_device(device.clone())?
            .with_buffer_size(BufferSize::Fixed(buffer_size));
        if let Ok(sink) = builder.open_stream() {
            return Ok(sink);
        }
    }

    Ok(DeviceSinkBuilder::from_device(device)?.open_sink_or_fallback()?)
}
