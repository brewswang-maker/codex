//! Unit tests for the capture pipeline's pure parts: downmixing, the
//! resampler's phase bookkeeping across callback boundaries, PCM16
//! conversion, and chunk accounting.
#![allow(clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn identity_resampler_passes_samples_through() {
    let mut resampler = Resampler::new(24_000, 24_000);
    let out = resampler.push(&[0.5, -0.5, 0.0]);
    assert_eq!(out, vec![16_384, -16_384, 0]);
}

#[test]
fn box_resampler_averages_integer_groups() {
    let mut resampler = Resampler::new(48_000, 24_000);
    // Pairs average: (1.0 + 0.0) / 2 = 0.5 -> 16384; (-1.0 + -1.0)/2 -> -32767.
    let out = resampler.push(&[1.0, 0.0, -1.0, -1.0]);
    assert_eq!(out, vec![16_384, -32_767]);
    assert_eq!(resampler.push(&[0.25, 0.25]), vec![8_192]);
}

#[test]
fn box_resampler_keeps_phase_across_pushes() {
    let mut whole = Resampler::new(48_000, 24_000);
    let mut split = Resampler::new(48_000, 24_000);
    let input = [0.1, 0.9, -0.4, 0.2, 0.7, -0.7];
    let all = whole.push(&input);
    let mut halves = split.push(&input[..3]);
    halves.extend(split.push(&input[3..]));
    assert_eq!(all, halves);
}

#[test]
fn linear_resampler_upsamples_to_the_target_count() {
    let mut resampler = Resampler::new(16_000, 24_000);
    let out = resampler.push(&vec![0.25; 1_600]);
    // 1.5x the input samples, within the startup offset (the resampler
    // emits nothing for the very first sample and trails by one span).
    assert!(
        out.len().abs_diff(2_400) <= 2,
        "expected ~2400 outputs, got {}",
        out.len()
    );
    assert!(out.iter().all(|sample| *sample == 8_192));
}

#[test]
fn linear_resampler_keeps_phase_across_pushes() {
    let mut whole = Resampler::new(44_100, 24_000);
    let mut split = Resampler::new(44_100, 24_000);
    let input: Vec<f32> = (0..441).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
    let all = whole.push(&input);
    let mut halves = split.push(&input[..200]);
    halves.extend(split.push(&input[200..]));
    assert_eq!(all, halves);
}

#[test]
fn downmix_averages_stereo_frames() {
    let mono = downmix::<f32>(&[1.0, 0.0, 0.5, 0.5], 2);
    assert_eq!(mono, vec![0.5, 0.5]);
    // An odd trailing sample without a full frame is dropped.
    assert_eq!(downmix::<f32>(&[1.0, 0.0, 0.25], 2), vec![0.5]);
}

#[test]
fn downmix_converts_integer_formats_to_normalized_floats() {
    let mono = downmix::<i16>(&[i16::MAX, i16::MAX], 2);
    assert!((mono[0] - 1.0).abs() < 1e-4, "got {}", mono[0]);
}

#[test]
fn downmix_passes_mono_through() {
    assert_eq!(downmix::<f32>(&[0.1, 0.2], 1), vec![0.1, 0.2]);
}

#[test]
fn pcm16_rounding_clamps_out_of_range_samples() {
    assert_eq!(to_pcm16(2.0), i16::MAX);
    assert_eq!(to_pcm16(-2.0), -32_767);
    assert_eq!(to_pcm16(0.0), 0);
}

#[test]
fn audio_chunk_reports_its_duration() {
    assert_eq!(
        AudioChunk {
            pcm16: vec![0; 2_400]
        }
        .duration_ms(),
        100
    );
    assert_eq!(
        AudioChunk {
            pcm16: vec![0; 1_200]
        }
        .duration_ms(),
        50
    );
}

/// Manual smoke: records the real default device and checks that audio
/// keeps flowing and the device is released on stop. Ignored by default
/// (needs a microphone and an accessible audio server); run it as
///
/// ```text
/// cargo test -p codex-gui-core capture_smoke -- --ignored --nocapture
/// ```
#[test]
#[ignore = "smoke: needs a real microphone and audio server"]
fn capture_smoke_records_the_default_device() {
    use std::sync::Arc;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use std::time::Instant;

    let total = Arc::new(AtomicUsize::new(0));
    let first_chunk = Arc::new(Mutex::new(None::<Vec<i16>>));

    let totals = Arc::clone(&total);
    let first = Arc::clone(&first_chunk);
    let capture = match start(move |chunk| {
        totals.fetch_add(chunk.pcm16.len(), Ordering::Relaxed);
        let mut slot = first.lock().expect("chunk slot");
        if slot.is_none() {
            *slot = Some(chunk.pcm16);
        }
    }) {
        Ok(capture) => capture,
        Err(error) => {
            eprintln!("skip: no capture device available ({error})");
            return;
        }
    };

    // 12_000 samples = 500 ms of 24 kHz audio; allow 5 s to gather them.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && total.load(Ordering::Relaxed) < 12_000 {
        std::thread::sleep(Duration::from_millis(50));
    }

    capture.stop();
    let samples = total.load(Ordering::Relaxed);
    let preview = first_chunk
        .lock()
        .expect("chunk slot")
        .clone()
        .unwrap_or_default();
    eprintln!(
        "captured {samples} samples; first chunk starts with {:?}",
        &preview[..preview.len().min(8)]
    );
    assert!(
        samples >= 12_000,
        "expected at least 12_000 samples (500 ms) within 5 s, got {samples}"
    );

    // The device must be free again: a second session opens and stops.
    match start(|_chunk| {}) {
        Ok(capture) => capture.stop(),
        Err(error) => panic!("device still busy after stop: {error}"),
    }
}
