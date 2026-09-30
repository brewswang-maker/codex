//! Microphone capture for the composer's voice input.
//!
//! A dedicated thread owns the cpal input stream; the audio callback
//! downmixes to mono, resamples to [`TARGET_SAMPLE_RATE`] and converts to
//! 16-bit PCM before handing each chunk to the sink closure. Nothing is
//! written to disk and the stream - hence the device - is released as soon
//! as the [`Capture`] handle stops or drops.
//!
//! [`TARGET_SAMPLE_RATE`] is 24 kHz because that is the PCM format the
//! realtime session declares on the wire (`{"type": "audio/pcm", "rate":
//! 24000}`); chunks are forwarded to the model unchanged.
//!
//! ## Linux prerequisites
//!
//! Builds link ALSA through pkg-config, so the ALSA development headers
//! and pkg-config must be installed:
//!
//! ```text
//! sudo apt install libasound2-dev pkg-config   # Debian/Ubuntu
//! sudo dnf install alsa-lib-devel pkgconf      # Fedora
//! ```
//!
//! CI installs `libasound2-dev` on its Linux runners
//! (`.github/workflows/gui.yml`); macOS (CoreAudio) and Windows (WASAPI)
//! need no extra packages.
//!
//! Running needs a reachable audio server (PipeWire or PulseAudio) and an
//! input device; a headless machine has neither, so the ignored smoke test
//! below skips instead of failing:
//!
//! ```text
//! cargo test -p codex-gui-core capture_smoke -- --ignored --nocapture
//! ```

use cpal::FromSample;
use cpal::Sample;
use cpal::SampleFormat;
use cpal::SizedSample;
use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use std::sync::mpsc;

#[cfg(test)]
#[path = "audio_capture_tests.rs"]
mod tests;

/// The wire rate for realtime PCM input; capture always normalizes to it.
pub const TARGET_SAMPLE_RATE: u32 = 24_000;

/// One captured slice of mono PCM16 at [`TARGET_SAMPLE_RATE`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioChunk {
    /// Mono 16-bit signed samples in little-endian order on the wire.
    pub pcm16: Vec<i16>,
}

impl AudioChunk {
    /// Playback duration of the chunk in milliseconds.
    pub fn duration_ms(&self) -> u32 {
        (self.pcm16.len() as u64 * 1_000 / u64::from(TARGET_SAMPLE_RATE)) as u32
    }
}

