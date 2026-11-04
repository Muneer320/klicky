//! Retain Rodio's mixer/PCM conversion while using the fixed Windows renderer.
use std::num::{NonZeroU16, NonZeroU32};

use anyhow::{bail, Context, Result};
use cpal_windows::traits::{DeviceTrait, StreamTrait};
use cpal_windows::{
    BufferSize, Device, FromSample, SampleFormat, SizedSample, Stream, StreamConfig,
};
use rodio::mixer::{mixer, Mixer, MixerSource};

pub(super) struct Sink {
    // Drop/join the renderer before releasing the remaining mixer handle.
    _stream: Stream,
    mixer: Mixer,
    channels: NonZeroU16,
    rate: NonZeroU32,
    description: String,
}

impl Sink {
    pub fn open(
        device: &Device,
        buffer: BufferSize,
        errors: impl FnMut(cpal_windows::Error) + Send + 'static,
    ) -> Result<Self> {
        let default = device.default_output_config()?;
        let format = default.sample_format();
        let mut config = default.config();
        config.buffer_size = buffer;
        let channels =
            NonZeroU16::new(config.channels).context("audio output has zero channels")?;
        let rate =
            NonZeroU32::new(config.sample_rate).context("audio output has zero sample rate")?;
        let (mixer, source) = mixer(channels, rate);
        let stream = match format {
            SampleFormat::F32 => build::<f32, _>(device, config, source, errors),
            SampleFormat::F64 => build::<f64, _>(device, config, source, errors),
            SampleFormat::I8 => build::<i8, _>(device, config, source, errors),
            SampleFormat::I16 => build::<i16, _>(device, config, source, errors),
            SampleFormat::I24 => build::<cpal_windows::I24, _>(device, config, source, errors),
            SampleFormat::I32 => build::<i32, _>(device, config, source, errors),
            SampleFormat::I64 => build::<i64, _>(device, config, source, errors),
            SampleFormat::U8 => build::<u8, _>(device, config, source, errors),
            SampleFormat::U16 => build::<u16, _>(device, config, source, errors),
            SampleFormat::U24 => build::<cpal_windows::U24, _>(device, config, source, errors),
            SampleFormat::U32 => build::<u32, _>(device, config, source, errors),
            SampleFormat::U64 => build::<u64, _>(device, config, source, errors),
            _ => bail!("unsupported default Windows PCM format: {format:?}"),
        }?;
        stream.play()?;
        Ok(Self {
            _stream: stream,
            mixer,
            channels,
            rate,
            description: format!(
                "{channels} channels, {rate} Hz, {format:?}; requested buffer {buffer:?}"
            ),
        })
    }

    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }
    pub fn channel_count(&self) -> NonZeroU16 {
        self.channels
    }
    pub fn sample_rate(&self) -> NonZeroU32 {
        self.rate
    }
    pub fn description(&self) -> &str {
        &self.description
    }
}

fn build<T, E>(
    device: &Device,
    config: StreamConfig,
    mut source: MixerSource,
    errors: E,
) -> Result<Stream>
where
    T: SizedSample + FromSample<f32>,
    E: FnMut(cpal_windows::Error) + Send + 'static,
{
    Ok(device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            fill(&mut source, data);
        },
        errors,
        None,
    )?)
}

fn fill<T: SizedSample + FromSample<f32>>(source: &mut MixerSource, data: &mut [T]) {
    for sample in data {
        *sample = source.next().map(T::from_sample).unwrap_or(T::EQUILIBRIUM);
    }
}
