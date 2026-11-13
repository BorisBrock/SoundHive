//! Writes sounds in other formats, so that a library sound of any type can go into a project
//! file of a given type.
//!
//! WAV, AIFF, FLAC and Ogg Vorbis are encoded in-process. MP3 and M4A (AAC) are encoded with
//! FFmpeg if it is installed: their encoders cannot be built into an MIT-licensed app.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::num::{NonZeroU8, NonZeroU32};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::analysis::{self, Pcm};

/// Longer sounds are not converted (they would not fit in memory comfortably).
const MAX_CONVERT_SECONDS: u64 = 10 * 60;

/// Ogg Vorbis quality, -0.2..=1: 0.6 is about 190 kbit/s for stereo, transparent for most ears.
const VORBIS_QUALITY: f32 = 0.6;

/// Extension with spelling variants folded together.
pub fn format_of(path: &Path) -> String {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "wave" => "wav".into(),
        "aif" => "aiff".into(),
        "oga" => "ogg".into(),
        _ => ext,
    }
}

/// Formats encoded in-process.
const BUILT_IN: &[&str] = &["wav", "aiff", "flac", "ogg"];
/// Formats encoded with FFmpeg.
const WITH_FFMPEG: &[&str] = &["mp3", "m4a"];

fn ffmpeg() -> Option<PathBuf> {
    which::which("ffmpeg").ok()
}

/// The formats that any sound can be converted to on this machine.
pub fn writable_formats() -> Vec<String> {
    let mut formats: Vec<String> = BUILT_IN.iter().map(|f| f.to_string()).collect();
    if ffmpeg().is_some() {
        formats.extend(WITH_FFMPEG.iter().map(|f| f.to_string()));
    }
    formats
}

/// Converts `source` to `format` (as returned by [`format_of`]), written to `dest`.
pub fn convert(source: &Path, format: &str, dest: &Path) -> Result<(), String> {
    if WITH_FFMPEG.contains(&format) {
        return convert_with_ffmpeg(source, format, dest);
    }
    if !BUILT_IN.contains(&format) {
        return Err(format!("converting to {} is not supported", format.to_uppercase()));
    }
    let pcm = analysis::decode(source, MAX_CONVERT_SECONDS)?;
    let written = match format {
        "wav" => write_wav(dest, &pcm),
        "aiff" => write_aiff(dest, &pcm),
        "flac" => write_flac(dest, &pcm),
        _ => write_ogg(dest, &pcm),
    };
    written.map_err(|e| format!("could not write the sound: {e}"))
}

fn to_i16(s: f32) -> i16 {
    let s = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
    (s * 32767.0).round() as i16
}

fn frames(pcm: &Pcm) -> u64 {
    pcm.samples.len() as u64 / pcm.channels.max(1) as u64
}

/// Writes 16-bit PCM WAV.
fn write_wav(path: &Path, pcm: &Pcm) -> std::io::Result<()> {
    let channels = pcm.channels.max(1);
    let data_len = pcm.samples.len() as u64 * 2;
    if data_len + 36 > u32::MAX as u64 {
        return Err(std::io::Error::other("too long for a WAV file"));
    }
    let mut f = BufWriter::new(File::create(path)?);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len as u32).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&pcm.sample_rate.to_le_bytes())?;
    f.write_all(&(pcm.sample_rate * channels as u32 * 2).to_le_bytes())?;
    f.write_all(&(channels * 2).to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&(data_len as u32).to_le_bytes())?;
    for &s in &pcm.samples {
        f.write_all(&to_i16(s).to_le_bytes())?;
    }
    f.flush()
}

/// An integer as an 80-bit IEEE 754 extended float, big-endian (AIFF's sample rate).
fn extended(value: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if value == 0 {
        return out;
    }
    let shift = value.leading_zeros();
    let exponent = 16383 + 31 - shift as u16;
    let mantissa = (value as u64) << (32 + shift);
    out[..2].copy_from_slice(&exponent.to_be_bytes());
    out[2..].copy_from_slice(&mantissa.to_be_bytes());
    out
}

/// Writes 16-bit PCM AIFF.
fn write_aiff(path: &Path, pcm: &Pcm) -> std::io::Result<()> {
    let channels = pcm.channels.max(1);
    let data_len = pcm.samples.len() as u64 * 2;
    let ssnd_len = 8 + data_len;
    let form_len = 4 + (8 + 18) + (8 + ssnd_len);
    if form_len > u32::MAX as u64 {
        return Err(std::io::Error::other("too long for an AIFF file"));
    }
    let mut f = BufWriter::new(File::create(path)?);
    f.write_all(b"FORM")?;
    f.write_all(&(form_len as u32).to_be_bytes())?;
    f.write_all(b"AIFFCOMM")?;
    f.write_all(&18u32.to_be_bytes())?;
    f.write_all(&channels.to_be_bytes())?;
    f.write_all(&(frames(pcm) as u32).to_be_bytes())?;
    f.write_all(&16u16.to_be_bytes())?;
    f.write_all(&extended(pcm.sample_rate))?;
    f.write_all(b"SSND")?;
    f.write_all(&(ssnd_len as u32).to_be_bytes())?;
    f.write_all(&0u32.to_be_bytes())?; // offset
    f.write_all(&0u32.to_be_bytes())?; // block size
    for &s in &pcm.samples {
        f.write_all(&to_i16(s).to_be_bytes())?;
    }
    f.flush()
}

