//! Local WASAPI producers. A bounded 250ms queue separates capture from FFmpeg.
//!
//! Device samples are appended contiguously. Packet timestamps are used only to
//! (a) steer a slow, high-quality drift-correcting resampler and (b) re-anchor
//! after idle/starvation or a gross timing jump. Earlier builds re-placed every
//! packet by its jittery QPC timestamp, inserting or dropping samples at almost
//! every 10ms packet boundary, which was audible as crackling/distortion.
use crate::model::AudioSource;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
    collections::VecDeque,
    io::Write,
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use wasapi::{AudioClient, DeviceEnumerator, Direction, SampleType, StreamMode, WaveFormat};
use windows::Win32::Media::Audio::{
    IAudioCaptureClient, AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR,
};

/// Output stream format sent to FFmpeg: 48kHz interleaved stereo f32le.
pub const RATE: u32 = 48_000;
const CHANNELS: usize = 2;
const HNS_PER_SECOND: f64 = 10_000_000.0;
/// Frames per paced chunk (10ms).
const CHUNK: usize = 480;
/// Delay between capture time and delivery, absorbing packet delivery jitter.
/// Grows automatically (to the cap) for devices whose packets arrive later,
/// e.g. high-latency Bluetooth/USB microphones. It delays delivery to FFmpeg
/// only; sample-to-clock alignment, and therefore A/V sync, is unchanged.
const MIN_HOLDBACK_HNS: u64 = 500_000;
const MAX_HOLDBACK_HNS: u64 = 4_000_000;
const HOLDBACK_MARGIN_HNS: u64 = 100_000;
/// Timestamp disagreement treated as a real timing jump rather than jitter/drift.
const REALIGN_FRAMES: f64 = 2_400.0;
/// After starvation, data later than this was genuinely lost (not idle loopback).
const LATE_TOLERANCE_FRAMES: f64 = 48.0;
/// Drift controller: error low-pass time constant and proportional gain.
const DRIFT_FILTER_SECONDS: f64 = 2.0;
const DRIFT_GAIN_PER_SECOND: f64 = 0.05;
const MAX_RATIO_DEVIATION: f64 = 0.005;
/// Queue bound beyond the holdback before backpressure becomes an error.
const MAX_BACKLOG_FRAMES: usize = RATE as usize / 4;
/// Paced chunks buffered between capture and the FFmpeg socket writer.
pub const OUTPUT_CHANNEL_CHUNKS: usize = 1000;

#[derive(Default)]
pub struct Meter {
    pub peak: AtomicU32,
    pub packets: AtomicU64,
    pub gaps: AtomicU64,
    pub volume: AtomicU32,
    pub muted: AtomicBool,
    pub error: Mutex<Option<String>>,
    pub written_bytes: AtomicU64,
    pub last_write_ms: AtomicU64,
    pub last_packet_ms: AtomicU64,
    /// Audible timeline breaks: gross timestamp jumps plus data that arrived
    /// after its slot had already been output as silence.
    pub discontinuities: AtomicU64,
    /// Packets Windows flagged with AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.
    pub device_glitches: AtomicU64,
    /// Packets whose timestamps were flagged or implausible and were ignored.
    pub timestamp_errors: AtomicU64,
    /// Current drift correction applied by the resampler, in ppm (f32 bits).
    pub correction_ppm: AtomicU32,
    /// Device sample clock relative to QPC, measured from device positions (f32 bits).
    pub clock_ppm: AtomicU32,
    /// Windows endpoint mix format before conversion to 48kHz stereo float.
    pub format: Mutex<Option<String>>,
    /// New Voice Enhancer settings to apply on the next chunk (live tests).
    pub voice_update: Mutex<Option<(crate::voice::VoiceConfig, f32)>>,
    /// Play the unprocessed microphone, level-matched (live A/B only).
    pub bypass: AtomicBool,
    /// Frames the Voice Enhancer could not process (raw audio was used).
    pub dsp_faults: AtomicU64,
}

/// Largest device channel count opened natively (microphone arrays).
const MAX_DEVICE_CHANNELS: usize = 16;

/// Convert interleaved device samples to interleaved stereo.
/// 1 channel is duplicated, 2 pass through, and more (microphone arrays,
/// usually with no channel mask) are averaged into centred mono, which keeps a
/// coherent voice at its level while uncorrelated capsule noise is reduced.
pub fn to_stereo(samples: Vec<f32>, channels: usize) -> Vec<f32> {
    match channels {
        2 => samples,
        0 => Vec::new(),
        1 => samples.iter().flat_map(|v| [*v, *v]).collect(),
        n => samples
            .chunks_exact(n)
            .flat_map(|frame| {
                let mono = frame.iter().sum::<f32>() / n as f32;
                [mono, mono]
            })
            .collect(),
    }
}

/// Counters produced by [`Timeline`]; mirrored into [`Meter`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimelineStats {
    pub realignments: u64,
    pub late_after_starvation: u64,
    pub timestamp_errors: u64,
    pub inserted_silence_frames: u64,
    pub dropped_frames: u64,
}

fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > 1e-14 * sum {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        k += 1.0;
    }
    sum
}

const HALF_TAPS: usize = 16;
const TAPS: usize = 2 * HALF_TAPS;
const PHASES: usize = 256;
const KAISER_BETA: f64 = 8.0;

/// Variable-ratio band-limited interpolator (32-tap Kaiser-windowed sinc,
/// 256 linearly interpolated phases) for interleaved stereo. At a ratio of
/// exactly 1 it is a bit-exact passthrough; at the small ratios needed for
/// clock-drift correction its error is far below audibility.
pub struct Resampler {
    table: Vec<f32>,
    history: Vec<f32>,
    position: f64,
    step: f64,
}
impl Default for Resampler {
    fn default() -> Self {
        Self::new()
    }
}
impl Resampler {
    pub fn new() -> Self {
        let mut table = vec![0f32; (PHASES + 1) * TAPS];
        for phase in 0..=PHASES {
            let mu = phase as f64 / PHASES as f64;
            let mut taps = [0f64; TAPS];
            for (k, tap) in taps.iter_mut().enumerate() {
                let t = k as f64 - (HALF_TAPS - 1) as f64 - mu;
                let sinc = if t.abs() < 1e-9 {
                    1.0
                } else if (t - t.round()).abs() < 1e-9 {
                    0.0
                } else {
                    (std::f64::consts::PI * t).sin() / (std::f64::consts::PI * t)
                };
                let r = t / HALF_TAPS as f64;
                let window = if r.abs() >= 1.0 {
                    0.0
                } else {
                    bessel_i0(KAISER_BETA * (1.0 - r * r).sqrt()) / bessel_i0(KAISER_BETA)
                };
                *tap = sinc * window;
            }
            let sum: f64 = taps.iter().sum();
            for (k, tap) in taps.iter().enumerate() {
                table[phase * TAPS + k] = (tap / sum) as f32;
            }
        }
        Self {
            table,
            history: vec![0.; (HALF_TAPS - 1) * CHANNELS],
            position: (HALF_TAPS - 1) as f64,
            step: 1.0,
        }
    }
    /// Output frames per input frame.
    pub fn set_ratio(&mut self, ratio: f64) {
        self.step = 1.0 / ratio;
    }
    pub fn ratio(&self) -> f64 {
        1.0 / self.step
    }
    /// Output frames that the input already accepted will still produce.
    pub fn pending_output(&self) -> f64 {
        ((self.history.len() / CHANNELS) as f64 - self.position) / self.step
    }
    pub fn process(&mut self, input: &[f32], out: &mut VecDeque<f32>) {
        self.history.extend_from_slice(input);
        let frames = self.history.len() / CHANNELS;
        while (self.position as usize) + HALF_TAPS < frames {
            let index = self.position as usize;
            let scaled = (self.position - index as f64) * PHASES as f64;
            let phase = (scaled as usize).min(PHASES - 1);
            let blend = (scaled - phase as f64) as f32;
            let a = &self.table[phase * TAPS..(phase + 1) * TAPS];
            let b = &self.table[(phase + 1) * TAPS..(phase + 2) * TAPS];
            let base = (index + 1 - HALF_TAPS) * CHANNELS;
            let (mut left, mut right) = (0f32, 0f32);
            for k in 0..TAPS {
                let c = a[k] + (b[k] - a[k]) * blend;
                left += self.history[base + 2 * k] * c;
                right += self.history[base + 2 * k + 1] * c;
            }
            out.push_back(left);
            out.push_back(right);
            self.position += self.step;
        }
        let consumed = (self.position as usize).saturating_sub(HALF_TAPS - 1);
        if consumed > 0 {
            self.history.drain(..consumed * CHANNELS);
            self.position -= consumed as f64;
        }
    }
}