/// Why microphone capture could not start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// The host reports no default input device.
    NoInputDevice,
    /// The device refused its own default configuration.
    DeviceConfig(String),
    /// The input stream could not be built (busy device, format...).
    StreamBuild(String),
    /// The stream could not be started.
    StreamPlay(String),
    /// The capture thread could not be spawned or exited early.
    Thread(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoInputDevice => write!(f, "未检测到可用的麦克风设备"),
            Self::DeviceConfig(reason) => write!(f, "麦克风配置不可用：{reason}"),
            Self::StreamBuild(reason) => write!(f, "无法打开麦克风：{reason}"),
            Self::StreamPlay(reason) => write!(f, "无法开始录音：{reason}"),
            Self::Thread(reason) => write!(f, "录音线程不可用：{reason}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Handle to a running capture. Dropping it signals the thread to release
/// the device; [`Capture::stop`] additionally waits for that to happen.
pub struct Capture {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    /// Signals the capture thread and waits for the device to be released.
    pub fn stop(mut self) {
        self.signal();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    fn signal(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.signal();
    }
}

/// Starts capturing from the default input device into `sink`.
///
/// Blocks until the stream is playing (device open is fast) so device
/// failures surface synchronously; the audio itself arrives through `sink`
/// on the capture thread.
pub fn start(sink: impl FnMut(AudioChunk) + Send + 'static) -> Result<Capture, CaptureError> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), CaptureError>>();
    let thread = std::thread::Builder::new()
        .name("voice-capture".to_string())
        .spawn(move || run(sink, ready_tx, stop_rx))
        .map_err(|error| CaptureError::Thread(error.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(())) => Ok(Capture {
            stop: Some(stop_tx),
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            let _ = stop_tx.send(());
            let _ = thread.join();
            Err(error)
        }
        Err(_) => Err(CaptureError::Thread(
            "capture thread exited before starting".to_string(),
        )),
    }
}

/// The capture thread: open the device, play until stopped, release.
fn run(
    sink: impl FnMut(AudioChunk) + Send + 'static,
    ready: mpsc::Sender<Result<(), CaptureError>>,
    stop: mpsc::Receiver<()>,
) {
    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else {
        let _ = ready.send(Err(CaptureError::NoInputDevice));
        return;
    };
    let config = match device.default_input_config() {
        Ok(config) => config,
        Err(error) => {
            let _ = ready.send(Err(CaptureError::DeviceConfig(error.to_string())));
            return;
        }
    };
    let sample_rate = config.sample_rate();
    let channels = config.channels();
    let format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let stream =
        match build_any_format(&device, &stream_config, format, sample_rate, channels, sink) {
            Ok(stream) => stream,
            Err(error) => {
                let _ = ready.send(Err(CaptureError::StreamBuild(error)));
                return;
            }
        };
    if let Err(error) = stream.play() {
        let _ = ready.send(Err(CaptureError::StreamPlay(error.to_string())));
        return;
    }
    let _ = ready.send(Ok(()));

    // Park until the handle stops us (or is dropped); then dropping the
    // stream closes the device.
    let _ = stop.recv();
    drop(stream);
}

/// Builds the input stream for whichever sample format the device's default
/// configuration uses, mirroring the dispatch the upstream voice host uses.
fn build_any_format(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    format: SampleFormat,
    sample_rate: u32,
    channels: u16,
    sink: impl FnMut(AudioChunk) + Send + 'static,
) -> Result<cpal::Stream, String> {
    macro_rules! build {
        ($format:ty) => {
            build_stream::<$format>(device, stream_config, sample_rate, channels, sink)
        };
    }
    match format {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        _ => Err("unsupported sample format".to_string()),
    }
}

/// Builds the typed input stream: callback data is converted to mono f32,
/// resampled, and handed out as PCM16 chunks.
fn build_stream<T>(
    device: &cpal::Device,
    stream_config: &cpal::StreamConfig,
    sample_rate: u32,
    channels: u16,
    mut sink: impl FnMut(AudioChunk) + Send + 'static,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + Sample + Send + 'static,
    f32: FromSample<T>,
{
    let mut resampler = Resampler::new(sample_rate, TARGET_SAMPLE_RATE);
    device
        .build_input_stream(
            *stream_config,
            move |data: &[T], _info: &cpal::InputCallbackInfo| {
                let mono = downmix(data, channels);
                let pcm16 = resampler.push(&mono);
                if !pcm16.is_empty() {
                    sink(AudioChunk { pcm16 });
                }
            },
            |error| tracing::warn!(%error, "microphone stream error"),
            None,
        )
        .map_err(|error| error.to_string())
}

/// Averages interleaved channels into mono samples.
fn downmix<T>(data: &[T], channels: u16) -> Vec<f32>
where
    T: Sample,
    f32: FromSample<T>,
{
    let channels = channels.max(1) as usize;
    if channels == 1 {
        return data
            .iter()
            .map(|sample| sample.to_sample::<f32>())
            .collect();
    }
    data.chunks_exact(channels)
        .map(|frame| {
            let sum: f32 = frame.iter().map(|sample| sample.to_sample::<f32>()).sum();
            sum / channels as f32
        })
        .collect()
}

/// Rounds a normalized sample to PCM16.
fn to_pcm16(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * 32_767.0).round() as i16
}

/// Resamples mono f32 audio between two rates, keeping phase across the
/// callback boundaries so chunked input yields the same output as one run.
///
/// Integer downsample factors average each group (a box filter, which also
/// suppresses the aliasing plain decimation would fold in); every other
/// ratio interpolates linearly, per the plan's "simple linear resampling".
struct Resampler {
    mode: ResamplerMode,
}

#[derive(Debug, Clone, Copy)]
enum ResamplerMode {
    /// Input rate equals the target: samples pass through untouched.
    Identity,
    /// Integer downsample factor: emit the mean of each group of `factor`.
    Box { factor: u32, sum: f32, count: u32 },
    /// General ratio: linear interpolation with a fractional phase.
    Linear {
        step: f64,
        pos: f64,
        last: Option<f32>,
    },
}

impl Resampler {
    fn new(from: u32, to: u32) -> Self {
        let from = from.max(1);
        let to = to.max(1);
        let mode = if from == to {
            ResamplerMode::Identity
        } else if from.is_multiple_of(to) && from / to >= 2 {
            ResamplerMode::Box {
                factor: from / to,
                sum: 0.0,
                count: 0,
            }
        } else {
            ResamplerMode::Linear {
                step: f64::from(from) / f64::from(to),
                pos: 1.0,
                last: None,
            }
        };
        Self { mode }
    }

    /// Resamples one callback's worth of mono samples into PCM16.
    fn push(&mut self, input: &[f32]) -> Vec<i16> {
        match &mut self.mode {
            ResamplerMode::Identity => input.iter().map(|sample| to_pcm16(*sample)).collect(),
            ResamplerMode::Box { factor, sum, count } => {
                let mut out = Vec::with_capacity(input.len() / *factor as usize + 1);
                for sample in input {
                    *sum += sample;
                    *count += 1;
                    if *count == *factor {
                        out.push(to_pcm16(*sum / *factor as f32));
                        *sum = 0.0;
                        *count = 0;
                    }
                }
                out
            }
            ResamplerMode::Linear { step, pos, last } => {
                let mut out = Vec::with_capacity((input.len() as f64 / *step).ceil() as usize + 1);
                for sample in input {
                    let Some(previous) = *last else {
                        // First sample ever: it anchors the span (previous,
                        // current]; the next output sits one step later.
                        *last = Some(*sample);
                        *pos = *step;
                        continue;
                    };
                    while *pos <= 1.0 {
                        let fraction = *pos as f32;
                        out.push(to_pcm16(previous + (sample - previous) * fraction));
                        *pos += *step;
                    }
                    *pos -= 1.0;
                    *last = Some(*sample);
                }
                out
            }
        }
    }
}
