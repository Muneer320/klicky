use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

pub struct Player {
    _stream: OutputStream,
    handle: OutputStreamHandle,
    volume: f32,
}

#[derive(Clone)]
struct PcmSource {
    samples: Arc<Vec<i16>>,
    channels: u16,
    sample_rate: u32,
    pos: usize,
}

impl Iterator for PcmSource {
    type Item = i16;
    fn next(&mut self) -> Option<i16> {
        let s = self.samples.get(self.pos)?;
        self.pos += 1;
        Some(*s)
    }
}

impl Source for PcmSource {
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn current_frame_len(&self) -> Option<usize> {
        Some(self.samples.len() - self.pos)
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        Ok(Player {
            _stream: stream,
            handle,
            volume: 0.8,
        })
    }

    pub fn play(&self, samples: &Arc<Vec<i16>>, channels: u16, sample_rate: u32) {
        self.play_with_volume(samples, channels, sample_rate, self.volume);
    }

    pub fn play_with_volume(&self, samples: &Arc<Vec<i16>>, channels: u16, sample_rate: u32, volume: f32) {
        let source = PcmSource {
            samples: Arc::clone(samples),
            channels,
            sample_rate,
            pos: 0,
        };

        if let Ok(sink) = Sink::try_new(&self.handle) {
            sink.set_volume(volume);
            sink.append(source);
            sink.detach();
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }
}