/// Converts timestamped capture packets into a continuous stream paced by the
/// QPC clock shared with video capture. Pure logic; unit-tested without devices.
pub struct Timeline {
    origin_hns: u64,
    queue: VecDeque<f32>,
    sent: u64,
    resampler: Resampler,
    error_frames: f64,
    starved: bool,
    /// Input frames still to discard to honour a backward realignment.
    drop_debt: usize,
    /// Real input is held in the interpolator lookahead.
    unflushed: bool,
    /// Real (queued) audio has been output at least once.
    delivered: bool,
    holdback_hns: u64,
    pub stats: TimelineStats,
}
impl Timeline {
    /// `origin_hns`: QPC time (100ns units) of output frame 0.
    pub fn new(origin_hns: u64) -> Self {
        Self {
            origin_hns,
            queue: VecDeque::new(),
            sent: 0,
            resampler: Resampler::new(),
            error_frames: 0.,
            starved: true,
            drop_debt: 0,
            unflushed: false,
            delivered: false,
            holdback_hns: MIN_HOLDBACK_HNS,
            stats: TimelineStats::default(),
        }
    }
    fn written(&self) -> f64 {
        self.sent as f64 + (self.queue.len() / CHANNELS) as f64 + self.resampler.pending_output()
    }
    pub fn correction_ppm(&self) -> f64 {
        (self.resampler.ratio() - 1.) * 1e6
    }
    pub fn queued_frames(&self) -> usize {
        self.queue.len() / CHANNELS
    }
    pub fn holdback_ms(&self) -> f64 {
        self.holdback_hns as f64 / 10_000.
    }
    /// Accept one packet of interleaved stereo samples. `stamp_hns` is the QPC
    /// time of its first frame, or None when Windows marked it unreliable.
    /// Returns true when this packet re-anchored the timeline.
    pub fn push(&mut self, samples: &[f32], stamp_hns: Option<u64>) -> Result<bool> {
        let mut samples = samples;
        let frames = samples.len() / CHANNELS;
        let mut anchored = false;
        match stamp_hns.filter(|s| *s != 0) {
            // While a realignment drop is being settled, timing is not re-evaluated
            // unless the output starved in the meantime.
            Some(_) if self.drop_debt > 0 && !self.starved => {}
            Some(stamp) => {
                let expected =
                    (stamp as f64 - self.origin_hns as f64) * RATE as f64 / HNS_PER_SECOND;
                let error = expected - self.written();
                let max_early =
                    (self.holdback_hns as f64 / HNS_PER_SECOND) * RATE as f64 + REALIGN_FRAMES;
                if error > max_early || error < -(RATE as f64) {
                    // A packet cannot legitimately be due beyond the paced output or a
                    // second in the past; keep samples contiguous instead of trusting it.
                    self.stats.timestamp_errors += 1;
                } else if self.starved || error.abs() > REALIGN_FRAMES {
                    anchored = true;
                    if !self.starved {
                        self.stats.realignments += 1;
                    } else if error < -LATE_TOLERANCE_FRAMES {
                        // Audio arrived after its slot was output as silence. Hold back
                        // further so this device's delivery latency is covered from now on.
                        let late_hns = (-error / RATE as f64 * HNS_PER_SECOND) as u64;
                        self.holdback_hns = (self.holdback_hns + late_hns + HOLDBACK_MARGIN_HNS)
                            .min(MAX_HOLDBACK_HNS);
                        // Startup latency and settling an earlier realignment are not
                        // additional audible breaks.
                        if self.delivered && self.drop_debt == 0 {
                            self.stats.late_after_starvation += 1;
                        }
                    }
                    self.drop_debt = 0;
                    if error >= 1. {
                        let silence = error.round() as usize;
                        self.resampler
                            .process(&vec![0.; silence * CHANNELS], &mut self.queue);
                        self.stats.inserted_silence_frames += silence as u64;
                    } else if error <= -1. {
                        self.drop_debt = (-error).round() as usize;
                    }
                    self.error_frames = 0.;
                    self.resampler.set_ratio(1.);
                } else {
                    let alpha = 1. - (-(frames as f64) / RATE as f64 / DRIFT_FILTER_SECONDS).exp();
                    self.error_frames += alpha * (error - self.error_frames);
                    let deviation = (DRIFT_GAIN_PER_SECOND * self.error_frames / RATE as f64)
                        .clamp(-MAX_RATIO_DEVIATION, MAX_RATIO_DEVIATION);
                    self.resampler.set_ratio(1. + deviation);
                }
            }
            None => self.stats.timestamp_errors += 1,
        }
        self.starved = false;
        let drop = self.drop_debt.min(frames);
        self.drop_debt -= drop;
        self.stats.dropped_frames += drop as u64;
        samples = &samples[drop * CHANNELS..];
        self.unflushed |= !samples.is_empty();
        self.resampler.process(samples, &mut self.queue);
        let holdback_frames = (self.holdback_hns as f64 / HNS_PER_SECOND * RATE as f64) as usize;
        if self.queued_frames() > holdback_frames + MAX_BACKLOG_FRAMES {
            bail!("Audio queue exceeded 250ms; encoder is not keeping up")
        }
        Ok(anchored)
    }
    /// Return the next 10ms chunk once it is due at `now_hns`. Missing data is
    /// output as silence and the timeline is marked starved.
    pub fn pull(&mut self, now_hns: u64) -> Option<Vec<f32>> {
        let start = self.origin_hns + self.holdback_hns;
        let due = now_hns.saturating_sub(start) as f64 * RATE as f64 / HNS_PER_SECOND;
        if now_hns < start || ((self.sent + CHUNK as u64) as f64) > due {
            return None;
        }
        let mut chunk = vec![0f32; CHUNK * CHANNELS];
        if self.queue.len() < chunk.len() && self.unflushed {
            // About to starve: release the interpolator lookahead and ringing tail so
            // the final real frames play in place instead of being emitted after idle.
            self.resampler
                .process(&[0.; TAPS * CHANNELS], &mut self.queue);
            self.unflushed = false;
        }
        let available = chunk.len().min(self.queue.len());
        for (slot, sample) in chunk.iter_mut().zip(self.queue.drain(..available)) {
            *slot = sample;
        }
        self.delivered |= available > 0;
        if available < chunk.len() {
            self.starved = true;
        }
        self.sent += CHUNK as u64;
        Some(chunk)
    }
}

