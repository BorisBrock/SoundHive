//! Decodes a sound file once and measures what the library shows and filters by: duration,
//! levels, a waveform, brightness, attack time and whether it loops.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use symphonia::core::audio::{SampleBuffer, SignalSpec};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;

/// File extensions the library picks up.
pub const EXTENSIONS: &[&str] = &["wav", "wave", "mp3", "ogg", "oga", "flac", "aif", "aiff", "m4a"];

/// Number of values in a stored waveform.
pub const WAVEFORM_BUCKETS: usize = 400;

/// Longer files are only analysed up to here; their duration still comes from the header.
const MAX_ANALYSIS_SECONDS: u64 = 15 * 60;

/// Bumped when analysis changes, so that the next scan analyses every file again.
pub const ANALYSIS_VERSION: i64 = 1;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub bits: u32,
    pub peak_db: f64,
    pub rms_db: f64,
    pub zcr: f64,
    pub attack: f64,
    /// `marked` or `seamless`; loops by name are found by the tagger.
    pub loop_kind: Option<String>,
    pub description: String,
    pub waveform: Vec<u8>,
}

pub fn is_sound_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Floor for levels in dB, so that digital silence does not become -inf.
const MIN_DB: f64 = -120.0;

fn to_db(amplitude: f64) -> f64 {
    if amplitude <= 1e-6 { MIN_DB } else { (20.0 * amplitude.log10()).max(MIN_DB) }
}

/// Collects measurements while samples stream through; nothing but the envelope is kept.
struct Meter {
    channels: usize,
    sample_rate: u32,
    frames: u64,
    peak: f32,
    sum_squares: f64,
    zero_crossings: u64,
    last_sign: bool,
    /// Peak of every `block` frames.
    envelope: Vec<f32>,
    block: u64,
    block_peak: f32,
    block_fill: u64,
    /// First and last frames (mono mix) for the loop check.
    head: Vec<f32>,
    tail: std::collections::VecDeque<f32>,
    edge: usize,
}

impl Meter {
    fn new(spec: SignalSpec, expected_frames: Option<u64>) -> Self {
        let sample_rate = spec.rate.max(1);
        // Fine enough to measure attack times and to draw the waveform from.
        let block = match expected_frames {
            Some(n) if n > 0 => (n / (WAVEFORM_BUCKETS as u64 * 8)).clamp(1, 4096),
            _ => 256,
        };
        let edge = (sample_rate as usize / 100).max(16); // 10 ms
        Meter {
            channels: spec.channels.count().max(1),
            sample_rate,
            frames: 0,
            peak: 0.0,
            sum_squares: 0.0,
            zero_crossings: 0,
            last_sign: false,
            envelope: Vec::new(),
            block,
            block_peak: 0.0,
            block_fill: 0,
            head: Vec::with_capacity(edge),
            tail: std::collections::VecDeque::with_capacity(edge + 1),
            edge,
        }
    }

    fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(self.channels) {
            let mut frame_peak = 0f32;
            let mut mono = 0f32;
            for &s in frame {
                let s = if s.is_finite() { s } else { 0.0 };
                frame_peak = frame_peak.max(s.abs());
                mono += s;
                self.sum_squares += (s as f64) * (s as f64);
            }
            mono /= self.channels as f32;
            let sign = mono >= 0.0;
            if self.frames > 0 && sign != self.last_sign && mono.abs() > 1e-4 {
                self.zero_crossings += 1;
            }
            if mono.abs() > 1e-4 {
                self.last_sign = sign;
            }
            self.peak = self.peak.max(frame_peak);
            self.block_peak = self.block_peak.max(frame_peak);
            self.block_fill += 1;
            if self.block_fill == self.block {
                self.envelope.push(self.block_peak);
                self.block_peak = 0.0;
                self.block_fill = 0;
            }
            if self.head.len() < self.edge {
                self.head.push(mono);
            }
            self.tail.push_back(mono);
            if self.tail.len() > self.edge {
                self.tail.pop_front();
            }
            self.frames += 1;
        }
    }

    fn finish(mut self, header_frames: Option<u64>) -> Analysis {
        if self.block_fill > 0 {
            self.envelope.push(self.block_peak);
        }
        let rate = self.sample_rate as f64;
        let frames = header_frames.filter(|&n| n > 0).unwrap_or(self.frames);
        let analysed = self.frames.max(1) as f64;
        let rms = (self.sum_squares / (analysed * self.channels as f64)).sqrt();
        let block_secs = self.block as f64 / rate;

        // Attack: from the first audible block to the first one near the peak.
        let peak = self.peak;
        let onset = self.envelope.iter().position(|&p| p >= peak * 0.05).unwrap_or(0);
        let top = self.envelope.iter().position(|&p| p >= peak * 0.8).unwrap_or(onset);
        let attack = top.saturating_sub(onset) as f64 * block_secs;

        let loop_kind = self.seamless(rms as f32).then(|| "seamless".to_string());

        Analysis {
            duration: frames as f64 / rate,
            sample_rate: self.sample_rate,
            channels: self.channels as u32,
            bits: 0,
            peak_db: to_db(peak as f64),
            rms_db: to_db(rms),
            zcr: self.zero_crossings as f64 / (analysed / rate),
            attack,
            loop_kind,
            description: String::new(),
            waveform: resample_envelope(&self.envelope, peak),
        }
    }

    /// Whether the end runs into the start without a click or a fade: both ends are about as
    /// loud as the sound on average, and the last sample is close to the first.
    fn seamless(&self, rms: f32) -> bool {
        let duration = self.frames as f64 / self.sample_rate as f64;
        if duration < 0.5 || rms < 0.003 || self.head.len() < self.edge || self.tail.len() < self.edge {
            return false;
        }
        let level = |s: &mut dyn Iterator<Item = f32>| {
            let (sum, n) = s.fold((0f32, 0usize), |(a, n), x| (a + x * x, n + 1));
            (sum / n.max(1) as f32).sqrt()
        };
        let head = level(&mut self.head.iter().copied());
        let tail = level(&mut self.tail.iter().copied());
        let first = self.head[0];
        let last = *self.tail.back().unwrap_or(&0.0);
        head > rms * 0.25
            && tail > rms * 0.25
            && (head / tail).max(tail / head) < 4.0
            && (last - first).abs() < self.peak.max(1e-6) * 0.15
    }
}

/// Reduces the fine envelope to `WAVEFORM_BUCKETS` peaks, scaled so the loudest is 255.
fn resample_envelope(envelope: &[f32], peak: f32) -> Vec<u8> {
    if envelope.is_empty() || peak <= 0.0 {
        return vec![0; WAVEFORM_BUCKETS];
    }
    (0..WAVEFORM_BUCKETS)
        .map(|i| {
            let from = i * envelope.len() / WAVEFORM_BUCKETS;
            let to = ((i + 1) * envelope.len() / WAVEFORM_BUCKETS).max(from + 1).min(envelope.len());
            let p = envelope[from.min(envelope.len() - 1)..to].iter().fold(0f32, |a, &b| a.max(b));
            ((p / peak).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

/// Decodes and measures a sound file.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&ext);
    let format_opts = FormatOptions { enable_gapless: true, ..Default::default() };
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &MetadataOptions::default())
        .map_err(|e| format!("unsupported format: {e}"))?;

    let mut description = String::new();
    let mut marked_loop = false;
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        read_tags(rev, &mut description, &mut marked_loop);
    }
    let mut format = probed.format;
    if let Some(rev) = format.metadata().current() {
        read_tags(rev, &mut description, &mut marked_loop);
    }

    let track =
        format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("no audio track")?;
    let track_id = track.id;
    let params = track.codec_params.clone();
    let header_frames = params.n_frames;
    let bits = params.bits_per_sample.unwrap_or(0);
    let mut decoder = symphonia::default::get_codecs()
        .make(&params, &DecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;

    let mut meter: Option<Meter> = None;
    let mut buffer: Option<SampleBuffer<f32>> = None;
    let mut fallback_spec = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => {
                // A broken tail is common; keep what was decoded so far.
                if meter.as_ref().is_some_and(|m| m.frames > 0) {
                    break;
                }
                return Err(e.to_string());
            }
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        let spec = *decoded.spec();
        fallback_spec.get_or_insert(spec);
        let needed = decoded.capacity() * spec.channels.count();
        if buffer.as_ref().is_none_or(|b| b.capacity() < needed) {
            buffer = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let buf = buffer.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        let m = meter.get_or_insert_with(|| Meter::new(spec, header_frames));
        m.push(buf.samples());
        if m.frames > MAX_ANALYSIS_SECONDS * m.sample_rate as u64 {
            break;
        }
    }

    let meter = match meter {
        Some(m) => m,
        None => match (params.sample_rate, params.channels) {
            // Header but no audio, e.g. an empty WAV.
            (Some(rate), Some(ch)) => Meter::new(SignalSpec::new(rate, ch), header_frames),
            _ => return Err("no audio data".into()),
        },
    };
    let mut analysis = meter.finish(header_frames);
    analysis.bits = bits;
    if matches!(ext.as_str(), "wav" | "wave")
        && let Ok(riff) = read_riff_extras(path)
    {
        marked_loop |= riff.has_loop;
        if description.is_empty() {
            description = riff.description;
        }
    }
    if marked_loop {
        analysis.loop_kind = Some("marked".into());
    }
    analysis.description = description.split_whitespace().collect::<Vec<_>>().join(" ");
    Ok(analysis)
}

/// Decoded audio: interleaved samples in -1..=1.
pub struct Pcm {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

/// Decodes a whole sound file, up to `max_seconds`; longer files are an error.
pub fn decode(path: &Path, max_seconds: u64) -> Result<Pcm, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(&ext);
    let format_opts = FormatOptions { enable_gapless: true, ..Default::default() };
    let mut format = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &MetadataOptions::default())
        .map_err(|e| format!("unsupported format: {e}"))?
        .format;
    let track =
        format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).ok_or("no audio track")?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("unsupported codec: {e}"))?;

    let mut spec: Option<SignalSpec> = None;
    let mut buffer: Option<SampleBuffer<f32>> = None;
    let mut samples = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(SymError::ResetRequired) => break,
            // A broken tail is common; keep what was decoded so far.
            Err(_) if !samples.is_empty() => break,
            Err(e) => return Err(e.to_string()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) | Err(SymError::IoError(_)) => continue,
            Err(e) => return Err(e.to_string()),
        };
        let s = *spec.get_or_insert(*decoded.spec());
        let needed = decoded.capacity() * s.channels.count();
        if buffer.as_ref().is_none_or(|b| b.capacity() < needed) {
            buffer = Some(SampleBuffer::new(decoded.capacity() as u64, s));
        }
        let buf = buffer.as_mut().unwrap();
        buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buf.samples());
        let channels = s.channels.count().max(1) as u64;
        if samples.len() as u64 > max_seconds * s.rate as u64 * channels {
            return Err(format!("longer than {} minutes", max_seconds / 60));
        }
    }
    let spec = spec.ok_or("no audio data")?;
    Ok(Pcm { sample_rate: spec.rate, channels: spec.channels.count() as u16, samples })
}

