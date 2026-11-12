use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rodio::cpal::traits::HostTrait;
use rodio::cpal::BufferSize;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};

pub(crate) const BUFFER_SIZES: [u32; 3] = [256, 512, 1024];

pub struct Player {
    sink: MixerDeviceSink,
    volume: f32,
    generation: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct PcmSource {
    samples: Arc<Vec<i16>>,
    channels: NonZeroU16,
    sample_rate: NonZeroU32,
    pos: usize,
    cancellation: Option<(Arc<AtomicU64>, u64)>,
}

impl PcmSource {
    pub(crate) fn new(samples: &Arc<Vec<i16>>, channels: u16, sample_rate: u32) -> Option<Self> {
        Some(Self {
            samples: Arc::clone(samples),
            channels: NonZeroU16::new(channels)?,
            sample_rate: NonZeroU32::new(sample_rate)?,
            pos: 0,
            cancellation: None,
        })
    }
}

impl Iterator for PcmSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|(generation, expected)| generation.load(Ordering::Relaxed) != *expected)
        {
            return None;
        }
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
        Ok(Self {
            sink,
            volume: 0.8,
            generation: Arc::new(AtomicU64::new(0)),
        })
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
        let Some(mut source) = PcmSource::new(samples, channels, sample_rate) else {
            return;
        };
        source.cancellation = Some((
            Arc::clone(&self.generation),
            self.generation.load(Ordering::Relaxed),
        ));
        self.sink.mixer().add(source.amplify(volume));
    }

    /// End queued and currently mixed sounds without reopening the device.
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clearing_sounds_ends_active_and_queued_sources_but_allows_new_sounds() {
        let samples = Arc::new(vec![1000; 32]);
        let generation = Arc::new(AtomicU64::new(0));
        let mut active = PcmSource::new(&samples, 1, 48000).unwrap();
        active.cancellation = Some((Arc::clone(&generation), 0));
        let mut queued = active.clone();
        assert!(active.next().is_some());
        generation.fetch_add(1, Ordering::Relaxed);
        assert!(active.next().is_none());
        assert!(queued.next().is_none());
        let mut new = PcmSource::new(&samples, 1, 48000).unwrap();
        new.cancellation = Some((generation, 1));
        assert!(new.next().is_some());
    }
}