/// QPC in 100ns units, the same clock as WASAPI packet timestamps.
fn qpc_hns() -> u64 {
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
    let (mut counter, mut frequency) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
        let _ = QueryPerformanceFrequency(&mut frequency);
    }
    (counter as u128 * 10_000_000 / frequency.max(1) as u128) as u64
}

fn describe_format(f: &WaveFormat) -> String {
    format!(
        "{} Hz, {} ch, {}-bit ({} valid) {:?}, block {} bytes, mask 0x{:x}",
        f.get_samplespersec(),
        f.get_nchannels(),
        f.get_bitspersample(),
        f.get_validbitspersample(),
        f.get_subformat().ok(),
        f.get_blockalign(),
        f.get_dwchannelmask()
    )
}
pub struct AudioFeed {
    pub url: String,
    pub meter: Arc<Meter>,
    stop: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
    pub started: Instant,
    pub microphone: bool,
}
impl Drop for AudioFeed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}
#[derive(Serialize, Clone, Debug)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
}
pub fn devices() -> Result<Vec<AudioDevice>> {
    wasapi::initialize_mta().ok()?;
    let en = DeviceEnumerator::new()?;
    let mut out = Vec::new();
    for (dir, kind) in [
        (Direction::Render, "system"),
        (Direction::Capture, "microphone"),
    ] {
        for d in &en.get_device_collection(&dir)? {
            let d = d?;
            out.push(AudioDevice {
                id: d.get_id()?,
                name: d.get_friendlyname()?,
                kind: kind.into(),
            });
        }
    }
    Ok(out)
}
/// Result of a short test capture through the real producer (no encoder).
#[derive(Debug, Clone, Serialize)]
pub struct TestCapture {
    pub seconds: u64,
    pub packets: u64,
    pub peak: f32,
    pub rms: f64,
    pub discontinuities: u64,
    pub device_glitches: u64,
    pub timestamp_errors: u64,
    pub drift_correction_ppm: f32,
    pub measured_device_clock_ppm: f32,
    pub endpoint_mix_format: Option<String>,
    /// Interleaved stereo f32 at 48kHz, exactly as delivered to the encoder.
    #[serde(skip)]
    pub pcm: Vec<f32>,
}

/// Capture `seconds` of audio from `source`, calling `on_level(peak, elapsed)`
/// about every 100ms. Used by the CLI diagnostic and the in-app device test.
pub fn record_test(
    source: AudioSource,
    seconds: u64,
    stop: &std::sync::atomic::AtomicBool,
    mut on_level: impl FnMut(f32, f64),
) -> Result<TestCapture> {
    use std::io::Read;
    let feed = spawn(source)?;
    let mut socket = std::net::TcpStream::connect(feed.url.trim_start_matches("tcp://"))?;
    socket.set_read_timeout(Some(Duration::from_secs(2)))?;
    let began = Instant::now();
    let limit = seconds as usize * RATE as usize * CHANNELS * 4;
    let mut bytes = Vec::with_capacity(limit);
    let mut input = [0u8; 8192];
    let mut last_level = Instant::now();
    while bytes.len() < limit && !stop.load(Ordering::Relaxed) {
        if let Some(error) = feed.meter.error.lock().unwrap().clone() {
            bail!("Audio capture error: {error}")
        }
        if began.elapsed() > Duration::from_secs(seconds + 5) {
            bail!("Audio capture did not deliver enough samples")
        }
        let count = socket.read(&mut input[..8192.min(limit - bytes.len())])?;
        if count == 0 {
            bail!("Audio transport closed unexpectedly")
        }
        bytes.extend_from_slice(&input[..count]);
        if last_level.elapsed() > Duration::from_millis(100) {
            on_level(
                f32::from_bits(feed.meter.peak.load(Ordering::Relaxed)),
                began.elapsed().as_secs_f64(),
            );
            last_level = Instant::now();
        }
    }
    let m = feed.meter.clone();
    let format = m.format.lock().unwrap().clone();
    let pcm: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    if pcm.iter().any(|v| !v.is_finite()) {
        bail!("Nonfinite audio sample")
    }
    let peak = pcm.iter().fold(0f32, |p, v| p.max(v.abs()));
    let rms =
        (pcm.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / pcm.len().max(1) as f64).sqrt();
    Ok(TestCapture {
        seconds,
        packets: m.packets.load(Ordering::Relaxed),
        peak,
        rms,
        discontinuities: m.discontinuities.load(Ordering::Relaxed),
        device_glitches: m.device_glitches.load(Ordering::Relaxed),
        timestamp_errors: m.timestamp_errors.load(Ordering::Relaxed),
        drift_correction_ppm: f32::from_bits(m.correction_ppm.load(Ordering::Relaxed)),
        measured_device_clock_ppm: f32::from_bits(m.clock_ppm.load(Ordering::Relaxed)),
        endpoint_mix_format: format,
        pcm,
    })
}

