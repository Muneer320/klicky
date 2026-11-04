//! Windows transaction preparation; the existing Unix loader is unchanged.
use std::io::{Cursor, Read};

use anyhow::bail;
use symphonia::core::codecs::CODEC_TYPE_VORBIS;
use symphonia::core::errors::Error;

use super::*;

const MAX_JSON: usize = 1024 * 1024;
const MAX_COMPRESSED: usize = 32 * 1024 * 1024;
const MAX_PCM_SAMPLES: usize = 256 * 1024 * 1024 / size_of::<i16>();
const MAX_DEFINITIONS: usize = 512;

impl SoundPack {
    /// Blocking and cancellable between file chunks, pages, packets and slices.
    /// Cancellation cannot forcibly interrupt an OS file read or codec call.
    pub(crate) fn load_bounded(dir: &Path, cancelled: &impl Fn() -> bool) -> Result<Self> {
        let config: SoundConfig = serde_json::from_slice(&read_bounded(
            &dir.join("config.json"),
            MAX_JSON,
            cancelled,
        )?)?;
        if config.defines.is_empty() || config.defines.len() > MAX_DEFINITIONS {
            bail!("sound pack must define between 1 and 512 keys");
        }
        if config
            .defines
            .keys()
            .any(|key| key.is_empty() || key.len() > 64)
        {
            bail!("sound-pack key names must contain 1 to 64 bytes");
        }
        let encoded = read_bounded(&dir.join("sound.ogg"), MAX_COMPRESSED, cancelled)?;
        validate_ogg(&encoded, cancelled)?;
        let (pcm, channels, sample_rate) = decode(encoded, cancelled)?;
        let samples_per_ms = f64::from(sample_rate) / 1000.0 * f64::from(channels);
        let mut samples = HashMap::new();
        let mut prepared = 0usize;
        for (name, [start_ms, duration_ms]) in config.defines {
            check(cancelled)?;
            let start = (start_ms as f64 * samples_per_ms) as usize;
            let length = (duration_ms as f64 * samples_per_ms) as usize;
            let end = start.saturating_add(length).min(pcm.len());
            if start >= end {
                bail!("sound-pack key {name:?} has an empty or out-of-range sample");
            }
            prepared = prepared
                .checked_add(end - start)
                .context("prepared PCM size overflow")?;
            if prepared > MAX_PCM_SAMPLES {
                bail!("prepared sound-pack samples exceed 256 MiB");
            }
            let mut slice = Vec::new();
            slice.try_reserve_exact(end - start)?;
            slice.extend_from_slice(&pcm[start..end]);
            normalize_samples(&mut slice);
            samples.insert(name, Arc::new(slice));
        }
        check(cancelled)?;
        Ok(Self {
            samples,
            channels,
            sample_rate,
        })
    }
}

fn check(cancelled: &impl Fn() -> bool) -> Result<()> {
    if cancelled() {
        bail!("sound-pack preparation cancelled or expired");
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: usize, cancelled: &impl Fn() -> bool) -> Result<Vec<u8>> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        check(cancelled)?;
        let count = file.read(&mut chunk)?;
        if count == 0 {
            return Ok(bytes);
        }
        if count > limit.saturating_sub(bytes.len()) {
            bail!(
                "{} exceeds the {}-byte sound-pack limit",
                path.display(),
                limit
            );
        }
        bytes.try_reserve(count)?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}