/// Writes 16-bit FLAC.
fn write_flac(path: &Path, pcm: &Pcm) -> std::io::Result<()> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;

    let channels = pcm.channels.max(1) as usize;
    if channels > 8 {
        return Err(std::io::Error::other("FLAC takes up to 8 channels"));
    }
    let samples: Vec<i32> = pcm.samples.iter().map(|&s| to_i16(s) as i32).collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, e)| std::io::Error::other(format!("{e:?}")))?;
    let source = flacenc::source::MemSource::from_samples(&samples, channels, 16, pcm.sample_rate as usize);
    let mut stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    // flacenc records the short last frame as the minimum block size, which makes decoders
    // (Symphonia, libFLAC) take the stream for a variable-block-size one and misread it. Like the
    // reference encoder, state the fixed block size.
    stream
        .stream_info_mut()
        .set_block_sizes(config.block_size, config.block_size)
        .map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
    std::fs::write(path, sink.as_slice())
}

/// Writes Ogg Vorbis.
fn write_ogg(path: &Path, pcm: &Pcm) -> std::io::Result<()> {
    use vorbis_rs::{VorbisBitrateManagementStrategy, VorbisEncoderBuilder};

    let err = |e: vorbis_rs::VorbisError| std::io::Error::other(e.to_string());
    let channels = pcm.channels.max(1) as usize;
    let rate = NonZeroU32::new(pcm.sample_rate).ok_or_else(|| std::io::Error::other("no sample rate"))?;
    let channel_count = u8::try_from(channels)
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or_else(|| std::io::Error::other("too many channels"))?;
    let file = BufWriter::new(File::create(path)?);
    let mut encoder = VorbisEncoderBuilder::new(rate, channel_count, file)
        .map_err(err)?
        .bitrate_management_strategy(VorbisBitrateManagementStrategy::QualityVbr {
            target_quality: VORBIS_QUALITY,
        })
        .build()
        .map_err(err)?;
    const BLOCK: usize = 4096;
    let mut planar = vec![Vec::with_capacity(BLOCK); channels];
    for chunk in pcm.samples.chunks(BLOCK * channels) {
        for p in planar.iter_mut() {
            p.clear();
        }
        for frame in chunk.chunks_exact(channels) {
            for (p, &s) in planar.iter_mut().zip(frame) {
                p.push(if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 });
            }
        }
        encoder.encode_audio_block(&planar).map_err(err)?;
    }
    encoder.finish().map_err(err)?.flush()
}

fn convert_with_ffmpeg(source: &Path, format: &str, dest: &Path) -> Result<(), String> {
    let program = ffmpeg().ok_or_else(|| {
        format!("converting to {} needs FFmpeg; install it, or pick a {0} sound", format.to_uppercase())
    })?;
    let codec: &[&str] = match format {
        "mp3" => &["-c:a", "libmp3lame", "-q:a", "2", "-f", "mp3"],
        _ => &["-c:a", "aac", "-b:a", "192k", "-f", "ipod"],
    };
    let mut cmd = Command::new(&program);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(source)
        .args(["-vn", "-map", "0:a:0", "-map_metadata", "-1"])
        .args(codec)
        .arg(dest)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd.output().map_err(|e| format!("could not start FFmpeg: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("unknown error");
    Err(format!("FFmpeg could not convert the sound: {last}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{sine, write_wav as write_test_wav};

    #[test]
    fn extended_sample_rates() {
        assert_eq!(extended(44100), [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(48000), [0x40, 0x0E, 0xBB, 0x80, 0, 0, 0, 0, 0, 0]);
        assert_eq!(extended(8000), [0x40, 0x0B, 0xFA, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn converts_to_every_built_in_format() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hit.wav");
        let tone = sine(44100, 0.5, 330.0, |_| 0.5);
        let stereo: Vec<f32> = tone.iter().flat_map(|&s| [s, -s]).collect();
        write_test_wav(&src, 44100, 2, &stereo, &[]);
        for format in BUILT_IN {
            let dest = dir.path().join(format!("out.{format}"));
            convert(&src, format, &dest).unwrap();
            let a = analysis::analyze(&dest).unwrap_or_else(|e| panic!("{format}: {e}"));
            assert!((a.duration - 0.5).abs() < 0.02, "{format}: {}", a.duration);
            assert_eq!((a.sample_rate, a.channels), (44100, 2), "{format}");
            assert!((a.peak_db - -6.0).abs() < 1.0, "{format}: {}", a.peak_db);
        }
    }

    /// Runs only where FFmpeg is installed.
    #[test]
    fn converts_to_mp3_and_m4a_with_ffmpeg() {
        if ffmpeg().is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hit.flac");
        let pcm = Pcm { sample_rate: 44100, channels: 1, samples: sine(44100, 0.5, 330.0, |_| 0.5) };
        write_flac(&src, &pcm).unwrap();
        for format in WITH_FFMPEG {
            // Like the temporary files of `project::write_into`: the extension does not say the format.
            let dest = dir.path().join(format!(".out.{format}.soundhive-tmp"));
            convert(&src, format, &dest).unwrap();
            let named = dir.path().join(format!("out.{format}"));
            std::fs::rename(&dest, &named).unwrap();
            let a = analysis::analyze(&named).unwrap_or_else(|e| panic!("{format}: {e}"));
            assert!((a.duration - 0.5).abs() < 0.1, "{format}: {}", a.duration);
            assert_eq!((a.sample_rate, a.channels), (44100, 1), "{format}");
        }
    }

    #[test]
    fn refuses_unknown_formats() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("hit.wav");
        write_test_wav(&src, 44100, 1, &sine(44100, 0.1, 330.0, |_| 0.5), &[]);
        assert!(convert(&src, "xyz", &dir.path().join("out.xyz")).is_err());
    }
}