/// Linear monitoring gain for a volume (0-2); boost is applied by the
/// Voice Enhancer in the feed.
pub fn monitor_level(volume: f32) -> f32 {
    volume.clamp(0.0, 2.0)
}
/// Whether listening would leak into capture: an enabled System or
/// "everything except" source records what plays on the speakers.
pub fn monitor_leaks(sources: &[AudioSource]) -> bool {
    sources
        .iter()
        .any(|a| a.enabled && (a.kind == "system" || a.kind == "exclude"))
}
/// Longest a monitoring session runs before it stops by itself.
pub const MONITOR_LIMIT: Duration = Duration::from_secs(600);
/// "Hear yourself": play `source` (normally the microphone) on an output
/// device in real time until `stop` is set. Uses the same capture path as
/// recording (volume included), adds the boost with a soft limiter, keeps
/// latency bounded (about 0.1-0.15 s) and never records anything.
pub fn monitor(
    source: AudioSource,
    output: &str,
    stop: &AtomicBool,
    gain: &AtomicU32,
    meter_slot: &Mutex<Option<Arc<Meter>>>,
    mut on_level: impl FnMut(f32),
) -> Result<()> {
    use std::io::Read;
    // Volume comes from `gain`; boost and the Voice Enhancer run in the feed
    // exactly as when recording, and can be changed through the meter.
    let feed = spawn(AudioSource {
        volume: 1.0,
        muted: false,
        ..source
    })?;
    *meter_slot.lock().unwrap() = Some(feed.meter.clone());
    let mut socket = std::net::TcpStream::connect(feed.url.trim_start_matches("tcp://"))?;
    socket.set_read_timeout(Some(Duration::from_millis(10)))?;
    wasapi::initialize_mta().ok()?;
    let en = DeviceEnumerator::new()?;
    let device = if output.is_empty() {
        en.get_default_device(&Direction::Render)?
    } else {
        en.get_device(output)?
    };
    let mut client = device.get_iaudioclient()?;
    let format = WaveFormat::new(32, 32, &SampleType::Float, RATE as usize, CHANNELS, None);
    client.initialize_client(
        &format,
        &Direction::Render,
        &StreamMode::PollingShared {
            autoconvert: true,
            buffer_duration_hns: 400_000,
        },
    )?;
    let render = client.get_audiorenderclient()?;
    client.start_stream()?;
    let frame_bytes = CHANNELS * 4;
    // Keep at most ~60 ms queued: monitoring must stay immediate, so a
    // backlog (after a stall) is dropped rather than played late.
    let max_queue = RATE as usize * 60 / 1000 * frame_bytes;
    let mut queue: VecDeque<u8> = VecDeque::with_capacity(max_queue * 2);
    let mut input = [0u8; 4096];
    let mut pending: Vec<u8> = Vec::new();
    let began = Instant::now();
    let mut peak = 0f32;
    let mut last_level = Instant::now();
    while !stop.load(Ordering::Relaxed) && began.elapsed() < MONITOR_LIMIT {
        if let Some(error) = feed.meter.error.lock().unwrap().clone() {
            bail!("Microphone error: {error}")
        }
        match socket.read(&mut input) {
            Ok(0) => bail!("Audio transport closed unexpectedly"),
            Ok(n) => pending.extend_from_slice(&input[..n]),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.into()),
        }
        let whole = pending.len() / frame_bytes * frame_bytes;
        if whole > 0 {
            let mut samples: Vec<f32> = pending[..whole]
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            pending.drain(..whole);
            let g = f32::from_bits(gain.load(Ordering::Relaxed));
            for s in samples.iter_mut() {
                *s = (*s * g).clamp(-1.0, 1.0);
            }
            peak = samples.iter().fold(peak, |p, v| p.max(v.abs()));
            queue.extend(samples.iter().flat_map(|v| v.to_le_bytes()));
            if queue.len() > max_queue {
                let excess = (queue.len() - max_queue / 2) / frame_bytes * frame_bytes;
                queue.drain(..excess);
            }
        }
        let space = client.get_available_space_in_frames()? as usize;
        let frames = space.min(queue.len() / frame_bytes);
        if frames > 0 {
            render.write_to_device_from_deque(frames, &mut queue, None)?;
        }
        if last_level.elapsed() > Duration::from_millis(80) {
            on_level(peak);
            peak = 0.0;
            last_level = Instant::now();
        }
    }
    let _ = client.stop_stream();
    *meter_slot.lock().unwrap() = None;
    Ok(())
}