// Require one complete logical Ogg stream. CRC and EOS validation keep a
// truncated/corrupt stream from being accepted as the decoder's ordinary EOF.
fn validate_ogg(bytes: &[u8], cancelled: &impl Fn() -> bool) -> Result<()> {
    let mut offset = 0usize;
    let mut stream = None;
    let mut sequence = 0u32;
    let mut ended = false;
    let mut continued = false;
    while offset < bytes.len() {
        check(cancelled)?;
        let header = bytes
            .get(offset..offset + 27)
            .context("truncated Ogg page header")?;
        if &header[..4] != b"OggS" || header[4] != 0 || header[5] & !7 != 0 || ended {
            bail!("invalid or chained Ogg stream");
        }
        if (header[5] & 1 != 0) != continued {
            bail!("discontinuous Ogg packet continuation");
        }
        let serial = u32::from_le_bytes(header[14..18].try_into()?);
        let number = u32::from_le_bytes(header[18..22].try_into()?);
        if stream.is_none() {
            if header[5] & 2 == 0 || header[5] & 1 != 0 {
                bail!("Ogg stream lacks a valid first page");
            }
            stream = Some(serial);
            sequence = number;
        } else if stream != Some(serial) || header[5] & 2 != 0 {
            bail!("multiple Ogg streams are unsupported");
        }
        if number != sequence {
            bail!("discontinuous Ogg page sequence");
        }
        sequence = sequence.wrapping_add(1);
        let table_end = offset + 27 + usize::from(header[26]);
        let table = bytes
            .get(offset + 27..table_end)
            .context("truncated Ogg segment table")?;
        let payload: usize = table.iter().map(|value| usize::from(*value)).sum();
        let end = table_end + payload;
        let page = bytes
            .get(offset..end)
            .context("truncated Ogg page payload")?;
        let expected = u32::from_le_bytes(header[22..26].try_into()?);
        let mut crc = 0u32;
        for (index, byte) in page.iter().enumerate() {
            crc ^= u32::from(if (22..26).contains(&index) { 0 } else { *byte }) << 24;
            for _ in 0..8 {
                crc = (crc << 1)
                    ^ if crc & 0x8000_0000 != 0 {
                        0x04c1_1db7
                    } else {
                        0
                    };
            }
        }
        if crc != expected {
            bail!("Ogg page checksum mismatch");
        }
        if let Some(last) = table.last() {
            continued = *last == 255;
        }
        ended = header[5] & 4 != 0;
        if ended && continued {
            bail!("unfinished final Ogg packet");
        }
        offset = end;
    }
    if !ended {
        bail!("Ogg stream has no complete end page");
    }
    Ok(())
}

fn decode(bytes: Vec<u8>, cancelled: &impl Fn() -> bool) -> Result<(Vec<i16>, u16, u32)> {
    check(cancelled)?;
    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("ogg");
    let probed = symphonia::default::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    )?;
    let mut format = probed.format;
    if format.tracks().len() != 1 {
        bail!("sound pack must contain one Vorbis track");
    }
    let track = format
        .default_track()
        .context("sound pack has no default track")?;
    let id = track.id;
    let params = track.codec_params.clone();
    if params.codec != CODEC_TYPE_VORBIS {
        bail!("Windows sound packs require Ogg Vorbis");
    }
    let rate = params
        .sample_rate
        .context("sound pack has no sample rate")?;
    let channels = params
        .channels
        .context("sound pack has no channel layout")?
        .count() as u16;
    if !(8000..=192000).contains(&rate) || !(1..=2).contains(&channels) {
        bail!("sound pack requires mono/stereo PCM at 8 to 192 kHz");
    }
    let mut decoder = symphonia::default::get_codecs().make(&params, &DecoderOptions::default())?;
    let mut samples = Vec::new();
    loop {
        check(cancelled)?;
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(error) => return Err(error.into()),
        };
        if packet.track_id() != id {
            bail!("unexpected sound-pack track");
        }
        let decoded = decoder.decode(&packet)?;
        if decoded.spec().rate != rate || decoded.spec().channels.count() != usize::from(channels) {
            bail!("sound-pack audio format changed during decoding");
        }
        let capacity = decoded
            .capacity()
            .checked_mul(usize::from(channels))
            .context("PCM capacity overflow")?;
        if capacity > MAX_PCM_SAMPLES.saturating_sub(samples.len()) {
            bail!("decoded sound pack exceeds 256 MiB");
        }
        let mut buffer = SampleBuffer::<i16>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        let required = samples.len() + buffer.samples().len();
        if required > samples.capacity() {
            let capacity = samples
                .capacity()
                .saturating_mul(2)
                .max(required)
                .min(MAX_PCM_SAMPLES);
            samples.try_reserve_exact(capacity - samples.len())?;
        }
        samples.extend_from_slice(buffer.samples());
    }
    if samples.is_empty() {
        bail!("sound pack contains no decoded audio");
    }
    check(cancelled)?;
    Ok((samples, channels, rate))
}