/// Picks a description and game-engine loop markers (`LOOPSTART`, as used by RPG Maker and
/// others) from a file's tags.
fn read_tags(rev: &MetadataRevision, description: &mut String, marked_loop: &mut bool) {
    for tag in rev.tags() {
        let key = tag.key.to_ascii_uppercase();
        if matches!(key.as_str(), "LOOPSTART" | "LOOP_START" | "LOOPLENGTH" | "LOOP_END" | "LOOPEND") {
            *marked_loop = true;
        }
        let text = matches!(
            tag.std_key,
            Some(StandardTagKey::TrackTitle | StandardTagKey::Comment | StandardTagKey::Description)
        );
        if text {
            let value = tag.value.to_string();
            let value = value.trim();
            if !value.is_empty() && !description.contains(value) && description.len() < 500 {
                if !description.is_empty() {
                    description.push_str(" · ");
                }
                description.push_str(value);
            }
        }
    }
}

#[derive(Debug, Default)]
struct RiffExtras {
    has_loop: bool,
    description: String,
}

/// Reads loop points (`smpl`) and the broadcast-wave description (`bext`) that symphonia skips.
/// Only chunk headers are read; audio data is skipped over.
fn read_riff_extras(path: &Path) -> std::io::Result<RiffExtras> {
    let mut f = File::open(path)?;
    let mut header = [0u8; 12];
    f.read_exact(&mut header)?;
    let mut out = RiffExtras::default();
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Ok(out);
    }
    let len = f.metadata()?.len();
    let mut pos = 12u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos))?;
        let mut ch = [0u8; 8];
        f.read_exact(&mut ch)?;
        let size = u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]) as u64;
        match &ch[0..4] {
            b"smpl" if size >= 36 => {
                let mut body = [0u8; 36];
                f.read_exact(&mut body)?;
                let loops = u32::from_le_bytes([body[28], body[29], body[30], body[31]]);
                out.has_loop |= loops > 0;
            }
            b"bext" if size >= 256 => {
                let mut body = [0u8; 256];
                f.read_exact(&mut body)?;
                let end = body.iter().position(|&b| b == 0).unwrap_or(256);
                out.description = String::from_utf8_lossy(&body[..end]).trim().to_string();
            }
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    Ok(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::Write;

    /// Writes a 16-bit PCM WAV file; `extra` is appended as raw chunk bytes.
    pub fn write_wav(path: &Path, rate: u32, channels: u16, samples: &[f32], extra: &[u8]) {
        let data: Vec<u8> =
            samples.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
        let mut f = File::create(path).unwrap();
        let riff_len = 4 + (8 + 16) + (8 + data.len()) + extra.len();
        f.write_all(b"RIFF").unwrap();
        f.write_all(&(riff_len as u32).to_le_bytes()).unwrap();
        f.write_all(b"WAVEfmt ").unwrap();
        f.write_all(&16u32.to_le_bytes()).unwrap();
        f.write_all(&1u16.to_le_bytes()).unwrap();
        f.write_all(&channels.to_le_bytes()).unwrap();
        f.write_all(&rate.to_le_bytes()).unwrap();
        f.write_all(&(rate * channels as u32 * 2).to_le_bytes()).unwrap();
        f.write_all(&(channels * 2).to_le_bytes()).unwrap();
        f.write_all(&16u16.to_le_bytes()).unwrap();
        f.write_all(b"data").unwrap();
        f.write_all(&(data.len() as u32).to_le_bytes()).unwrap();
        f.write_all(&data).unwrap();
        f.write_all(extra).unwrap();
    }

    pub fn sine(rate: u32, seconds: f32, freq: f32, envelope: impl Fn(f32) -> f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / rate as f32;
                (t * freq * std::f32::consts::TAU).sin() * envelope(t)
            })
            .collect()
    }

    #[test]
    fn measures_a_decaying_hit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hit.wav");
        let samples = sine(44100, 1.0, 220.0, |t| (-t * 8.0).exp() * 0.5);
        write_wav(&path, 44100, 1, &samples, &[]);
        let a = analyze(&path).unwrap();
        assert!((a.duration - 1.0).abs() < 0.01, "{}", a.duration);
        assert_eq!((a.sample_rate, a.channels, a.bits), (44100, 1, 16));
        assert!((a.peak_db - -6.0).abs() < 0.5, "{}", a.peak_db);
        assert!(a.rms_db < a.peak_db - 6.0);
        assert!(a.attack < 0.02, "{}", a.attack);
        assert!((a.zcr - 440.0).abs() < 30.0, "{}", a.zcr);
        assert_eq!(a.loop_kind, None);
        assert_eq!(a.waveform.len(), WAVEFORM_BUCKETS);
        assert_eq!(a.waveform[0], 255);
        assert!(a.waveform[WAVEFORM_BUCKETS - 1] < 10);
    }

    #[test]
    fn steady_tone_with_whole_periods_is_seamless() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hum.wav");
        // 100 Hz for exactly 2 s: the end meets the start.
        let samples = sine(48000, 2.0, 100.0, |_| 0.3);
        let stereo: Vec<f32> = samples.iter().flat_map(|&s| [s, s]).collect();
        write_wav(&path, 48000, 2, &stereo, &[]);
        let a = analyze(&path).unwrap();
        assert_eq!(a.channels, 2);
        assert_eq!(a.loop_kind.as_deref(), Some("seamless"));
    }

    #[test]
    fn reads_loop_points_and_description() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("marked.wav");
        let mut extra = Vec::new();
        extra.extend_from_slice(b"smpl");
        extra.extend_from_slice(&60u32.to_le_bytes());
        let mut smpl = vec![0u8; 60];
        smpl[28..32].copy_from_slice(&1u32.to_le_bytes());
        extra.extend_from_slice(&smpl);
        extra.extend_from_slice(b"bext");
        extra.extend_from_slice(&602u32.to_le_bytes());
        let mut bext = vec![0u8; 602];
        bext[..17].copy_from_slice(b"Big door slam 01 ");
        extra.extend_from_slice(&bext);
        let samples = sine(22050, 0.3, 300.0, |t| 1.0 - t * 3.0);
        write_wav(&path, 22050, 1, &samples, &extra);
        let a = analyze(&path).unwrap();
        assert_eq!(a.loop_kind.as_deref(), Some("marked"));
        assert_eq!(a.description, "Big door slam 01");
    }

    #[test]
    fn broken_files_report_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.wav");
        std::fs::write(&path, b"not a sound").unwrap();
        assert!(analyze(&path).is_err());
    }
}