/// Write interleaved stereo 48kHz samples as a 16-bit PCM WAV file.
pub fn write_wav16(path: &std::path::Path, pcm: &[f32]) -> Result<()> {
    std::fs::write(path, wav16_bytes(pcm))?;
    Ok(())
}
/// Interleaved stereo 48kHz samples as 16-bit PCM WAV bytes.
pub fn wav16_bytes(pcm: &[f32]) -> Vec<u8> {
    let data: Vec<u8> = pcm
        .iter()
        .flat_map(|v| ((v.clamp(-1., 1.) * 32767.) as i16).to_le_bytes())
        .collect();
    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend((36 + data.len() as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes());
    wav.extend((CHANNELS as u16).to_le_bytes());
    wav.extend(RATE.to_le_bytes());
    wav.extend((RATE * 4).to_le_bytes());
    wav.extend(4u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend((data.len() as u32).to_le_bytes());
    wav.extend(data);
    wav
}

/// Endpoint mix formats (what Windows converts from), read without starting a stream.
pub fn device_formats() -> Result<Vec<(String, String, String)>> {
    wasapi::initialize_mta().ok()?;
    let en = DeviceEnumerator::new()?;
    let mut out = Vec::new();
    for (dir, kind) in [
        (Direction::Render, "system"),
        (Direction::Capture, "microphone"),
    ] {
        for d in &en.get_device_collection(&dir)? {
            let d = d?;
            let mut format = d
                .get_iaudioclient()
                .and_then(|c| c.get_mixformat())
                .map(|f| describe_format(&f))
                .unwrap_or_else(|e| format!("unavailable: {e}"));
            if kind == "microphone" {
                // Negotiate (never start) the native-channel stream REPLAYX uses.
                let opened = d.get_iaudioclient().and_then(|mut c| {
                    let n = c.get_mixformat()?.get_nchannels() as usize;
                    let f = WaveFormat::new(32, 32, &SampleType::Float, RATE as usize, n, None);
                    c.initialize_client(
                        &f,
                        &Direction::Capture,
                        &StreamMode::PollingShared {
                            autoconvert: true,
                            buffer_duration_hns: 1_000_000,
                        },
                    )?;
                    Ok(n)
                });
                format += &match opened {
                    Ok(n) => format!(
                        " | opens natively as {n}ch float → {}",
                        if n == 1 {
                            "duplicated"
                        } else if n == 2 {
                            "stereo"
                        } else {
                            "averaged mono"
                        }
                    ),
                    Err(e) => format!(" | native open failed ({e}); stereo fallback"),
                };
            }
            out.push((kind.to_owned(), d.get_friendlyname()?, format));
        }
    }
    Ok(out)
}
fn valid_process(s: &AudioSource) -> Result<()> {
    let mut sys = sysinfo::System::new();
    let pid = sysinfo::Pid::from_u32(s.pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
    let p = sys.process(pid).context(
        "Selected application exited. Select its new process before restarting capture.",
    )?;
    if p.start_time() != s.process_started {
        bail!("Selected PID has been reused. Select the application again.")
    }
    Ok(())
}
/// Apply the mix gain to one interleaved stereo chunk and return its peak.
/// The gain moves linearly from the previous chunk's value to the new one
/// across the chunk (10 ms), so muting, unmuting and volume changes during a
/// recording never produce a click.
pub fn apply_gain(chunk: &mut [f32], from: f32, to: f32) -> f32 {
    let frames = (chunk.len() / CHANNELS).max(1) as f32;
    let mut peak = 0f32;
    for (n, frame) in chunk.chunks_mut(CHANNELS).enumerate() {
        let gain = from + (to - from) * ((n + 1) as f32 / frames);
        for sample in frame {
            *sample = (*sample * gain).clamp(-4., 4.);
            peak = peak.max(sample.abs());
        }
    }
    peak
}
/// Windows audio errors that mean the device is changing or not ready yet
/// (AUDCLNT_E_DEVICE_INVALIDATED, _ENDPOINT_CREATE_FAILED,
/// _SERVICE_NOT_RUNNING, _RESOURCES_INVALIDATED, E_NOTFOUND): worth retrying
/// with the then-current default device, and a reason to reconnect, not to stop.
pub fn is_device_change(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "0x88890004",
        "0x8889000f",
        "0x88890010",
        "0x88890026",
        "0x80070490",
    ]
    .iter()
    .any(|code| m.contains(code))
}
pub fn spawn(source: AudioSource) -> Result<AudioFeed> {
    let name = source.name.clone();
    let microphone = source.kind == "microphone";
    let created = Instant::now();
    if source.kind == "application" || source.kind == "exclude" {
        valid_process(&source)?;
    }
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("tcp://127.0.0.1:{}", listener.local_addr()?.port());
    let stop = Arc::new(AtomicBool::new(false));
    let connected = Arc::new(AtomicBool::new(false));
    let meter = Arc::new(Meter::default());
    meter
        .volume
        .store(source.volume.to_bits(), Ordering::Relaxed);
    meter.muted.store(source.muted, Ordering::Relaxed);
    // 3s of 10ms chunks: FFmpeg may not read audio while it initialises GPU
    // devices; sustained stalls are caught by the 3s audio-write health check.
    let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(OUTPUT_CHANNEL_CHUNKS);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let st = stop.clone();
    let mt = meter.clone();
    let conn = connected.clone();
    let capture = thread::spawn(move || {
        let result = (|| -> Result<()> {
            wasapi::initialize_mta().ok()?;
            let stream = StreamMode::PollingShared {
                // Windows converts rate and sample type (AUTOCONVERTPCM +
                // SRC_DEFAULT_QUALITY); 100ms buffer tolerates stalls under game load.
                autoconvert: true,
                buffer_duration_hns: 1_000_000,
            };
            let float =
                |n: usize| WaveFormat::new(32, 32, &SampleType::Float, RATE as usize, n, None);
            // `channels` is the layout delivered by Windows; it is converted to
            // stereo by `to_stereo` before entering the timeline.
            // Opens the device, looking up the current default each time.
            let open = || -> Result<(AudioClient, usize)> {
                let en = DeviceEnumerator::new()?;
                Ok(
                    if source.kind == "application" || source.kind == "exclude" {
                        let mut c = AudioClient::new_application_loopback_client(
                            source.pid,
                            source.kind == "application",
                        )?;
                        c.initialize_client(&float(CHANNELS), &Direction::Capture, &stream)?;
                        (c, CHANNELS)
                    } else {
                        let dir = if source.kind == "microphone" {
                            Direction::Capture
                        } else {
                            Direction::Render
                        };
                        let device = if source.device.is_empty() {
                            en.get_default_device(&dir)?
                        } else {
                            en.get_device(&source.device)?
                        };
                        let mut c = device.get_iaudioclient()?;
                        let native = c.get_mixformat().ok();
                        if let Some(mix) = &native {
                            *mt.format.lock().unwrap() = Some(describe_format(mix));
                        }
                        // Microphones open at their native channel count so REPLAYX, not an
                        // unspecified channel matrix, decides how arrays become stereo.
                        // Loopback keeps Windows' mask-aware surround-to-stereo downmix.
                        let wanted = native
                            .filter(|_| source.kind == "microphone")
                            .map(|m| m.get_nchannels() as usize)
                            .filter(|n| (1..=MAX_DEVICE_CHANNELS).contains(n))
                            .unwrap_or(CHANNELS);
                        match c.initialize_client(&float(wanted), &Direction::Capture, &stream) {
                            Ok(()) => (c, wanted),
                            Err(_) if wanted != CHANNELS => {
                                let mut stereo = device.get_iaudioclient()?;
                                stereo.initialize_client(
                                    &float(CHANNELS),
                                    &Direction::Capture,
                                    &stream,
                                )?;
                                (stereo, CHANNELS)
                            }
                            Err(e) => return Err(e.into()),
                        }
                    },
                )
            };
            // A device that is switching (headset connecting, Bluetooth changing
            // profile, waking from sleep) answers "device invalidated" for a
            // moment: wait and try again with the then-current default device.
            let mut attempt = 0u64;
            let (client, channels) = loop {
                match open() {
                    Ok(opened) => break opened,
                    Err(e)
                        if attempt < 5
                            && is_device_change(&format!("{e:#}"))
                            && !st.load(Ordering::Relaxed) =>
                    {
                        attempt += 1;
                        thread::sleep(Duration::from_millis(250 * attempt));
                    }
                    Err(e) => return Err(e),
                }
            };
            if channels != CHANNELS {
                let mut f = mt.format.lock().unwrap();
                *f = Some(format!(
                    "{} → {}",
                    f.clone().unwrap_or_default(),
                    if channels == 1 {
                        "mono duplicated to both channels"
                    } else {
                        "channels averaged to centred mono"
                    }
                ));
            }
            let reader: IAudioCaptureClient = unsafe { client.as_iaudioclient().GetService()? };
            let _ = ready_tx.send(Ok::<(), String>(()));
            while !conn.load(Ordering::Relaxed) {
                if st.load(Ordering::Relaxed) {
                    return Ok(());
                };
                thread::sleep(Duration::from_millis(5));
            }
            let mut timeline = Timeline::new(qpc_hns());
            // Voice Enhancer for microphones (see crate::voice): runs here, on
            // each paced 10 ms chunk, never on the video path.
            let mut voice = (source.kind == "microphone"
                && (source.voice.active() || source.boost_db > 0.05))
                .then(|| crate::voice::Processor::new(&source.voice, source.boost_db));
            let mut mono = vec![0f32; CHUNK];
            // Gain of the previous chunk (mute and volume changes are ramped).
            let mut last_gain: Option<f32> = None;
            client.start_stream()?;
            let mut clock_base: Option<(u64, u64)> = None;
            let mut last_process_check = Instant::now();
            while !st.load(Ordering::Relaxed) {
                if last_process_check.elapsed() > Duration::from_secs(1)
                    && ["application", "exclude"].contains(&source.kind.as_str())
                {
                    valid_process(&source)?;
                    last_process_check = Instant::now();
                }
                while unsafe { reader.GetNextPacketSize()? } > 0 {
                    let (mut data, mut frames, mut flags, mut position, mut stamp) =
                        (std::ptr::null_mut(), 0, 0, 0, 0);
                    unsafe {
                        reader.GetBuffer(
                            &mut data,
                            &mut frames,
                            &mut flags,
                            Some(&mut position),
                            Some(&mut stamp),
                        )?
                    };
                    let sample_count = frames as usize * channels;
                    // Never dereference a silent WASAPI packet; Windows may return a null pointer.
                    let samples: Vec<f32> =
                        if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                            vec![0.; sample_count]
                        } else {
                            // The negotiated format is 32-bit IEEE float; decode explicitly as
                            // little-endian f32 without assuming buffer alignment.
                            unsafe { std::slice::from_raw_parts(data, sample_count * 4) }
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|b| f32::from_le_bytes(*b))
                                .collect()
                        };
                    unsafe { reader.ReleaseBuffer(frames)? };
                    let samples = to_stereo(samples, channels);
                    // Windows commonly flags the first packet after Start; only later
                    // flags indicate lost device data.
                    if flags & AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32 != 0 {
                        if mt.packets.load(Ordering::Relaxed) > 0 {
                            mt.device_glitches.fetch_add(1, Ordering::Relaxed);
                        }
                        clock_base = None;
                    }
                    let reliable = flags & AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32 == 0;
                    if timeline.push(&samples, reliable.then_some(stamp))? {
                        clock_base = None;
                    }
                    // Independent drift measurement from device frame positions.
                    match clock_base {
                        None if reliable => clock_base = Some((position, stamp)),
                        Some((p0, s0)) if stamp > s0 + 50_000_000 && position > p0 => {
                            let measured = (position - p0) as f64
                                / RATE as f64
                                / ((stamp - s0) as f64 / HNS_PER_SECOND);
                            mt.clock_ppm.store(
                                (((measured - 1.) * 1e6) as f32).to_bits(),
                                Ordering::Relaxed,
                            );
                        }
                        _ => {}
                    }
                    let s = timeline.stats;
                    mt.discontinuities
                        .store(s.realignments + s.late_after_starvation, Ordering::Relaxed);
                    mt.timestamp_errors
                        .store(s.timestamp_errors, Ordering::Relaxed);
                    mt.correction_ppm.store(
                        (timeline.correction_ppm() as f32).to_bits(),
                        Ordering::Relaxed,
                    );
                    mt.packets.fetch_add(1, Ordering::Relaxed);
                    mt.last_packet_ms
                        .store(created.elapsed().as_millis() as u64, Ordering::Relaxed);
                }
                // Idle loopback (no packets) is valid silence and is paced out as zeros.
                while let Some(mut chunk) = timeline.pull(qpc_hns()) {
                    if let Some((cfg, boost)) = mt.voice_update.lock().unwrap().take() {
                        match voice.as_mut() {
                            Some(p) => p.configure(&cfg, boost),
                            None => voice = Some(crate::voice::Processor::new(&cfg, boost)),
                        }
                    }
                    if let Some(p) = voice.as_mut().filter(|_| chunk.len() == CHUNK * CHANNELS) {
                        for (m, f) in mono.iter_mut().zip(chunk.chunks(CHANNELS)) {
                            *m = (f[0] + f[1]) * 0.5;
                        }
                        let raw = mono.clone();
                        p.process(&mut mono);
                        if mt.bypass.load(Ordering::Relaxed) {
                            // Original, as loud as the enhanced voice (fair A/B).
                            let g = p.match_gain();
                            for (m, r) in mono.iter_mut().zip(&raw) {
                                *m = (r * g).clamp(-0.99, 0.99);
                            }
                        }
                        for (f, m) in chunk.chunks_mut(CHANNELS).zip(&mono) {
                            f[0] = *m;
                            f[1] = *m;
                        }
                        mt.dsp_faults.store(p.faults, Ordering::Relaxed);
                    }
                    let gain = if mt.muted.load(Ordering::Relaxed) {
                        0.
                    } else {
                        f32::from_bits(mt.volume.load(Ordering::Relaxed))
                    };
                    // Float PCM keeps headroom above full scale: a source
                    // turned above 100% is limited smoothly in FFmpeg
                    // (media::source_chain) instead of hard-clipped here.
                    let peak = apply_gain(&mut chunk, last_gain.unwrap_or(gain), gain);
                    last_gain = Some(gain);
                    mt.peak.store(peak.to_bits(), Ordering::Relaxed);
                    let chunk: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
                    match tx.try_send(chunk) {
                        Ok(()) => {}
                        Err(mpsc::TrySendError::Full(_)) => {
                            mt.gaps.fetch_add(1, Ordering::Relaxed);
                            bail!("Audio output backpressure exceeded 10 s: FFmpeg stopped reading audio")
                        }
                        Err(_) => return Ok(()),
                    }
                }
                thread::sleep(Duration::from_millis(4));
            }
            client.stop_stream()?;
            Ok(())
        })();
        if let Err(e) = result {
            let message = format!("{}: {e}", source.name);
            *mt.error.lock().unwrap() = Some(message.clone());
            let _ = ready_tx.try_send(Err(message));
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(8)) {
        Ok(Ok(())) => {}
        result => {
            stop.store(true, Ordering::Relaxed);
            match result {
                Ok(Err(message)) if is_device_change(&message) => bail!(
                    "{message}. The audio device was changing or not ready, and still wasn't after several retries. Check Windows Sound settings (or reconnect the headset) and start capture again."
                ),
                Ok(Err(message)) => bail!("Audio could not start: {message}"),
                _ => bail!("{name}: the audio device did not respond within 8 seconds. Check Windows Sound settings and start capture again."),
            }
        }
    }
    let st = stop.clone();
    let mt = meter.clone();
    let writer = thread::spawn(move || {
        let result = (|| -> Result<()> {
            let deadline = Instant::now() + Duration::from_secs(12);
            let mut socket = loop {
                if st.load(Ordering::Relaxed) {
                    return Ok(());
                }
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e.into()),
                }
                if Instant::now() > deadline {
                    bail!("FFmpeg did not connect to audio")
                };
                thread::sleep(Duration::from_millis(10));
            };
            socket.set_nonblocking(false)?;
            // Backstop only: FFmpeg may pause reading while it initialises GPU
            // devices. Real stalls are reported by the engine's 3s write check.
            socket.set_write_timeout(Some(Duration::from_secs(10)))?;
            socket.set_nodelay(true)?;
            connected.store(true, Ordering::Relaxed);
            while !st.load(Ordering::Relaxed) {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(data) => {
                        socket.write_all(&data)?;
                        mt.written_bytes
                            .fetch_add(data.len() as u64, Ordering::Relaxed);
                        mt.last_write_ms
                            .store(created.elapsed().as_millis() as u64, Ordering::Relaxed);
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(_) => break,
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            if !st.load(Ordering::Relaxed) {
                *mt.error.lock().unwrap() = Some(format!("Audio transport stopped: {e}"));
            }
        }
    });
    Ok(AudioFeed {
        url,
        meter,
        stop,
        threads: vec![capture, writer],
        started: created,
        microphone,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn muting_and_unmuting_ramp_without_a_click() {
        // A full-scale tone muted mid-recording: the last chunk fades to
        // silence instead of jumping, and everything after it is silent.
        let tone = |n: usize| -> Vec<f32> {
            (0..n * CHANNELS)
                .map(|i| ((i / CHANNELS) as f32 * 0.2).sin())
                .collect()
        };
        let mut fading = tone(480);
        apply_gain(&mut fading, 1.0, 0.0);
        let step = fading
            .chunks(CHANNELS)
            .zip(fading.chunks(CHANNELS).skip(1))
            .map(|(a, b)| (a[0] - b[0]).abs())
            .fold(0f32, f32::max);
        assert!(step < 0.25, "largest sample-to-sample step {step}");
        assert_eq!(fading[fading.len() - 1], 0.0, "ends in silence");
        let mut silent = tone(480);
        assert_eq!(
            apply_gain(&mut silent, 0.0, 0.0),
            0.0,
            "muted: nothing is captured"
        );
        assert!(silent.iter().all(|v| *v == 0.0));
        // Unmuting fades in from silence; a steady gain changes nothing.
        let mut rising = tone(480);
        apply_gain(&mut rising, 0.0, 1.0);
        assert!(rising[0].abs() < 0.01 && rising[1].abs() < 0.01);
        let (mut steady, original) = (tone(480), tone(480));
        apply_gain(&mut steady, 1.0, 1.0);
        assert_eq!(steady, original);
        // Both channels of a frame get the same gain (no stereo shift).
        let mut pair = vec![1.0f32; 960];
        apply_gain(&mut pair, 1.0, 0.0);
        assert!(pair.chunks(CHANNELS).all(|f| f[0] == f[1]));
    }
    #[test]
    fn device_changes_are_retried_and_reconnected_not_fatal() {
        // The error from the report: a device switching while it was opened.
        assert!(is_device_change(
            "System audio: Windows returned an error: 0x88890004"
        ));
        assert!(is_device_change(
            "Microphone: Windows returned an error: 0x88890026"
        ));
        assert!(is_device_change(
            "System audio: Element not found. (0x80070490)"
        ));
        // Real failures are still reported as errors.
        assert!(!is_device_change(
            "Microphone: Access is denied. (0x80070005)"
        ));
        assert!(!is_device_change(
            "Selected PID has been reused. Select the application again."
        ));
    }
    use std::f64::consts::TAU;

    const ORIGIN: u64 = 1_000_000_000;
    const AMP: f64 = 0.5;
    type Packet = (Vec<f32>, Option<u64>, u64);

    struct Lcg(u64);
    impl Lcg {
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Simulated device: 480-frame packets of L=1kHz/R=1.5kHz tones whose
    /// sample clock runs `drift_ppm` relative to QPC. Stamps carry +/-`jitter`
    /// frames of noise and are truncated to whole 100ns units, like WASAPI.
    fn device(seconds: f64, drift_ppm: f64, jitter: f64, right: bool) -> Vec<Packet> {
        let rate = RATE as f64 * (1. + drift_ppm * 1e-6);
        let mut rng = Lcg(7);
        let mut out = Vec::new();
        let mut k = 0u64;
        while (k as f64) < seconds * rate {
            let mut samples = Vec::with_capacity(CHUNK * 2);
            for n in k..k + CHUNK as u64 {
                let t = n as f64 / RATE as f64;
                samples.push((AMP * (TAU * 1000. * t).sin()) as f32);
                samples.push(if right {
                    (AMP * (TAU * 1500. * t).sin()) as f32
                } else {
                    0.
                });
            }
            let true_hns = ORIGIN as f64 + 200_000. + k as f64 / rate * HNS_PER_SECOND;
            let noise = (rng.unit() * 2. - 1.) * jitter / RATE as f64 * HNS_PER_SECOND;
            let arrival = true_hns + 100_000. + 20_000. + rng.unit() * 60_000.;
            out.push((samples, Some((true_hns + noise) as u64), arrival as u64));
            k += CHUNK as u64;
        }
        out
    }

    fn run(packets: &[Packet], end_hns: u64) -> (Vec<f32>, Timeline) {
        let mut timeline = Timeline::new(ORIGIN);
        let mut output = Vec::new();
        let mut next = 0;
        let mut now = ORIGIN;
        while now < end_hns {
            while next < packets.len() && packets[next].2 <= now {
                timeline.push(&packets[next].0, packets[next].1).unwrap();
                next += 1;
            }
            while let Some(chunk) = timeline.pull(now) {
                output.extend(chunk);
            }
            now += 40_000;
        }
        (output, timeline)
    }

    fn end_of(packets: &[Packet]) -> u64 {
        packets.last().unwrap().2 + 300_000
    }

    /// Largest |second difference| of one channel between frames `from..to`.
    /// A clean sine of angular step w is bounded by AMP*(2 sin(w/2))^2; a single
    /// inserted/dropped frame produces roughly AMP*w, about 8x larger at 1kHz.
    fn max_d2(out: &[f32], channel: usize, from: usize, to: usize) -> f64 {
        (from + 2..to)
            .map(|n| {
                let x = |i: usize| out[i * 2 + channel] as f64;
                (x(n) - 2. * x(n - 1) + x(n - 2)).abs()
            })
            .fold(0., f64::max)
    }

    fn sine_bound(freq: f64, drift_ppm: f64) -> f64 {
        let w = TAU * freq * (1. + drift_ppm * 1e-6) / RATE as f64;
        AMP * (2. * (w / 2.).sin()).powi(2) * 1.02 + 2e-5
    }

    fn active_range(out: &[f32]) -> (usize, usize) {
        let frames = out.len() / 2;
        let first = (0..frames).find(|&n| out[n * 2] != 0.).unwrap();
        let last = (0..frames).rev().find(|&n| out[n * 2] != 0.).unwrap();
        (first + 64, last - 64)
    }

    #[test]
    fn resampler_unity_ratio_is_bit_exact() {
        let input: Vec<f32> = (0..9600)
            .map(|i| ((i * 7919) % 2001) as f32 / 1000. - 1.)
            .collect();
        let mut r = Resampler::new();
        let mut out = VecDeque::new();
        for block in input.chunks(960) {
            r.process(block, &mut out);
        }
        let out: Vec<f32> = out.into_iter().collect();
        assert!(out.len() >= input.len() - 2 * HALF_TAPS * 2);
        assert_eq!(&out[..], &input[..out.len()]);
    }

    #[test]
    fn resampler_small_ratio_matches_ideal_interpolation() {
        let input: Vec<f32> = (0..48_000)
            .flat_map(|n| {
                let v = (AMP * (TAU * 1000. * n as f64 / RATE as f64).sin()) as f32;
                [v, -v]
            })
            .collect();
        let mut r = Resampler::new();
        r.set_ratio(1.002);
        let mut out = VecDeque::new();
        for block in input.chunks(960) {
            r.process(block, &mut out);
        }
        let step = 1. / 1.002;
        let mut worst = 0f64;
        for n in 64..out.len() / 2 - 64 {
            let ideal = AMP * (TAU * 1000. * (n as f64 * step) / RATE as f64).sin();
            worst = worst.max((out[n * 2] as f64 - ideal).abs());
            assert_eq!(out[n * 2 + 1], -out[n * 2]);
        }
        // Better than -80dB relative to the tone.
        assert!(worst < AMP * 1e-4, "interpolation error {worst}");
    }

    #[test]
    fn jittered_timestamps_do_not_insert_or_drop_samples() {
        // Regression: per-packet QPC re-placement turned this jitter into
        // inserted/dropped frames at nearly every 10ms packet boundary.
        let packets = device(20., 0., 20., true);
        let (out, t) = run(&packets, end_of(&packets));
        let (from, to) = active_range(&out);
        assert!(max_d2(&out, 0, from, to) < sine_bound(1000., 0.));
        assert!(max_d2(&out, 1, from, to) < sine_bound(1500., 0.));
        assert_eq!(t.stats.realignments, 0);
        assert_eq!(t.stats.late_after_starvation, 0);
        assert_eq!(t.stats.dropped_frames, 0);
        // Only the initial anchor silence is inserted.
        assert!(t.stats.inserted_silence_frames < RATE as u64 / 10);
    }

    #[test]
    fn clock_drift_is_corrected_smoothly() {
        for drift in [-300., 300.] {
            let packets = device(120., drift, 10., true);
            let (out, t) = run(&packets, end_of(&packets));
            let (from, to) = active_range(&out);
            assert!(
                max_d2(&out, 0, from, to) < sine_bound(1000., drift),
                "drift {drift}"
            );
            assert!(
                max_d2(&out, 1, from, to) < sine_bound(1500., drift),
                "drift {drift}"
            );
            assert_eq!(t.stats.realignments, 0, "drift {drift}");
            assert_eq!(t.stats.late_after_starvation, 0, "drift {drift}");
            // Converged: correction cancels the device drift and the timing error
            // (its contribution to A/V offset) stays within a few milliseconds.
            assert!(
                (t.correction_ppm() + drift).abs() < 30.,
                "{}",
                t.correction_ppm()
            );
            assert!(
                t.error_frames.abs() < RATE as f64 * 0.01,
                "{}",
                t.error_frames
            );
            // Output length tracks the QPC clock, not the device clock.
            let expected =
                (end_of(&packets) - ORIGIN - MIN_HOLDBACK_HNS) as f64 / 1e7 * RATE as f64;
            assert!((out.len() as f64 / 2. - expected).abs() <= CHUNK as f64);
        }
    }

    #[test]
    fn stereo_channels_remain_isolated() {
        let packets = device(10., 250., 10., false);
        let (out, _) = run(&packets, end_of(&packets));
        assert!(out.iter().step_by(2).any(|v| v.abs() > 0.4));
        assert!(out.iter().skip(1).step_by(2).all(|v| *v == 0.));
    }

    #[test]
    fn idle_loopback_gap_is_silence_and_resumes_at_its_timestamp() {
        let mut packets = device(6., 0., 5., true);
        // No packets for 1.5s (nothing playing on the endpoint).
        let gap = 200..350;
        let resume_stamp = packets[gap.end].1.unwrap();
        packets.drain(gap);
        let (out, t) = run(&packets, end_of(&packets));
        let resume = ((resume_stamp - ORIGIN) as f64 * RATE as f64 / 1e7).round() as usize;
        let gap_start = 200 * CHUNK + 9600;
        let stray: Vec<_> = (gap_start..resume - HALF_TAPS)
            .filter(|n| out[n * 2] != 0. || out[n * 2 + 1] != 0.)
            .collect();
        assert!(
            stray.is_empty(),
            "gap {gap_start}..{resume} stray {} first {:?} last {:?} stats {:?}",
            stray.len(),
            stray.first(),
            stray.last(),
            t.stats
        );
        let onset = (resume - 4..out.len() / 2)
            .find(|&n| out[n * 2 + 1] != 0.)
            .unwrap();
        assert!(onset.abs_diff(resume) <= 6, "onset {onset} vs {resume}");
        assert_eq!(t.stats.realignments, 0);
        assert_eq!(t.stats.late_after_starvation, 0);
    }

    #[test]
    fn delivery_stall_beyond_holdback_is_reported() {
        let mut packets = device(4., 0., 5., true);
        // The capture thread stalls for 150ms; queued packets then arrive in order.
        let resume = packets[100].2 + 1_500_000;
        for p in &mut packets[100..] {
            p.2 = p.2.max(resume);
        }
        let (out, t) = run(&packets, end_of(&packets));
        assert_eq!(t.stats.late_after_starvation, 1);
        assert_eq!(t.stats.realignments, 0);
        // Alignment is restored by discarding the stale audio, then it is clean.
        let (_, to) = active_range(&out);
        let after = (resume as f64 - ORIGIN as f64) / 1e7 * RATE as f64;
        assert!(max_d2(&out, 0, after as usize + 4800, to) < sine_bound(1000., 0.));
    }

    #[test]
    fn gross_backward_timestamp_jump_realigns_once() {
        let mut packets = device(4., 0., 5., true);
        for p in &mut packets[150..] {
            p.1 = p.1.map(|s| s - 800_000);
        }
        let (out, t) = run(&packets, end_of(&packets));
        assert_eq!(t.stats.realignments, 1);
        assert_eq!(t.stats.late_after_starvation, 0);
        // 80ms of stale audio, plus any silence the 50ms queue could not cover.
        let dropped = t.stats.dropped_frames as usize;
        assert!((3800..3840 + 2400 + CHUNK).contains(&dropped), "{dropped}");
        // Packet 200 (first frame 96000 at stamp-80ms) lands where its stamp says.
        let landed = ((packets[200].1.unwrap() - ORIGIN) as f64 / 1e7 * RATE as f64) as i64;
        let misfit = |offset: i64| -> f64 {
            (0..96)
                .map(|j| {
                    let n = (landed + offset + j) as usize;
                    let ideal = AMP * (TAU * 1000. * (96_000 + j) as f64 / RATE as f64).sin();
                    (out[n * 2] as f64 - ideal).abs()
                })
                .sum()
        };
        let best = (-20..=20)
            .min_by(|a, b| misfit(*a).total_cmp(&misfit(*b)))
            .unwrap();
        // Sub-frame placement error alone allows ~AMP*w/2 per sample.
        assert!(
            best.abs() <= 8 && misfit(best) < 96. * 0.04,
            "offset {best}"
        );
    }

    #[test]
    fn implausible_or_missing_timestamps_keep_audio_contiguous() {
        let mut packets = device(4., 0., 5., true);
        packets[120].1 = Some(packets[120].1.unwrap() + 100_000_000);
        packets[121].1 = None;
        packets[122].1 = Some(0);
        let (out, t) = run(&packets, end_of(&packets));
        let (from, to) = active_range(&out);
        assert!(max_d2(&out, 0, from, to) < sine_bound(1000., 0.));
        assert_eq!(t.stats.timestamp_errors, 3);
        assert_eq!(t.stats.realignments, 0);
    }

    #[test]
    fn high_latency_device_adapts_holdback_without_dropouts() {
        // Packets arrive 120ms after capture (e.g. Bluetooth microphone).
        let mut packets = device(20., 0., 5., true);
        for p in &mut packets {
            p.2 += 1_200_000;
        }
        let (out, t) = run(&packets, end_of(&packets));
        let (from, to) = active_range(&out);
        assert!(max_d2(&out, 0, from, to) < sine_bound(1000., 0.));
        assert_eq!(t.stats.late_after_starvation, 0);
        assert_eq!(t.stats.realignments, 0);
        assert!(t.holdback_ms() > 120. && t.holdback_ms() < 200.);
        // Only startup audio older than the adapted holdback is discarded.
        assert!(t.stats.dropped_frames < RATE as u64 / 5);
    }

    #[test]
    fn device_layouts_convert_to_stereo() {
        assert_eq!(
            to_stereo(vec![0.1, 0.2, 0.3, 0.4], 2),
            vec![0.1, 0.2, 0.3, 0.4]
        );
        assert_eq!(to_stereo(vec![0.5, -0.25], 1), vec![0.5, 0.5, -0.25, -0.25]);
        // A 4-capsule array frame: coherent voice keeps its level.
        assert_eq!(to_stereo(vec![0.4, 0.4, 0.4, 0.4], 4), vec![0.4, 0.4]);
        let out = to_stereo(vec![1.0, 0.0, 0.0, -1.0, 0.2, 0.2, 0.2, 0.2], 4);
        assert_eq!(out, vec![0.0, 0.0, 0.2, 0.2]);
        // Uncorrelated noise: averaging N channels lowers RMS by about sqrt(N).
        let mut rng = Lcg(3);
        let noise: Vec<f32> = (0..48_000 * 4)
            .map(|_| (rng.unit() * 2. - 1.) as f32)
            .collect();
        let rms = |v: &[f32]| {
            (v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        let ratio = rms(&noise) / rms(&to_stereo(noise.clone(), 4));
        assert!((ratio - 2.0).abs() < 0.05, "{ratio}");
        // Partial trailing frames are dropped, never misaligned.
        assert_eq!(to_stereo(vec![0.1; 6], 4).len(), 2);
    }

    #[test]
    fn unconsumed_queue_is_bounded() {
        let mut timeline = Timeline::new(ORIGIN);
        let packet = vec![0.25f32; CHUNK * 2];
        let failed = (0..40).any(|_| timeline.push(&packet, None).is_err());
        assert!(failed);
    }
}
