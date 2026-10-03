use std::collections::HashMap;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use serde::Deserialize;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

#[derive(Debug, Deserialize)]
struct SoundConfig {
    defines: HashMap<String, [u64; 2]>,
    name: String,
}

pub struct SoundPack {
    pub name: String,
    pub samples: HashMap<String, Arc<Vec<i16>>>,
    pub channels: u16,
    pub sample_rate: u32,
}

impl SoundPack {
    pub fn load(dir: &Path) -> Result<Self> {
        let config_path = dir.join("config.json");
        let config: SoundConfig = serde_json::from_reader(
            File::open(&config_path).context("Failed to open config.json")?,
        )?;

        let ogg_path = dir.join("sound.ogg");
        let (all_samples, channels, sample_rate) = decode_ogg_fully(&ogg_path)?;

        let samples_per_ms = (sample_rate as f64 / 1000.0) * channels as f64;
        let mut key_samples = HashMap::new();

        for (key_name, [start_ms, duration_ms]) in &config.defines {
            let start_idx = (*start_ms as f64 * samples_per_ms) as usize;
            let len = (*duration_ms as f64 * samples_per_ms) as usize;
            let end_idx = (start_idx + len).min(all_samples.len());

            if start_idx < all_samples.len() {
                key_samples.insert(
                    key_name.clone(),
                    Arc::new(all_samples[start_idx..end_idx].to_vec()),
                );
            }
        }

        Ok(SoundPack {
            name: config.name,
            samples: key_samples,
            channels,
            sample_rate,
        })
    }
}

fn decode_ogg_fully(path: &Path) -> Result<(Vec<i16>, u16, u32)> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    hint.with_extension("ogg");

    let probed = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;

    let mut format = probed.format;
    let track = format.default_track().context("no default track")?;
    let codec_params = track.codec_params.clone();

    let sample_rate = codec_params.sample_rate.context("no sample rate")?;
    let channels = codec_params
        .channels
        .map(|c| c.count() as u16)
        .context("no channels")?;

    let mut decoder =
        symphonia::default::get_codecs().make(&codec_params, &DecoderOptions::default())?;

    let mut all_samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(_)) => break,
            Err(e) => return Err(e.into()),
        };

        let decoded = decoder.decode(&packet)?;
        let mut sample_buf = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        sample_buf.copy_interleaved_ref(decoded);
        all_samples.extend_from_slice(sample_buf.samples());
    }

    Ok((all_samples, channels, sample_rate))
}
