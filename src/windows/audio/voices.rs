use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use rodio::{mixer::Mixer, Source};

use super::{Render, Shared};
use crate::player::PcmSource;

pub const MAX_VOICES: usize = 64;

pub struct Output {
    pub description: String,
    pub(super) mixer: Mixer,
    pub(super) render: Arc<Render>,
}
impl Output {
    pub fn is_available(&self) -> bool {
        self.render.valid.load(Ordering::Acquire)
            && shared_live(&self.render.shared, self.render.epoch)
    }

    pub fn set_enabled(&self, enabled: bool) {
        if self.render.enabled.swap(enabled, Ordering::AcqRel) != enabled {
            self.render.generation.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn discard_voices(&self) {
        self.render.generation.fetch_add(1, Ordering::AcqRel);
    }

    pub fn play(&self, samples: &Arc<Vec<i16>>, channels: u16, rate: u32, volume: f32) -> bool {
        if !self.render.live() || !volume.is_finite() || volume <= 0.0 {
            return false;
        }
        let Some(source) = PcmSource::new(samples, channels, rate) else {
            return false;
        };
        if self
            .render
            .voices
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_VOICES).then_some(count + 1)
            })
            .is_err()
        {
            self.render.voice_drops.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        self.mixer.add(Voice {
            source,
            volume: volume.min(1.0),
            generation: self.render.generation.load(Ordering::Acquire),
            render: Arc::clone(&self.render),
            until_check: 0,
        });
        true
    }

    pub fn voice_drops(&self) -> u64 {
        self.render.voice_drops.load(Ordering::Relaxed)
    }
}

struct Voice {
    source: PcmSource,
    volume: f32,
    generation: u64,
    render: Arc<Render>,
    until_check: u8,
}
impl Iterator for Voice {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.until_check == 0 {
            if !self.render.live()
                || self.generation != self.render.generation.load(Ordering::Acquire)
            {
                return None;
            }
            self.until_check = 127;
        } else {
            self.until_check -= 1;
        }
        self.source.next().map(|sample| sample * self.volume)
    }
}
impl Drop for Voice {
    fn drop(&mut self) {
        self.render.voices.fetch_sub(1, Ordering::AcqRel);
    }
}
impl Source for Voice {
    fn channels(&self) -> NonZeroU16 {
        self.source.channels()
    }
    fn sample_rate(&self) -> NonZeroU32 {
        self.source.sample_rate()
    }
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
}

pub(super) struct Heartbeat {
    pub render: Arc<Render>,
    pub channels: NonZeroU16,
    pub rate: NonZeroU32,
    pub until_tick: u8,
}
impl Iterator for Heartbeat {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.until_tick == 0 {
            self.render.heartbeat.fetch_add(1, Ordering::Relaxed);
            self.until_tick = 127;
        } else {
            self.until_tick -= 1;
        }
        Some(0.0)
    }
}
impl Source for Heartbeat {
    fn channels(&self) -> NonZeroU16 {
        self.channels
    }
    fn sample_rate(&self) -> NonZeroU32 {
        self.rate
    }
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

pub(super) fn shared_live(shared: &Shared, epoch: u64) -> bool {
    shared.active.load(Ordering::Acquire)
        && !shared.stop.load(Ordering::Acquire)
        && shared.epoch.load(Ordering::Acquire) == epoch
}
