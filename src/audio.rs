//! Audio finishing with ffmpeg: speed, trailing silence, voice references and joined exports.
//! Commands take argument arrays and never go through a shell.

use std::path::{Path, PathBuf};

/// The ffmpeg audio filter for a speed and trailing silence, or `None` when the audio is final.
pub fn filter(speed: f64, silence: f64) -> Option<String> {
    let mut parts = Vec::new();
    if (speed - 1.0).abs() > f64::EPSILON {
        parts.push(format!("atempo={speed:.4}"));
    }
    if silence > 0.0 {
        parts.push(format!("apad=pad_dur={silence}"));
    }
    (!parts.is_empty()).then(|| parts.join(","))
}

/// Seconds of PCM audio in a WAV file, from its `fmt ` byte rate and `data` size.
pub fn wav_seconds(bytes: &[u8]) -> Option<f64> {
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let (mut at, mut byte_rate) = (12, None);
    while let Some(id) = bytes.get(at..at + 4) {
        let size = u32_at(at + 4)?;
        match id {
            b"fmt " => byte_rate = u32_at(at + 16),
            b"data" => return Some(f64::from(size) / f64::from(byte_rate.filter(|r| *r > 0)?)),
            _ => {}
        }
        // Chunks are padded to an even size.
        at += 8 + usize::try_from(size).ok()? + usize::try_from(size % 2).ok()?;
    }
    None
}

async fn ffmpeg(args: &[&std::ffi::OsStr]) -> std::io::Result<()> {
    let output = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .output()
        .await?;
    if output.status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Writes `source` through `filter` to `target` as 24-bit PCM; the pitch is kept.
pub async fn apply_filter(source: &Path, target: &Path, filter: &str) -> std::io::Result<()> {
    ffmpeg(&[
        "-i".as_ref(),
        source.as_os_str(),
        "-map".as_ref(),
        "0:a:0".as_ref(),
        "-af".as_ref(),
        filter.as_ref(),
        "-c:a".as_ref(),
        "pcm_s24le".as_ref(),
        target.as_os_str(),
    ])
    .await
}

/// Decodes any audio into a 16-bit PCM WAV, cut at 121 seconds so overlong input is detectable.
pub async fn to_wav(source: &Path, target: &Path) -> std::io::Result<()> {
    ffmpeg(&[
        "-i".as_ref(),
        source.as_os_str(),
        "-t".as_ref(),
        "121".as_ref(),
        "-vn".as_ref(),
        "-c:a".as_ref(),
        "pcm_s16le".as_ref(),
        target.as_os_str(),
    ])
    .await
}

/// Joins WAVs end to end into one 24-bit PCM WAV.
pub async fn join(sources: &[PathBuf], target: &Path) -> std::io::Result<()> {
    let mut args: Vec<std::ffi::OsString> = Vec::new();
    for source in sources {
        args.extend(["-i".into(), source.into()]);
    }
    let inputs: String = (0..sources.len()).fold(String::new(), |mut out, i| {
        use std::fmt::Write as _;
        let _ = write!(out, "[{i}:a]");
        out
    });
    args.extend([
        "-filter_complex".into(),
        format!("{inputs}concat=n={}:v=0:a=1[out]", sources.len()).into(),
        "-map".into(),
        "[out]".into(),
        "-c:a".into(),
        "pcm_s24le".into(),
        target.into(),
    ]);
    let args: Vec<&std::ffi::OsStr> = args.iter().map(std::ffi::OsString::as_os_str).collect();
    ffmpeg(&args).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(rate: u32, channels: u16, bits: u16, frames: u32) -> Vec<u8> {
        let block = u32::from(channels) * u32::from(bits / 8);
        let data = frames * block;
        let mut out = Vec::new();
        out.extend(b"RIFF");
        out.extend((36 + 8 + data).to_le_bytes());
        out.extend(b"WAVE");
        // An unrelated chunk before fmt, as some encoders write.
        out.extend(b"LIST");
        out.extend(4u32.to_le_bytes());
        out.extend(b"INFO");
        out.extend(b"fmt ");
        out.extend(16u32.to_le_bytes());
        out.extend(1u16.to_le_bytes());
        out.extend(channels.to_le_bytes());
        out.extend(rate.to_le_bytes());
        out.extend((rate * block).to_le_bytes());
        out.extend(u16::try_from(block).unwrap().to_le_bytes());
        out.extend(bits.to_le_bytes());
        out.extend(b"data");
        out.extend(data.to_le_bytes());
        out.resize(out.len() + data as usize, 0);
        out
    }

    #[test]
    fn the_filter_changes_speed_and_pads_silence_only_when_asked() {
        assert_eq!(filter(1.0, 0.0), None);
        assert_eq!(filter(1.25, 0.0).as_deref(), Some("atempo=1.2500"));
        assert_eq!(filter(1.0, 1.5).as_deref(), Some("apad=pad_dur=1.5"));
        assert_eq!(
            filter(0.75, 10.0).as_deref(),
            Some("atempo=0.7500,apad=pad_dur=10")
        );
    }

    #[test]
    fn wav_seconds_reads_the_byte_rate_and_data_size() {
        assert_eq!(wav_seconds(&wav(24_000, 1, 16, 36_000)), Some(1.5));
        assert_eq!(wav_seconds(&wav(48_000, 2, 24, 96_000)), Some(2.0));
        assert_eq!(wav_seconds(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(wav_seconds(b"not audio"), None);
        assert_eq!(wav_seconds(&[]), None);
    }
}
