//! Development prototype of a native capture-to-encode path (not used by the
//! recorder). Step 1, `--feasibility`: can the hardware H.264 encoder take
//! the fixed-function converter's D3D11 NV12 textures directly, without any
//! CPU readback? Nothing is captured and nothing is shown: frames are
//! synthetic, encoded to a raw H.264 file for FFmpeg to verify.
//!
//! usage: replayx-native-proto --feasibility OUT.h264 [--frames 600] [--paced 0|1] [--vendor 8086]
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    fs::OpenOptions,
    io::{BufWriter, Write},
    mem::ManuallyDrop,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{Interface, GUID, PWSTR},
    Win32::{
        Foundation::{HMODULE, VARIANT_BOOL},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_NV12},
        },
        Media::MediaFoundation::*,
        System::{
            Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED},
            Threading::{GetCurrentProcess, GetProcessTimes},
            Variant::{VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4},
        },
    },
};

#[path = "../native_worker/cadence.rs"]
mod cadence;
#[path = "../native_worker/common.rs"]
mod common;
#[path = "../native_worker/grid.rs"]
mod grid;
#[path = "../native_worker/h264.rs"]
mod h264;
#[path = "../native_worker/lifetime.rs"]
mod lifetime;
#[path = "../native_worker/output.rs"]
mod output;
#[path = "../native_worker/phase.rs"]
mod phase;
#[path = "../native_worker/select.rs"]
mod select;
#[path = "../native_worker/timeline.rs"]
mod timeline;
#[path = "../native_worker/trace.rs"]
mod trace;
#[path = "../native_worker/transport.rs"]
mod transport;
use common::{adapter_by_vendor, texture, Native};
use select::{CfrSelector, Source};

const DIAGNOSTIC_SAMPLES: usize = 4096;
fn bounded_push<T>(samples: &mut Vec<T>, value: T) {
    if samples.len() < DIAGNOSTIC_SAMPLES {
        samples.push(value);
    }
}
struct CaptureFrame(windows::Graphics::Capture::Direct3D11CaptureFrame);
impl std::ops::Deref for CaptureFrame {
    type Target = windows::Graphics::Capture::Direct3D11CaptureFrame;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl Drop for CaptureFrame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}
struct WinrtRuntime;
impl Drop for WinrtRuntime {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::System::WinRT::RoUninitialize();
        }
    }
}

/// The parent owns the worker's job. A driver call stuck inside ProcessInput
/// or Shutdown cannot hold the command indefinitely or outlive its parent.
fn supervise(timeout: Duration) -> Result<()> {
    use std::{os::windows::process::CommandExt, process::Command};
    let mut child = Command::new(std::env::current_exe()?)
        .args(std::env::args().skip(1))
        .arg("--native-worker")
        .creation_flags(0x0800_0000)
        .spawn()?;
    let job = match replayx_core::process_job::CaptureJob::attach(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let deadline = Instant::now() + Duration::from_secs(5);
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            return Err(error);
        }
    };
    let mut deadline = Instant::now() + timeout;
    let pid = argument("--pid", 0u32)?;
    let mut last_guard = Instant::now();
    let stop_file: Option<std::path::PathBuf> = std::env::args()
        .position(|a| a == "--stop-file")
        .and_then(|i| std::env::args().nth(i + 1))
        .map(Into::into);
    loop {
        if let Some(exit) = child.try_wait()? {
            drop(job);
            if !exit.success() {
                bail!("Native worker failed ({exit})")
            }
            return Ok(());
        }
        if pid != 0 && last_guard.elapsed() >= Duration::from_millis(250) {
            last_guard = Instant::now();
            if window_in_front(pid).is_none() {
                deadline = deadline.min(Instant::now() + Duration::from_secs(5));
            }
        }
        if stop_file.as_ref().is_some_and(|p| p.exists()) {
            deadline = deadline.min(Instant::now() + Duration::from_secs(5));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            let killed = Instant::now() + Duration::from_secs(5);
            while child.try_wait()?.is_none() && Instant::now() < killed {
                std::thread::sleep(Duration::from_millis(10));
            }
            drop(job);
            bail!("Native worker exceeded its shutdown deadline and was terminated; partial output retained")
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn argument<T: std::str::FromStr>(key: &str, default: T) -> Result<T> {
    let args: Vec<_> = std::env::args().collect();
    match args.iter().position(|s| s == key) {
        Some(i) => args
            .get(i + 1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("Invalid {key}")),
        None => Ok(default),
    }
}

fn stats(mut values: Vec<f64>) -> serde_json::Value {
    if values.is_empty() {
        return serde_json::Value::Null;
    }
    values.sort_by(f64::total_cmp);
    json!({"samples":values.len(), "mean_ms":values.iter().sum::<f64>()/values.len() as f64,
        "p50_ms":values[(values.len()-1)/2], "p95_ms":values[(values.len()-1)*95/100],
        "p99_ms":values[(values.len()-1)*99/100], "max_ms":values.last()})
}

/// CPU time (user + kernel) of the calling thread, in milliseconds: the
/// recording loop's own work, without the encoder's internal threads.
fn thread_cpu_ms() -> f64 {
    use windows::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};
    let (mut c, mut e, mut k, mut u) = Default::default();
    unsafe {
        let _ = GetThreadTimes(GetCurrentThread(), &mut c, &mut e, &mut k, &mut u);
    }
    let t = |f: windows::Win32::Foundation::FILETIME| {
        ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64
    };
    (t(k) + t(u)) / 10_000.
}

/// CPU time (user + kernel) of this process, in milliseconds.
fn cpu_ms() -> f64 {
    let (mut c, mut e, mut k, mut u) = Default::default();
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
    }
    let t = |f: windows::Win32::Foundation::FILETIME| {
        ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) as f64
    };
    (t(k) + t(u)) / 10_000.
}

fn variant(vt: VARENUM, value: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: value,
            }),
        },
    }
}

/// Encoder settings that mirror REPLAYX's 1080p60 Balanced H.264 recording.
pub struct EncoderSettings {
    pub size: (u32, u32),
    pub fps: u32,
    pub bitrate: u32,
    pub max_bitrate: u32,
    pub gop: u32,
}

/// A hardware H.264 encoder (Media Foundation transform, asynchronous) fed
/// D3D11 NV12 textures from the same device: no CPU copy of any frame.
pub struct MfEncoder {
    transform: IMFTransform,
    _shutdown: TransformShutdown,
    events: IMFMediaEventGenerator,
    provides_samples: bool,
    output_buffer_size: u32,
    input_credits: u32,
    pub trace_log: Option<trace::Log>,
    pub name: String,
    pub vendor: String,
    pub adapter_luid: u64,
    pub settings_accepted: Vec<(String, bool)>,
    output: BufWriter<output::Target>,
    transport: Option<transport::Transport>,
    pub bytes_written: u64,
    busy: HashMap<usize, Arc<AtomicUsize>>,
    stop_file: Option<std::path::PathBuf>,
    guard_pid: Option<u32>,
    last_guard: Instant,
    cancelled: bool,
    graceful_stop: bool,
    stop_requested: bool,
    last_flush: Instant,
    simulate_stall: bool,
    live_capture_input: bool,
    release_delay: Duration,
    surface_waits: u64,
    last_output_time: Option<i64>,
    timestamps_increasing: bool,
    timestamps_regular: bool,
    expected_step: i64,
    clock_fps: u32,
    expected_gop: usize,
    rational_clock: bool,
    capture_vfr: bool,
    submitted_count: usize,
    expected_output: VecDeque<(i64, bool)>,
    next_key_time: i64,
    presentation_end: i64,
    keyframe_count: usize,
    pub frames_out: usize,
    pub keyframes: Vec<usize>,
    pub output_times: Vec<i64>,
    submitted: HashMap<i64, Instant>,
    pub latency_ms: Vec<f64>,
    /// Recording-thread CPU in encode(): waiting for an input request,
    /// wrapping the texture, ProcessInput, collecting output.
    pub cpu_parts: [f64; 4],
}

impl MfEncoder {
    /// Hardware H.264 encoders Windows offers: (name, vendor, D3D11 aware).
    unsafe fn list() -> Result<Vec<(String, String, bool, IMFActivate)>> {
        Self::list_for_adapter(None)
    }
    unsafe fn list_for_adapter(
        adapter: Option<u64>,
    ) -> Result<Vec<(String, String, bool, IMFActivate)>> {
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };
        let mut ptr: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        if let Some(luid) = adapter {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1)?;
            let attrs = attrs.unwrap();
            // MFTEnum2 takes the 8-byte Windows LUID as a blob (LowPart, HighPart).
            attrs.SetBlob(&MFT_ENUM_ADAPTER_LUID, &luid.to_le_bytes())?;
            MFTEnum2(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&input),
                Some(&output),
                &attrs,
                &mut ptr,
                &mut count,
            )?;
        } else {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&input),
                Some(&output),
                &mut ptr,
                &mut count,
            )?;
        }
        let mut found = Vec::new();
        if !ptr.is_null() {
            let all = std::slice::from_raw_parts_mut(ptr, count as usize);
            for slot in all.iter_mut() {
                let Some(activate) = slot.take() else {
                    continue;
                };
                let text = |key: &GUID| {
                    let mut value = PWSTR::null();
                    let mut len = 0u32;
                    if activate
                        .GetAllocatedString(key, &mut value, &mut len)
                        .is_ok()
                    {
                        let s = value.to_string().unwrap_or_default();
                        CoTaskMemFree(Some(value.0 as _));
                        s
                    } else {
                        String::new()
                    }
                };
                let name = text(&MFT_FRIENDLY_NAME_Attribute);
                let vendor = text(&MFT_ENUM_HARDWARE_VENDOR_ID_Attribute);
                let d3d11 = activate.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) != 0;
                found.push((name, vendor, d3d11, activate));
            }
            CoTaskMemFree(Some(ptr as _));
        }
        Ok(found)
    }

    unsafe fn new(
        device: &ID3D11Device,
        vendor: &str,
        s: &EncoderSettings,
        out_path: &str,
    ) -> Result<Self> {
        let desc = device
            .cast::<windows::Win32::Graphics::Dxgi::IDXGIDevice>()?
            .GetAdapter()?
            .GetDesc()?;
        let adapter_luid =
            ((desc.AdapterLuid.HighPart as u32 as u64) << 32) | desc.AdapterLuid.LowPart as u64;
        let encoders = Self::list_for_adapter(Some(adapter_luid)).with_context(|| {
            format!("Enumerate {vendor} hardware MFTs on adapter {adapter_luid:016X}")
        })?;
        let (name, vendor_id, transform) = replayx_core::native_gpu::first_activatable_encoder(
            encoders
                .into_iter()
                .filter(|(_, id, _, _)| id.eq_ignore_ascii_case(vendor))
                .map(|(name, id, _, activate)| (name, id, activate)),
            |activate| {
                activate
                    .ActivateObject::<IMFTransform>()
                    .map_err(Into::into)
            },
        )
        .with_context(|| format!("{vendor} hardware H.264 on adapter {adapter_luid:016X}"))?;
        let shutdown = TransformShutdown(transform.clone());
        let attributes = transform.GetAttributes()?;
        // Encoders report Direct3D 11 support on the transform itself.
        if attributes.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) == 0 {
            bail!("{name} does not accept Direct3D 11 surfaces")
        }
        attributes.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
        // The converter's device: the encoder reads its NV12 textures directly.
        let mut token = 0u32;
        let mut manager = None;
        MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
        let manager = manager.unwrap();
        manager.ResetDevice(device, token)?;
        transform
            .ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, manager.as_raw() as usize)
            .with_context(|| format!("Bind capture D3D11 device to {name}"))?;

        let frame_size = ((s.size.0 as u64) << 32) | s.size.1 as u64;
        let frame_rate = ((s.fps as u64) << 32) | 1;
        let colour = |t: &IMFMediaType| -> Result<()> {
            t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
            t.SetUINT64(&MF_MT_FRAME_SIZE, frame_size)?;
            t.SetUINT64(&MF_MT_FRAME_RATE, frame_rate)?;
            t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
            // BT.709, limited range: what the converter writes and REPLAYX tags.
            t.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32)?;
            t.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32)?;
            t.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32)?;
            t.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)?;
            Ok(())
        };
        // Rate control and GOP as REPLAYX's Balanced 1080p60 recording:
        // peak-constrained VBR, 2 s between keyframes (segments start on
        // one), no B-frames.
        let configure = || {
            let mut settings_accepted = Vec::new();
            if let Ok(api) = transform.cast::<ICodecAPI>() {
                let u4 = |v: u32| variant(VT_UI4, VARIANT_0_0_0 { ulVal: v });
                let mut set = |label: &str, key: &GUID, value: VARIANT| {
                    let result = api.SetValue(key, &value);
                    if let Err(error) = &result {
                        eprintln!("Encoder property {label} rejected: {error}; supported={:?}, modifiable={:?}, current={:?}", api.IsSupported(key), api.IsModifiable(key), api.GetValue(key).map(|v| (v.Anonymous.Anonymous.vt.0,v.Anonymous.Anonymous.Anonymous.ulVal)));
                    }
                    let enforced =
                        replayx_core::native_gpu::control_satisfied(label, result.is_ok(), true);
                    if !enforced {
                        eprintln!("Required native codec control is unavailable: {label}");
                    }
                    settings_accepted.push((label.to_string(), result.is_ok()));
                };
                set(
                    "rate_control=peak_constrained_vbr",
                    &CODECAPI_AVEncCommonRateControlMode,
                    u4(eAVEncCommonRateControlMode_PeakConstrainedVBR.0 as u32),
                );
                set(
                    "mean_bitrate",
                    &CODECAPI_AVEncCommonMeanBitRate,
                    u4(s.bitrate),
                );
                set(
                    "max_bitrate",
                    &CODECAPI_AVEncCommonMaxBitRate,
                    u4(s.max_bitrate),
                );
                set("gop_size", &CODECAPI_AVEncMPVGOPSize, u4(s.gop));
                set("b_frames=0", &CODECAPI_AVEncMPVDefaultBPictureCount, u4(0));
                set(
                    "low_latency=off",
                    &CODECAPI_AVLowLatencyMode,
                    variant(
                        VT_BOOL,
                        VARIANT_0_0_0 {
                            boolVal: VARIANT_BOOL(0),
                        },
                    ),
                );
            }
            settings_accepted
        };
        let controls_first = replayx_core::native_gpu::Vendor::from_pci(desc.VendorId)?
            .controls_before_media_types();
        let mut settings_accepted = if controls_first {
            configure()
        } else {
            Vec::new()
        };

        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, s.bitrate)?;
        output.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
        if argument("--pacing", String::new())? == "capture-vfr" {
            // The opt-in source stream may have up to 250 pictures/s at 1080p.
            // Do not advertise the default 60 FPS encoder's lower AVC level.
            output.SetUINT32(&MF_MT_MPEG2_LEVEL, eAVEncH264VLevel5_2.0 as u32)?;
        }
        colour(&output)?;
        transform
            .SetOutputType(0, &output, 0)
            .with_context(|| format!("Configure {name} H.264 output"))?;
        let input = MFCreateMediaType()?;
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        colour(&input)?;
        transform
            .SetInputType(0, &input, 0)
            .with_context(|| format!("Configure {name} NV12 input"))?;

        if !controls_first {
            settings_accepted = configure();
        }
        if settings_accepted.len() != 6
            || settings_accepted.iter().any(|(label, accepted)| {
                !replayx_core::native_gpu::control_satisfied(label, *accepted, true)
            })
        {
            bail!("Encoder rejected required bitrate, GOP, B-frame or latency settings")
        }

        let info = transform.GetOutputStreamInfo(0)?;
        let provides_samples = info.dwFlags
            & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
                | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32)
            != 0;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .with_context(|| format!("Begin streaming on {name}"))?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .with_context(|| format!("Start stream on {name}"))?;
        Ok(Self {
            trace_log: trace::Log::from_args()?,
            events: transform.cast()?,
            transform,
            _shutdown: shutdown,
            provides_samples,
            output_buffer_size: info.cbSize.max(4 << 20),
            input_credits: 0,
            name,
            vendor: vendor_id,
            adapter_luid,
            settings_accepted,
            output: BufWriter::with_capacity(
                65536,
                output::Target::open(
                    out_path,
                    std::env::args()
                        .position(|a| a == "--stream-address")
                        .and_then(|i| std::env::args().nth(i + 1))
                        .as_deref(),
                )?,
            ),
            transport: std::env::args()
                .any(|a| a == "--stream-ts")
                .then(transport::Transport::default),
            bytes_written: 0,
            busy: HashMap::new(),
            stop_file: std::env::args()
                .position(|a| a == "--stop-file")
                .and_then(|i| std::env::args().nth(i + 1))
                .map(Into::into),
            guard_pid: None,
            last_guard: Instant::now(),
            cancelled: false,
            graceful_stop: std::env::args().any(|a| a == "--graceful-stop"),
            stop_requested: false,
            last_flush: Instant::now(),
            simulate_stall: std::env::args().any(|a| a == "--test-encoder-stall"),
            live_capture_input: std::env::args().any(|a| a == "--test-live-input-budget"),
            release_delay: Duration::from_millis(
                argument("--test-release-delay-ms", 0u64)?.min(200),
            ),
            surface_waits: 0,
            last_output_time: None,
            timestamps_increasing: true,
            timestamps_regular: true,
            expected_step: 10_000_000 / s.fps as i64,
            clock_fps: s.fps,
            expected_gop: s.gop as usize,
            rational_clock: false,
            capture_vfr: false,
            submitted_count: 0,
            expected_output: VecDeque::new(),
            next_key_time: 0,
            presentation_end: 0,
            keyframe_count: 0,
            frames_out: 0,
            keyframes: Vec::new(),
            output_times: Vec::new(),
            submitted: HashMap::new(),
            latency_ms: Vec::new(),
            cpu_parts: [0.0; 4],
        })
    }

    fn check(&mut self, deadline: Instant) -> Result<()> {
        if self.last_guard.elapsed() >= Duration::from_millis(250) {
            self.last_guard = Instant::now();
            let stopping = self.stop_file.as_ref().is_some_and(|p| p.exists());
            self.stop_requested |= self.graceful_stop && stopping;
            self.cancelled |= (!self.graceful_stop && stopping)
                || self
                    .guard_pid
                    .is_some_and(|pid| window_in_front(pid).is_none());
        }
        lifetime::check_wait(deadline, self.cancelled)
    }
    /// Polling is bounded by the caller's absolute deadline, including event storms.
    unsafe fn pump(&mut self) -> Result<bool> {
        if self.simulate_stall {
            return Ok(false);
        }
        let event = match self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
            Ok(e) => e,
            Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        event.GetStatus()?.ok()?;
        let kind = event.GetType()?;
        if kind == METransformNeedInput.0 as u32 {
            self.input_credits += 1;
        } else if kind == METransformHaveOutput.0 as u32 {
            self.collect()?;
        } else if kind == METransformDrainComplete.0 as u32 {
            return Ok(true);
        }
        Ok(true)
    }

    unsafe fn collect(&mut self) -> Result<()> {
        let sample = if self.provides_samples {
            None
        } else {
            let s = MFCreateSample()?;
            s.AddBuffer(&MFCreateMemoryBuffer(self.output_buffer_size)?)?;
            Some(s)
        };
        let mut buffer = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: ManuallyDrop::new(sample),
            dwStatus: 0,
            pEvents: ManuallyDrop::new(None),
        };
        let mut status = 0u32;
        let output_started = self.trace_log.as_ref().map(|_| trace::qpc());
        let result =
            self.transform
                .ProcessOutput(0, std::slice::from_mut(&mut buffer), &mut status);
        let output_finished = output_started.map(|_| trace::qpc());
        let sample = ManuallyDrop::take(&mut buffer.pSample);
        ManuallyDrop::drop(&mut buffer.pEvents);
        match result {
            Ok(()) => {}
            Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                // Take the encoder's updated output type and continue.
                let t = self.transform.GetOutputAvailableType(0, 0)?;
                self.transform.SetOutputType(0, &t, 0)?;
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
        let Some(sample) = sample else { return Ok(()) };
        let time = sample
            .GetSampleTime()
            .context("Hardware encoder omitted its timestamp")?;
        let key = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0;
        let (expected, required_key) = self
            .expected_output
            .pop_front()
            .context("Encoder emitted output without a queued input")?;
        h264::validate_submission(time, expected, key, required_key)?;
        if !self.rational_clock && !self.capture_vfr {
            h264::validate_timing(
                self.frames_out,
                time,
                self.expected_step,
                key,
                self.expected_gop,
            )?;
        }
        if sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0 {
            self.keyframe_count += 1;
            bounded_push(&mut self.keyframes, self.frames_out);
        }
        let data = sample.ConvertToContiguousBuffer()?;
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        data.Lock(&mut ptr, None, Some(&mut len))?;
        let bytes = std::slice::from_raw_parts(ptr, len as usize);
        let write_started = self.trace_log.as_ref().map(|_| trace::qpc());
        let written = (|| -> Result<()> {
            h264::validate_no_b(bytes)?;
            if let Some(transport) = &mut self.transport {
                transport.write(
                    &mut self.output,
                    time,
                    sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) != 0,
                    bytes,
                )?;
            } else {
                self.output.write_all(bytes)?;
            }
            Ok(())
        })();
        let write_finished = write_started.map(|_| trace::qpc());
        let unlocked = data.Unlock();
        written?;
        unlocked?;
        if let Some(log) = &self.trace_log {
            log.emit("encoder_output",json!({"output":self.frames_out,"pts":time,"bytes":len,"keyframe":sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0)!=0,"process_output_started_hns":output_started,"process_output_finished_hns":output_finished,"transport_write_started_hns":write_started,"transport_write_finished_hns":write_finished}))?;
        }
        self.bytes_written += len as u64;
        if self.last_flush.elapsed() >= Duration::from_millis(250) {
            let began = self.trace_log.as_ref().map(|_| trace::qpc());
            self.output.flush()?;
            if let Some(log) = &self.trace_log {
                log.emit(
                    "encoder_flush",
                    json!({"begin_hns":began,"end_hns":trace::qpc()}),
                )?;
            }
            self.last_flush = Instant::now();
        }
        if let Some(t) = self.submitted.remove(&time) {
            bounded_push(&mut self.latency_ms, t.elapsed().as_secs_f64() * 1000.);
        }
        self.timestamps_increasing &= self.last_output_time.is_none_or(|last| time > last);
        self.timestamps_regular &= !self.capture_vfr
            && if self.rational_clock {
                time == grid::offset(self.clock_fps, self.frames_out as i64)
            } else {
                self.last_output_time
                    .map_or(time == 0, |last| time - last == self.expected_step)
            };
        self.last_output_time = Some(time);
        bounded_push(&mut self.output_times, time);
        self.frames_out += 1;
        Ok(())
    }

    /// Submit one NV12 texture (same device) for encoding at `time` (100 ns).
    unsafe fn encode(&mut self, nv12: &ID3D11Texture2D, time: i64, duration: i64) -> Result<()> {
        let c = thread_cpu_ms();
        let trace_begin = self.trace_log.as_ref().map(|_| trace::qpc());
        let deadline = Instant::now() + lifetime::input_wait_budget(self.live_capture_input);
        while self.input_credits == 0 || self.submitted.len() >= 64 {
            self.check(deadline).with_context(|| if self.live_capture_input {
                "Waiting for live encoder input (500 ms budget); capture stopped to prevent frozen recording".to_owned()
            } else { "Waiting for encoder input".to_owned() })?;
            if !self.pump()? {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        self.check(deadline)?;
        self.input_credits -= 1;
        let c1 = thread_cpu_ms();
        let trace_credit = trace_begin.map(|_| trace::qpc());
        let key = nv12.as_raw() as usize;
        let busy = self.busy.entry(key).or_default().clone();
        let tracked = MFCreateTrackedSample()?;
        let sample: IMFSample = tracked.cast()?;
        sample.AddBuffer(&MFCreateDXGISurfaceBuffer(
            &ID3D11Texture2D::IID,
            nv12,
            0,
            false,
        )?)?;
        let mut lease = lifetime::Lease::new(busy, Some(nv12.clone()));
        lease.release_delay = self.release_delay;
        lease.trace_log = self.trace_log.clone();
        lease.pts = time;
        let callback: IMFAsyncCallback = lifetime::Released(lease).into();
        tracked.SetAllocator(&callback, None)?;
        if time < 0
            || duration <= 0
            || self
                .expected_output
                .back()
                .is_some_and(|(last, _)| time <= *last)
            || self.last_output_time.is_some_and(|last| time <= last)
        {
            bail!("Native input timestamp is invalid or out of order")
        }
        let required_key = if self.capture_vfr {
            time >= self.next_key_time
        } else {
            self.submitted_count.is_multiple_of(self.expected_gop)
        };
        if self.capture_vfr && required_key {
            // The documented control applies to the very next ProcessInput.
            // Fail explicitly if this optional mode cannot enforce time-based GOPs.
            self.transform
                .cast::<ICodecAPI>()?
                .SetValue(
                    &CODECAPI_AVEncVideoForceKeyFrame,
                    &variant(VT_UI4, VARIANT_0_0_0 { ulVal: 1 }),
                )
                .context("Timestamp-preserving mode requires source-clock keyframe control")?;
            self.next_key_time = (time / 20_000_000 + 1) * 20_000_000;
        }
        sample.SetSampleTime(time)?;
        sample.SetSampleDuration(duration)?;
        self.expected_output.push_back((time, required_key));
        self.presentation_end = time + duration;
        self.submitted.insert(time, Instant::now());
        if let Some(log) = &self.trace_log {
            log.emit("encoder_submit",json!({"output":self.submitted_count,"pts":time,"duration_hns":duration,"texture":key,"leases":self.busy.get(&key).unwrap().load(Ordering::Acquire)}))?;
        }
        let c2 = thread_cpu_ms();
        let trace_input_started = trace_begin.map(|_| trace::qpc());
        self.transform.ProcessInput(0, &sample, 0)?;
        let trace_input_done = trace_begin.map(|_| trace::qpc());
        self.submitted_count += 1;
        let c3 = thread_cpu_ms();
        // Collect whatever is ready without waiting.
        while self.pump()? {
            self.check(deadline)?;
        }
        let c4 = thread_cpu_ms();
        if let Some(log) = &self.trace_log {
            log.emit("encoder_stage_times",json!({"output":self.submitted_count-1,"begin_hns":trace_begin,"credit_hns":trace_credit,"process_input_started_hns":trace_input_started,"process_input_finished_hns":trace_input_done,"collect_finished_hns":trace::qpc()}))?;
        }
        for (slot, v) in self
            .cpu_parts
            .iter_mut()
            .zip([c1 - c, c2 - c1, c3 - c2, c4 - c3])
        {
            *slot += v;
        }
        Ok(())
    }

    /// A texture may be overwritten only after all tracked input samples release it.
    unsafe fn wait_surface(&mut self, surface: &ID3D11Texture2D) -> Result<()> {
        let busy = self.busy.get(&(surface.as_raw() as usize)).cloned();
        if busy
            .as_ref()
            .is_some_and(|b| b.load(Ordering::Acquire) != 0)
        {
            self.surface_waits += 1;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while busy
            .as_ref()
            .is_some_and(|b| b.load(Ordering::Acquire) != 0)
        {
            self.check(deadline)
                .context("Waiting for encoder to release an input surface")?;
            if !self.pump()? {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if let Some(log) = &self.trace_log {
            log.emit("surface_available",json!({"texture":surface.as_raw() as usize,"leases":busy.as_ref().map_or(0,|b|b.load(Ordering::Acquire))}))?;
        }
        self.check(deadline)
    }
    unsafe fn finish(&mut self) -> Result<()> {
        self.transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0)?;
        self.transform
            .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.check(deadline)?;
            match self.events.GetEvent(MF_EVENT_FLAG_NO_WAIT) {
                Ok(event) => {
                    event.GetStatus()?.ok()?;
                    let kind = event.GetType()?;
                    if kind == METransformHaveOutput.0 as u32 {
                        self.collect()?;
                    } else if kind == METransformDrainComplete.0 as u32 {
                        break;
                    }
                }
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(e) => return Err(e.into()),
            }
        }
        self.transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0)?;
        self.transform
            .ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)?;
        MFShutdownObject(&self.transform)
            .context("Shutdown drained encoder to release retained input samples")?;
        while self
            .busy
            .values()
            .any(|busy| busy.load(Ordering::Acquire) != 0)
        {
            self.check(deadline)
                .context("Encoder retained input surfaces after drain/flush")?;
            std::thread::sleep(Duration::from_millis(1));
        }
        self.output.flush()?;
        self.output.get_ref().sync()?;
        Ok(())
    }
}

/// The capture GPU's device for the converter and the encoder, shared with
/// Media Foundation's threads.
impl Drop for MfEncoder {
    fn drop(&mut self) {
        let _ = self.output.flush();
    }
}
struct TransformShutdown(IMFTransform);
impl Drop for TransformShutdown {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdownObject(&self.0);
        }
    }
}
struct MediaRuntime;
impl MediaRuntime {
    unsafe fn start() -> Result<Self> {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .context("Initialize COM for Media Foundation")?;
        if let Err(error) = MFStartup(MF_VERSION, 0) {
            CoUninitialize();
            return Err(error).context("Start Media Foundation");
        }
        Ok(Self)
    }
}
impl Drop for MediaRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

unsafe fn video_device(vendor_id: u32) -> Result<(ID3D11Device, ID3D11DeviceContext, String)> {
    let (adapter, name) = adapter_by_vendor(vendor_id).context("No GPU from that vendor")?;
    let (mut device, mut context) = (None, None);
    D3D11CreateDevice(
        &adapter,
        D3D_DRIVER_TYPE_UNKNOWN,
        HMODULE::default(),
        D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
        None,
        D3D11_SDK_VERSION,
        Some(&mut device),
        None,
        Some(&mut context),
    )?;
    let (device, context): (ID3D11Device, ID3D11DeviceContext) =
        (device.unwrap(), context.unwrap());
    let _ = device
        .cast::<ID3D11Multithread>()?
        .SetMultithreadProtected(true);
    Ok((device, context, name))
}

/// Today's 1080p60 recording, natively:
/// - frame selection exactly as the recorder (select.rs);
/// - one fixed-function conversion per *chosen* frame (captures the
///   selector drops are never converted);
/// - Quick Sync encodes the D3D11 NV12 texture directly;
/// - repeats re-encode the last converted frame.
struct HeldCapture {
    texture: ID3D11Texture2D,
    _frame: Option<CaptureFrame>,
    time: i64,
    index: usize,
    log: Option<trace::Log>,
}
impl Drop for HeldCapture {
    fn drop(&mut self) {
        self._frame.take();
        if let Some(log) = &self.log {
            let _ = log.emit("capture_frame_closed", json!({"capture":self.index}));
        }
    }
}
struct NativeRecorder {
    native: Native,
    encoder: MfEncoder,
    selector: CfrSelector,
    grid: Option<grid::Grid>,
    capture_vfr: bool,
    /// Where the phase lock's "new picture" hints come from.
    change_hints: &'static str,
    source_timeline: timeline::Timeline,
    pending_capture: Option<HeldCapture>,
    fps: u32,
    nominal_capture_rate: u32,
    probe: Option<trace::Probe>,
    ring: Vec<ID3D11Texture2D>,
    next_ring: usize,
    last_ring: Option<usize>,
    slot_duration: i64,
    captures: usize,
    converted: usize,
    encoded: usize,
    /// For checking: the capture index shown in each output slot.
    shown: Vec<usize>,
    trace: bool,
    origin_qpc_hns: Option<i64>,
    origin_path: Option<std::path::PathBuf>,
    last_shown: usize,
    convert_submit_ms: Vec<f64>,
    encode_submit_ms: Vec<f64>,
    /// Recording-thread CPU spent converting and submitting to the encoder.
    convert_cpu_ms: f64,
    encode_cpu_ms: f64,
}

impl NativeRecorder {
    unsafe fn new(
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        input: (u32, u32),
        settings: &EncoderSettings,
        capture_rate: u32,
        vendor: &str,
        out_path: &str,
    ) -> Result<Self> {
        let native = Native::new(device, context, input, settings.size, false)?;
        let mut encoder = MfEncoder::new(device, vendor, settings, out_path)?;
        let pacing: String = argument("--pacing", "legacy".to_owned())?;
        encoder.rational_clock = pacing == "clocked" || pacing == "phase-lock";
        encoder.capture_vfr = pacing == "capture-vfr";
        let probe = match (trace::Roi::from_args()?, encoder.trace_log.clone()) {
            (Some(roi), Some(log)) => {
                Some(trace::Probe::new(device, roi, input, settings.size, log)?)
            }
            (Some(_), None) => bail!("--trace-roi requires --trace-file"),
            _ => None,
        };
        // Experiment (`--encoder-bind 1`): ring textures also bound for the
        // video encoder, which may let it read them without its own copy.
        let encoder_bind = if argument("--encoder-bind", 0u8)? != 0 {
            D3D11_BIND_VIDEO_ENCODER.0
        } else {
            0
        };
        let ring = (0..8)
            .map(|_| {
                texture(
                    device,
                    settings.size,
                    DXGI_FORMAT_NV12,
                    D3D11_USAGE_DEFAULT,
                    D3D11_BIND_FLAG(
                        D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0 | encoder_bind,
                    ),
                    D3D11_CPU_ACCESS_FLAG(0),
                )
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            native,
            encoder,
            selector: CfrSelector::new(settings.fps, capture_rate),
            grid: match pacing.as_str() {
                "clocked" => Some(grid::Grid::new(settings.fps)),
                // Hint availability is set by the caller before the first capture.
                "phase-lock" => Some(grid::Grid::with_phase_lock(settings.fps, false)),
                _ => None,
            },
            capture_vfr: pacing == "capture-vfr",
            change_hints: "every-capture",
            source_timeline: timeline::Timeline::default(),
            pending_capture: None,
            fps: settings.fps,
            nominal_capture_rate: capture_rate,
            probe,
            ring,
            next_ring: 0,
            last_ring: None,
            slot_duration: 10_000_000 / settings.fps as i64,
            captures: 0,
            converted: 0,
            encoded: 0,
            shown: Vec::new(),
            trace: false,
            origin_qpc_hns: None,
            origin_path: std::env::args()
                .position(|a| a == "--origin-file")
                .and_then(|i| std::env::args().nth(i + 1))
                .map(Into::into),
            last_shown: 0,
            convert_submit_ms: Vec::new(),
            encode_submit_ms: Vec::new(),
            convert_cpu_ms: 0.0,
            encode_cpu_ms: 0.0,
        })
    }

    /// One captured frame (a texture on the converter's device), captured
    /// at `time` (100 ns, Windows' capture clock).
    unsafe fn deliver(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        source: &ID3D11Texture2D,
        time: i64,
        frame_lease: Option<CaptureFrame>,
        picture_hints: (bool, bool),
    ) -> Result<()> {
        let (changed, phase_changed) = picture_hints;
        if self.origin_qpc_hns.is_none() {
            self.origin_qpc_hns = Some(time);
            if let Some(path) = &self.origin_path {
                let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
                serde_json::to_writer(&mut file, &json!({"origin_qpc_hns":time,"fps":60}))?;
                file.flush()?;
            }
        }
        let index = self.captures;
        self.captures += 1;
        if let Some(log) = &self.encoder.trace_log {
            log.emit(
                "capture_dequeued",
                json!({"capture":index,"capture_hns":time,"changed":changed,"phase_changed":phase_changed,"texture":source.as_raw() as usize}),
            )?;
        }
        if let Some(probe) = &mut self.probe {
            probe.sample(context, source, "captured_content", index, time)?;
        }
        if self.capture_vfr {
            let current = HeldCapture {
                texture: source.clone(),
                _frame: frame_lease,
                time,
                index,
                log: self.encoder.trace_log.clone(),
            };
            let previous = self.pending_capture.take();
            if let Some(span) = self.source_timeline.push(time, index)? {
                let previous = previous
                    .as_ref()
                    .context("Source timeline lost its pending frame")?;
                if previous.index != span.capture {
                    bail!("Source timeline capture ownership mismatch")
                }
                self.submit_picture(
                    device,
                    context,
                    self.encoded as i64,
                    previous,
                    index,
                    (span.pts, span.duration),
                )?;
            }
            self.pending_capture = Some(current);
            if let Some(probe) = &mut self.probe {
                context.Flush();
                probe.poll(context)?;
            }
            return Ok(());
        }
        if self.grid.is_some() {
            let current = HeldCapture {
                texture: source.clone(),
                _frame: frame_lease,
                time,
                index,
                log: self.encoder.trace_log.clone(),
            };
            let previous = self.pending_capture.take();
            let decisions =
                self.grid
                    .as_mut()
                    .unwrap()
                    .push_observed(time, index, changed, phase_changed)?;
            for (slot, pick) in decisions {
                let chosen = if pick == index {
                    &current
                } else {
                    previous.as_ref().context("Grid lost its pending capture")?
                };
                self.submit_grid(device, context, slot, chosen, index)?;
            }
            self.pending_capture = Some(current);
            if let Some(probe) = &mut self.probe {
                context.Flush();
                probe.poll(context)?;
            }
            return Ok(());
        }
        let _lease = frame_lease;
        let mut converted: Option<usize> = None;
        for (slot, source_kind) in self.selector.push(time) {
            let ring = match source_kind {
                // A repeat: the last frame actually recorded.
                Source::Previous => self.last_ring.context("repeat before the first frame")?,
                Source::Current => match converted {
                    Some(r) => r,
                    None => {
                        let r = self.next_ring;
                        self.next_ring = (r + 1) % self.ring.len();
                        self.encoder.wait_surface(&self.ring[r])?;
                        let (t, c) = (Instant::now(), thread_cpu_ms());
                        self.native
                            .convert_to(device, context, source, &self.ring[r])?;
                        if let Some(probe) = &mut self.probe {
                            probe.sample(
                                context,
                                &self.ring[r],
                                "converted_content",
                                index,
                                time,
                            )?;
                        }
                        context.Flush();
                        bounded_push(
                            &mut self.convert_submit_ms,
                            t.elapsed().as_secs_f64() * 1000.,
                        );
                        self.convert_cpu_ms += thread_cpu_ms() - c;
                        self.converted += 1;
                        converted = Some(r);
                        r
                    }
                },
            };
            if let Some(log) = &self.encoder.trace_log {
                log.emit("selected",json!({"output":slot,"pts":slot*self.slot_duration,"capture":if source_kind==Source::Current{index}else{self.last_shown},"trigger_capture":index,"kind":format!("{source_kind:?}"),"ring":ring,"texture":self.ring[ring].as_raw() as usize}))?;
            }
            let (t, c) = (Instant::now(), thread_cpu_ms());
            self.encoder.encode(
                &self.ring[ring],
                slot * self.slot_duration,
                self.slot_duration,
            )?;
            bounded_push(
                &mut self.encode_submit_ms,
                t.elapsed().as_secs_f64() * 1000.,
            );
            self.encode_cpu_ms += thread_cpu_ms() - c;
            self.encoded += 1;
            if source_kind == Source::Current {
                self.last_ring = Some(ring);
                self.last_shown = index;
            }
            if self.trace {
                bounded_push(&mut self.shown, self.last_shown);
            }
        }
        if let Some(probe) = &mut self.probe {
            context.Flush();
            probe.poll(context)?;
        }
        Ok(())
    }

    unsafe fn submit_grid(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        slot: i64,
        chosen: &HeldCapture,
        trigger: usize,
    ) -> Result<()> {
        let pts = grid::offset(self.fps, slot);
        let duration = grid::offset(self.fps, slot + 1) - pts;
        self.submit_picture(device, context, slot, chosen, trigger, (pts, duration))
    }
    unsafe fn submit_picture(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        slot: i64,
        chosen: &HeldCapture,
        trigger: usize,
        stamp: (i64, i64),
    ) -> Result<()> {
        let (pts, duration) = stamp;
        let ring = if let Some(ring) = self.last_ring.filter(|_| self.last_shown == chosen.index) {
            ring
        } else {
            let r = self.next_ring;
            self.next_ring = (r + 1) % self.ring.len();
            self.encoder.wait_surface(&self.ring[r])?;
            let (t, c) = (Instant::now(), thread_cpu_ms());
            self.native
                .convert_to(device, context, &chosen.texture, &self.ring[r])?;
            if let Some(probe) = &mut self.probe {
                probe.sample(
                    context,
                    &self.ring[r],
                    "converted_content",
                    chosen.index,
                    chosen.time,
                )?;
            }
            context.Flush();
            bounded_push(
                &mut self.convert_submit_ms,
                t.elapsed().as_secs_f64() * 1000.,
            );
            self.convert_cpu_ms += thread_cpu_ms() - c;
            self.converted += 1;
            r
        };
        if let Some(log) = &self.encoder.trace_log {
            log.emit("selected",json!({"output":slot,"pts":pts,"duration_hns":duration,"grid_hns":if self.capture_vfr {chosen.time}else{self.grid.as_ref().unwrap().time(slot)},"deadline_hns":self.grid.as_ref().and_then(|g|g.sealed_deadline(slot)),"capture":chosen.index,"trigger_capture":trigger,"kind":if self.capture_vfr{"CaptureVfr"}else{"Clocked"},"ring":ring,"texture":self.ring[ring].as_raw() as usize}))?;
        }
        let (t, c) = (Instant::now(), thread_cpu_ms());
        self.encoder.encode(&self.ring[ring], pts, duration)?;
        bounded_push(
            &mut self.encode_submit_ms,
            t.elapsed().as_secs_f64() * 1000.,
        );
        self.encode_cpu_ms += thread_cpu_ms() - c;
        self.encoded += 1;
        self.last_ring = Some(ring);
        self.last_shown = chosen.index;
        if self.trace {
            bounded_push(&mut self.shown, chosen.index);
        }
        Ok(())
    }
    unsafe fn advance_grid(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        end: i64,
        idle: bool,
    ) -> Result<()> {
        if self.grid.is_none() {
            return Ok(());
        }
        if let Some(pending) = self.pending_capture.take() {
            if let Some(log) = &self.encoder.trace_log {
                log.emit(
                    "grid_watermark",
                    json!({"end_hns":end,"before_capture":self.captures,"idle":idle}),
                )?;
            }
            let decisions = if idle {
                self.grid.as_mut().unwrap().advance_idle(end)?
            } else {
                self.grid.as_mut().unwrap().advance(end)?
            };
            for (slot, _) in decisions {
                self.submit_grid(device, context, slot, &pending, pending.index)?;
            }
            self.pending_capture = Some(pending);
        }
        Ok(())
    }
    unsafe fn seal_end(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        end: i64,
    ) -> Result<()> {
        if self.capture_vfr {
            if let Some(span) = self.source_timeline.finish(end)? {
                let pending = self
                    .pending_capture
                    .take()
                    .context("Source timeline lost its final frame")?;
                self.submit_picture(
                    device,
                    context,
                    self.encoded as i64,
                    &pending,
                    pending.index,
                    (span.pts, span.duration),
                )?;
            }
        } else {
            self.advance_grid(device, context, end, false)?;
        }
        Ok(())
    }
    fn next_wake(&self) -> Duration {
        self.grid.as_ref().and_then(|g| g.next_idle_time()).map_or(
            Duration::from_millis(100),
            |t| {
                Duration::from_nanos(
                    ((t + 10_000_000 / self.fps as i64 - trace::qpc()).max(0) as u64) * 100,
                )
                .min(Duration::from_millis(100))
            },
        )
    }
    /// End of recording: FFmpeg's final repeats, then drain the encoder.
    unsafe fn finish(&mut self) -> Result<()> {
        for (slot, _) in if self.grid.is_some() || self.capture_vfr {
            Vec::new()
        } else {
            self.selector.finish()
        } {
            if let Some(ring) = self.last_ring {
                self.encoder.encode(
                    &self.ring[ring],
                    slot * self.slot_duration,
                    self.slot_duration,
                )?;
                self.encoded += 1;
                if self.trace {
                    bounded_push(&mut self.shown, self.last_shown);
                }
            }
        }
        self.encoder.finish()?;
        if let Some(log) = &self.encoder.trace_log {
            log.flush()?;
        }
        if self.encoder.frames_out != self.encoded {
            bail!(
                "Encoder lost output frames: {} submitted, {} written",
                self.encoded,
                self.encoder.frames_out
            )
        }
        Ok(())
    }

    fn report(&self) -> serde_json::Value {
        let mut report = json!({
            "worker_pid":std::process::id(),
            "origin_qpc_hns": self.origin_qpc_hns,
            "frame_pacing":if self.capture_vfr{"capture-vfr-experimental"}else if self.grid.as_ref().is_some_and(|g|g.lock.is_some()){"clocked-phase-lock-experimental"}else if self.grid.is_some(){"clocked-causal"}else{"legacy-ffmpeg"},
            "phase_lock":self.grid.as_ref().and_then(|g|g.lock.as_ref()).map(|l|json!({"moves":l.moves,"wraps":l.wraps,"held_without_structure":l.held_without_structure,"late_recoveries":self.grid.as_ref().unwrap().late_recoveries,"idle_deferrals":self.grid.as_ref().unwrap().idle_deferrals,"deadline_offset_ms":l.delta/10_000.0,"change_hints":self.change_hints})),
            "equal_capture_timestamps_coalesced":self.source_timeline.coalesced,
            "presentation_end_hns":self.origin_qpc_hns.map(|o|o+self.encoder.presentation_end),
            "output_timestamp_contract_valid":self.encoder.expected_output.is_empty() && self.encoder.frames_out==self.encoded,
            "grid_late_captures":self.grid.as_ref().map_or(0,|g|g.late),
            "capture_not_converted":self.captures.saturating_sub(self.converted),
            "shown_captures":if self.trace && std::env::args().any(|a|a=="--trace-schedule") {json!(self.shown)}else{serde_json::Value::Null},
            "probe_dropped":self.probe.as_ref().map_or(0,|p|p.dropped),
            "probe_completed":self.probe.as_ref().map_or(0,|p|p.completed),
            "surface_reuse_waits": self.encoder.surface_waits,
            "live_input_samples": self.encoder.busy.values().map(|n| n.load(Ordering::Acquire)).sum::<usize>(),
            "bytes_written": self.encoder.bytes_written,
            "diagnostic_sample_limit": DIAGNOSTIC_SAMPLES,
            "captures":self.captures, "converted":self.converted, "encoded":self.encoded,
            "frames_out":self.encoder.frames_out, "dup_frames":if self.grid.is_some(){self.encoded.saturating_sub(self.converted) as u64}else{self.selector.dups}, "drop_frames":if self.grid.is_some(){self.captures.saturating_sub(self.converted) as u64}else{self.selector.drops},
            "repeats_of_dropped_frames":self.selector.repeats_of_dropped,
            "keyframe_count":self.encoder.keyframe_count,
            "output_timestamps_increasing":self.encoder.timestamps_increasing,
            "output_timestamps_regular":self.encoder.timestamps_regular,
            "convert_submit":stats(self.convert_submit_ms.clone()),
            "encode_submit":stats(self.encode_submit_ms.clone()),
            "encode_latency":stats(self.encoder.latency_ms.clone()),
            "input_views_created":self.native.input_views_created,
            "loop_cpu_ms_per_output_frame_convert":self.convert_cpu_ms / self.encoded.max(1) as f64,
            "loop_cpu_ms_per_output_frame_encode_submit":self.encode_cpu_ms / self.encoded.max(1) as f64,
            "encode_submit_cpu_ms_per_frame":{
                "wait_for_input_request":self.encoder.cpu_parts[0] / self.encoded.max(1) as f64,
                "wrap_texture_in_sample":self.encoder.cpu_parts[1] / self.encoded.max(1) as f64,
                "process_input":self.encoder.cpu_parts[2] / self.encoded.max(1) as f64,
                "collect_output":self.encoder.cpu_parts[3] / self.encoded.max(1) as f64,
            },
            "encoder":self.encoder.name, "encoder_vendor":self.encoder.vendor,"capture_encoder_adapter_luid":format!("{:016X}",self.encoder.adapter_luid),"transfer":"same-device GPU textures; no CPU readback", "no_b_frames":"validated in every output access unit", "settings_accepted":self.encoder.settings_accepted.iter().map(|(k, v)| json!({k: v})).collect::<Vec<_>>(),
        });
        report["live_input_wait_budget_ms"] =
            json!(lifetime::input_wait_budget(self.encoder.live_capture_input).as_millis());
        report
    }
}

/// Capture times (100 ns) for the synthetic delivery patterns, mirroring
/// scripts/bench/native-select-check.py.
fn synthetic_times(scenario: &str, seconds: u32) -> Result<Vec<i64>> {
    let t = |s: f64| (s * 10_000_000.0) as i64;
    let mut lcg = 12345u64;
    let mut jitter = || {
        lcg = lcg
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((lcg >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 0.006
    };
    Ok(match scenario {
        "exact60" | "static60" => (0..seconds * 60).map(|n| t(n as f64 / 60.)).collect(),
        "slow30" => (0..seconds * 30).map(|n| t(n as f64 / 30.)).collect(),
        "rate120" | "rate144" | "rate240" | "rate250" => {
            let rate: u32 = scenario.trim_start_matches("rate").parse()?;
            (0..seconds * rate)
                .map(|n| t(n as f64 / rate as f64))
                .collect()
        }
        "regular80" => (0..seconds * 80).map(|n| t(n as f64 * 0.0125)).collect(),
        "jitter" => (0..seconds * 80)
            .map(|n| t(0.01 + n as f64 * 0.0125 + jitter()))
            .collect(),
        "gap" => {
            let first = seconds * 40;
            (0..seconds * 80)
                .filter(|n| !(first..first + 80).contains(n))
                .map(|n| t(n as f64 * 0.0125))
                .collect()
        }
        "starve50" => (0..seconds * 50).map(|n| t(n as f64 * 0.02)).collect(),
        "refresh240" => (0..seconds * 80)
            .map(|n| t(((n / 2) * 7 + (n % 2) * 3) as f64 / 240.0))
            .collect(),
        other => bail!("unknown scenario {other}"),
    })
}

/// A capture counts as a new picture for the phase lock when its dirty
/// regions cover at least 5% of the captured area: a game or a video redraws
/// most of it; the cursor, a caret or a clock do not. If Windows cannot report
/// the regions for this frame, the capture counts (timestamps-only behaviour).
struct DirtyReport {
    area: i64,
    count: u32,
    rects: [[i32; 4]; 128],
    len: usize,
}
fn reported_dirty_report(frame: &CaptureFrame) -> Option<DirtyReport> {
    let regions = frame.DirtyRegions().ok()?;
    let count = regions.Size().ok()?;
    let mut report = DirtyReport {
        area: 0,
        count,
        rects: [[0; 4]; 128],
        len: 0,
    };
    if count > 128 {
        return Some(report);
    }
    for i in 0..count {
        let r = regions.GetAt(i).ok()?;
        report.area += r.Width.max(0) as i64 * r.Height.max(0) as i64;
        report.rects[i as usize] = [r.X, r.Y, r.Width, r.Height];
        report.len += 1;
    }
    Some(report)
}

/// `--pacing-replay TIMELINE.json [--phase-hints content|none] [--pacing
/// clocked|phase-lock]`: the 60 FPS grid's choices for a recorded timeline
/// (`captures: [{time_hns, content_id}]`), as JSON. Pure scheduling: no
/// device is opened and nothing is captured.
fn pacing_replay(path: &str) -> Result<()> {
    let timeline: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let captures = timeline["captures"]
        .as_array()
        .context("captures missing")?;
    let hint_mode = argument("--phase-hints", "content".to_owned())?;
    if !["content", "reported", "none"].contains(&hint_mode.as_str()) {
        bail!("Invalid phase hint mode")
    }
    let content_hints = hint_mode != "none";
    let lock = argument("--pacing", "phase-lock".to_owned())? == "phase-lock";
    let mut g = if lock {
        grid::Grid::with_phase_lock(60, content_hints)
    } else {
        grid::Grid::new(60)
    };
    if let Some(calibration) = timeline["calibration"].as_array() {
        if let Some(phase) = &mut g.lock {
            for c in calibration {
                if c["changed"].as_bool().unwrap_or(true) {
                    phase.arrival(c["time_hns"].as_i64().context("calibration time")?);
                    phase.update();
                }
            }
            if let Some(first) = captures.first() {
                phase.rebase_before_start(first["time_hns"].as_i64().context("first time")?);
            }
        }
    }
    if let Some(state) = timeline["phase_start"].as_object() {
        if let Some(phase) = &mut g.lock {
            let times: Vec<_> = state["observations"]
                .as_array()
                .context("phase observations")?
                .iter()
                .map(|t| t.as_i64().context("observation timestamp"))
                .collect::<Result<_>>()?;
            if !phase.restore(
                captures[0]["time_hns"].as_i64().context("origin")?,
                state["delta_hns"].as_f64().context("delta")?,
                &times,
            ) {
                bail!("Invalid saved phase")
            }
        }
    }
    let advances = timeline["advances"].as_array();
    let mut advance_index = 0;
    let (mut picks, mut deadlines) = (Vec::new(), Vec::new());
    let mut previous: Option<u64> = None;
    let mut last = 0;
    for (i, c) in captures.iter().enumerate() {
        if let Some(advances) = advances {
            while advance_index < advances.len()
                && advances[advance_index]["before_capture"].as_u64() == Some(i as u64)
            {
                let end = advances[advance_index]["end_hns"]
                    .as_i64()
                    .context("advance end")?;
                let decisions = if advances[advance_index]["idle"].as_bool().unwrap_or(false) {
                    g.advance_idle(end)?
                } else {
                    g.advance(end)?
                };
                for (slot, pick) in decisions {
                    picks.push(pick);
                    deadlines.push(g.sealed_deadline(slot));
                }
                advance_index += 1;
            }
        }
        let t = c["time_hns"].as_i64().context("time_hns")?;
        let id = c["content_id"].as_u64();
        let changed = if hint_mode == "reported" {
            c["changed"].as_bool().context("reported hint missing")?
        } else {
            !content_hints || previous.is_none() || id != previous
        };
        previous = id;
        let phase_changed = if hint_mode == "reported" {
            c["phase_changed"].as_bool().unwrap_or(changed)
        } else {
            changed
        };
        if let Some(reseeds) = timeline["reseeds"].as_array() {
            for r in reseeds
                .iter()
                .filter(|r| r["before_capture"].as_u64() == Some(i as u64))
            {
                if let Some(phase) = &mut g.lock {
                    let times: Vec<_> = r["observations"]
                        .as_array()
                        .context("reseed observations")?
                        .iter()
                        .map(|t| t.as_i64().context("observation timestamp"))
                        .collect::<Result<_>>()?;
                    if !phase.restore(
                        captures[0]["time_hns"].as_i64().context("origin")?,
                        r["delta_hns"].as_f64().context("delta")?,
                        &times,
                    ) {
                        bail!("Invalid saved phase")
                    }
                }
            }
        }
        for (slot, pick) in g.push_observed(t, i, changed, phase_changed)? {
            picks.push(pick);
            deadlines.push(g.sealed_deadline(slot));
        }
        last = t;
    }
    if let Some(advances) = advances {
        for a in &advances[advance_index..] {
            if a["before_capture"].as_u64() != Some(captures.len() as u64) {
                bail!("Unordered watermark")
            }
            let end = a["end_hns"].as_i64().context("advance end")?;
            let decisions = if a["idle"].as_bool().unwrap_or(false) {
                g.advance_idle(end)?
            } else {
                g.advance(end)?
            };
            for (slot, pick) in decisions {
                picks.push(pick);
                deadlines.push(g.sealed_deadline(slot));
            }
        }
    } else {
        for (slot, pick) in g.advance(last + 1)? {
            picks.push(pick);
            deadlines.push(g.sealed_deadline(slot));
        }
    }
    let l = g.lock.as_ref();
    println!(
        "{}",
        json!({"picks":picks,"deadlines_hns":deadlines,"origin_hns":g.origin,
               "moves":l.map(|l|l.moves),"wraps":l.map(|l|l.wraps)})
    );
    Ok(())
}

/// `--synthetic SCENARIO OUT.h264`: the full native path on synthetic
/// captures. Each capture is a flat grey whose level encodes its index, so
/// the decoded output proves which capture every slot shows. Nothing is
/// captured from the screen.
fn synthetic(scenario: &str, out_path: &str) -> Result<()> {
    let seconds = argument("--seconds", 8u32)?;
    if !(1..=60).contains(&seconds) {
        bail!("Generated diagnostics are limited to 1..60 seconds")
    }
    let paced = argument("--paced", 0u8)? != 0;
    let replay_path = argument("--replay-timeline", String::new())?;
    let replay = if replay_path.is_empty() {
        None
    } else {
        Some(serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(&replay_path)?,
        )?)
    };
    if replay.is_some()
        && std::env::args().any(|a| a == "--app-session" || a == "--clocked-synthetic")
    {
        bail!("Recorded-timeline replay is an offline diagnostic only")
    }
    let times: Vec<i64> = if let Some(r) = &replay {
        r["captures"]
            .as_array()
            .context("Replay captures missing")?
            .iter()
            .map(|c| c["time_hns"].as_i64().context("Invalid replay clock"))
            .collect::<Result<_>>()?
    } else {
        synthetic_times(scenario, seconds)?
    };
    if times.is_empty()
        || (replay.is_some() && times.len() > 4096)
        || times.windows(2).any(|w| w[1] < w[0])
        || times.last().unwrap() - times[0] > 600_000_000
    {
        bail!("Offline timeline must be ordered, at most 4096 captures and 60 seconds")
    }
    let source_ids: Vec<u32> = if let Some(r) = &replay {
        r["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                c["content_id"]
                    .as_u64()
                    .filter(|n| *n < 1 << 24)
                    .map(|n| n as u32)
                    .context("Invalid replay frame ID")
            })
            .collect::<Result<_>>()?
    } else {
        (0..times.len() as u32).collect()
    };
    unsafe {
        let _runtime = MediaRuntime::start()?;
        let vendor_id = u32::from_str_radix(&argument("--vendor", "8086".to_string())?, 16)?;
        let (device, context, adapter) = video_device(vendor_id)?;
        let settings = EncoderSettings {
            size: (1920, 1080),
            fps: 60,
            bitrate: 15_000_000,
            max_bitrate: 23_000_000,
            gop: 120,
        };
        let input = (2560, 1600);
        let capture_rate = match scenario {
            "exact60" | "static60" => 60,
            "slow30" => 30,
            "rate120" => 120,
            "rate144" => 144,
            "rate240" => 240,
            "rate250" => 250,
            _ => 80,
        };
        let mut recorder = NativeRecorder::new(
            &device,
            &context,
            input,
            &settings,
            capture_rate,
            &format!("VEN_{vendor_id:04X}"),
            out_path,
        )?;
        recorder.trace = true;
        // Synthetic pictures carry their identity: hint exactly which captures
        // are new (a perfect dirty-region report). `--phase-hints none` models
        // a platform without change information.
        let content_hints = argument("--phase-hints", "content".to_owned())? == "content";
        if content_hints {
            recorder.change_hints = "synthetic-content-ids";
        }
        if recorder.grid.as_ref().is_some_and(|g| g.lock.is_some()) {
            recorder.grid = Some(grid::Grid::with_phase_lock(settings.fps, content_hints));
        }
        let mut sources = Vec::new();
        for _ in 0..2 {
            let source = texture(
                &device,
                input,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                D3D11_USAGE_DEFAULT,
                D3D11_BIND_FLAG(D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0),
                D3D11_CPU_ACCESS_FLAG(0),
            )?;
            let mut rtv = None;
            device.CreateRenderTargetView(&source, None, Some(&mut rtv))?;
            let rtv = rtv.unwrap();
            sources.push((source, rtv));
        }
        let level = |i: usize| {
            if scenario == "static60" {
                0.5
            } else {
                ((source_ids[i] * 7) % 220) as f32 / 219.0
            }
        };
        let cpu_before = cpu_ms();
        let thread_before = thread_cpu_ms();
        let began = Instant::now();
        let t0 = times[0];
        let capture_offset = if std::env::args().any(|a| a == "--clocked-synthetic") {
            use windows::Win32::System::Performance::{
                QueryPerformanceCounter, QueryPerformanceFrequency,
            };
            let (mut counter, mut frequency) = (0i64, 0i64);
            QueryPerformanceCounter(&mut counter)?;
            QueryPerformanceFrequency(&mut frequency)?;
            (counter as i128 * 10_000_000 / frequency as i128) as i64
        } else {
            0
        };
        for (i, &time) in times.iter().enumerate() {
            recorder
                .encoder
                .check(Instant::now() + Duration::from_secs(5))?;
            if recorder.encoder.stop_requested {
                break;
            }
            if paced {
                let arrival_delta = replay
                    .as_ref()
                    .and_then(|r| {
                        let a = r["captures"][i]["arrival_hns"].as_i64()?;
                        let first = r["captures"][0]["arrival_hns"].as_i64()?;
                        Some((a - first).max(0))
                    })
                    .unwrap_or(time - t0);
                let due = began + Duration::from_nanos((arrival_delta * 100) as u64);
                if let Some(d) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(d);
                }
            }
            let g = level(i);
            let (source, rtv) = &sources[i % 2];
            context.ClearRenderTargetView(rtv, &[g, g, g, 1.0]);
            if replay.is_some() {
                let c1: ID3D11DeviceContext1 = context.cast()?;
                for bit in 0..24 {
                    let value = if source_ids[i] & (1 << bit) != 0 {
                        1.
                    } else {
                        0.
                    };
                    c1.ClearView(
                        rtv,
                        &[value, value, value, 1.],
                        Some(&[windows::Win32::Foundation::RECT {
                            left: 48 + bit * 48,
                            top: 32,
                            right: 80 + bit * 48,
                            bottom: 64,
                        }]),
                    );
                }
            }
            let changed = !content_hints || i == 0 || source_ids[i] != source_ids[i - 1];
            recorder.deliver(
                &device,
                &context,
                source,
                time + capture_offset,
                None,
                (changed, changed),
            )?;
        }
        let delivered = &times[..recorder.captures];
        let source_end = replay
            .as_ref()
            .and_then(|r| r["end_hns"].as_i64())
            .unwrap_or_else(|| {
                delivered.last().copied().unwrap_or(t0)
                    + capture_offset
                    + 10_000_000 / recorder.nominal_capture_rate as i64
            });
        if recorder.grid.is_some() || recorder.capture_vfr {
            recorder.seal_end(&device, &context, source_end)?;
        }
        recorder.finish()?;
        let elapsed = began.elapsed().as_secs_f64();
        let cpu = cpu_ms() - cpu_before;

        let expected_luma: Vec<u32> = recorder
            .shown
            .iter()
            .map(|&i| {
                if scenario == "static60" {
                    126
                } else {
                    16 + ((source_ids[i] * 7) % 220)
                }
            })
            .collect();
        let mut report = recorder.report();
        if std::env::args().any(|a| a == "--trace-schedule") {
            report["source_content_ids"] = json!(source_ids
                .iter()
                .take(recorder.captures)
                .take(DIAGNOSTIC_SAMPLES)
                .collect::<Vec<_>>());
            report["source_times_hns"] = json!(delivered
                .iter()
                .take(DIAGNOSTIC_SAMPLES)
                .map(|t| t + capture_offset)
                .collect::<Vec<_>>());
            report["source_schedule_complete"] = json!(delivered.len() <= DIAGNOSTIC_SAMPLES);
        }
        report["source_end_hns"] = json!(source_end);
        report["scenario"] = json!(scenario);
        report["adapter"] = json!(adapter);
        report["paced"] = json!(paced);
        report["seconds"] = json!(elapsed);
        report["cpu_ms_per_output_frame"] = json!(cpu / recorder.encoded.max(1) as f64);
        report["loop_cpu_ms_per_output_frame"] =
            json!((thread_cpu_ms() - thread_before) / recorder.encoded.max(1) as f64);
        report["expected_luma"] = json!(expected_luma);
        report["output"] = json!(out_path);
        println!("{}", serde_json::to_string(&report)?);
    }
    Ok(())
}

/// Whether `pid` is a process the diagnostics may capture: CS2, Fortnite
/// or REPLAYX's own test workload. Takes a process snapshot (about 15 ms of
/// CPU), so it is checked once: a process's executable does not change.
fn allowed_game(pid: u32) -> bool {
    let mut sys = sysinfo::System::new();
    let id = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[id]), true);
    sys.process(id).is_some_and(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        [
            "cs2.exe",
            "fortniteclient-win64-shipping.exe",
            "replayx-gameproxy.exe",
        ]
        .contains(&name.as_str())
    })
}

/// `pid`'s window if it is the foreground window and not minimized. Two
/// window-manager calls, no messages: cheap enough to poll.
fn window_in_front(pid: u32) -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    };
    unsafe {
        let front = GetForegroundWindow();
        let mut owner = 0;
        GetWindowThreadProcessId(front, Some(&mut owner));
        (owner == pid && !IsIconic(front).as_bool()).then_some(front)
    }
}

/// The adapter that drives `monitor` (where WGC delivers its frames), its
/// PCI vendor id and name.
unsafe fn adapter_for_monitor(
    monitor: windows::Win32::Graphics::Gdi::HMONITOR,
) -> Result<(windows::Win32::Graphics::Dxgi::IDXGIAdapter, u32, String)> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
    let mut i = 0;
    while let Ok(adapter) = factory.EnumAdapters1(i) {
        i += 1;
        let mut o = 0;
        while let Ok(output) = adapter.EnumOutputs(o) {
            o += 1;
            if output.GetDesc()?.Monitor == monitor {
                let colour = output
                    .cast::<windows::Win32::Graphics::Dxgi::IDXGIOutput6>()
                    .context("Native capture cannot verify this display's SDR color space")?
                    .GetDesc1()?;
                if colour.ColorSpace != windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709 {
                    bail!("Native Alpha supports SDR BT.709 displays only; HDR or other color spaces require an explicit conversion path")
                }
                let desc = adapter.GetDesc1()?;
                let name = String::from_utf16_lossy(&desc.Description)
                    .trim_end_matches('\0')
                    .to_string();
                return Ok((adapter.cast()?, desc.VendorId, name));
            }
        }
    }
    bail!("No adapter drives the game's display")
}

/// `--record --pid PID --seconds N --out FILE.h264`: the native prototype
/// recording a real game, for comparison with today's recorder.
/// - WGC capture of the game's display (or `--mode window`) at the
///   recorder's 12.5 ms interval;
/// - every delivered frame goes through the selector, in arrival order;
/// - chosen frames are converted once (fixed function) and encoded by the
///   display GPU's H.264 encoder from the D3D11 texture.
///
/// No audio (see docs); frame data never touches the CPU.
fn record() -> Result<()> {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    };
    use windows::{
        core::factory,
        Foundation::{TimeSpan, TypedEventHandler},
        Graphics::{
            Capture::{
                Direct3D11CaptureFramePool, GraphicsCaptureDirtyRegionMode, GraphicsCaptureItem,
                GraphicsCaptureSession,
            },
            DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        },
        Win32::{
            Graphics::{
                Dxgi::IDXGIDevice,
                Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
            },
            System::WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
                RoInitialize, RO_INIT_MULTITHREADED,
            },
            UI::HiDpi::{
                SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
        },
    };
    let pid = argument("--pid", 0u32)?;
    let desktop = std::env::args().any(|a| a == "--desktop");
    let app_session = std::env::args().any(|a| a == "--app-session");
    let seconds = argument("--seconds", 30u64)?;
    let out_path: String = argument("--out", String::new())?;
    let mode: String = argument("--mode", "monitor".to_string())?;
    let interval_us = argument("--interval-us", 12_500u32)?;
    if (pid == 0 && !desktop)
        || out_path.is_empty()
        || !(5..=if app_session { 86400 } else { 600 }).contains(&seconds)
        || !["monitor", "window"].contains(&mode.as_str())
        || (interval_us != 0 && !(1000..=100_000).contains(&interval_us))
    {
        bail!("Use --record --pid PID --seconds 5..600 --out FILE.h264 [--mode monitor|window] [--interval-us 12500]")
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if !desktop && !allowed_game(pid) {
            bail!("Only CS2, Fortnite or replayx-gameproxy can be recorded by the prototype")
        }
        let window = if desktop {
            windows::Win32::Foundation::HWND::default()
        } else {
            window_in_front(pid).context("The selected game must already be in front")?
        };
        RoInitialize(RO_INIT_MULTITHREADED).context("WinRT initialization")?;
        let _winrt = WinrtRuntime;
        let _runtime = MediaRuntime::start()?;
        let monitor = if desktop {
            windows::Win32::Graphics::Gdi::HMONITOR(replayx_core::geometry::monitor_handle(
                argument("--display", 0u32)?,
            )? as usize as *mut _)
        } else {
            MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST)
        };
        let (adapter, vendor_id, adapter_name) = adapter_for_monitor(monitor)?;
        let luid = adapter.GetDesc()?.AdapterLuid;
        replayx_core::native_gpu::check_capture_adapter(
            &argument("--expected-adapter-luid", String::new())?,
            luid.HighPart,
            luid.LowPart,
        )?;
        if argument("--pacing", String::new())? == "capture-vfr"
            && replayx_core::geometry::display(argument("--display", 0u32)?)?.refresh_hz > 250
        {
            bail!("Timestamp-preserving diagnostic is limited to displays up to 250 Hz; CFR defaults unchanged")
        }
        let pacing: String = argument(
            "--pacing",
            if vendor_id == 0x8086 {
                "legacy".to_owned()
            } else {
                "clocked".to_owned()
            },
        )?;
        if !["legacy", "clocked", "phase-lock", "capture-vfr"].contains(&pacing.as_str()) {
            bail!("Unknown native frame pacing")
        }
        let interval_us = if !std::env::args().any(|a| a == "--interval-us") && pacing != "legacy" {
            0
        } else {
            interval_us
        };
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context): (ID3D11Device, ID3D11DeviceContext) =
            (device.unwrap(), context.unwrap());
        let _ = device
            .cast::<ID3D11Multithread>()?
            .SetMultithreadProtected(true);
        let winrt_device: IDirect3DDevice =
            CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)?.cast()?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = if mode == "monitor" {
            interop.CreateForMonitor(monitor)?
        } else {
            interop.CreateForWindow(window)?
        };
        let size = item.Size()?;
        let input = (size.Width as u32, size.Height as u32);
        if desktop && app_session {
            let geometry = replayx_core::geometry::display(argument("--display", 0u32)?)?;
            let region = geometry.region(None)?;
            if input != (region.width, region.height) {
                bail!("Native capture size does not match the active display; use FFmpeg until geometry is stable")
            }
        }
        let settings = EncoderSettings {
            size: (1920, 1080),
            fps: 60,
            bitrate: if app_session {
                argument("--bitrate", 15_000_000u32)?
            } else {
                15_000_000
            },
            max_bitrate: if app_session {
                argument("--max-bitrate", 23_000_000u32)?
            } else {
                23_000_000
            },
            gop: 120,
        };
        let capture_rate = 1_000_000u32.checked_div(interval_us).unwrap_or(250);
        let mut recorder = NativeRecorder::new(
            &device,
            &context,
            input,
            &settings,
            capture_rate,
            &format!("VEN_{vendor_id:04X}"),
            &out_path,
        )?;
        if pacing == "clocked" && recorder.grid.is_none() {
            recorder.grid = Some(grid::Grid::new(settings.fps));
            recorder.encoder.rational_clock = true;
        }
        if pacing == "phase-lock" {
            recorder.encoder.rational_clock = true;
            recorder.encoder.live_capture_input = true;
        }
        recorder.encoder.guard_pid = if desktop { None } else { Some(pid) };
        // Two buffers, as FFmpeg's gfxcapture.
        let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
            &winrt_device,
            format,
            if recorder.grid.is_some() || recorder.capture_vfr {
                3
            } else {
                2
            },
            size,
        )?;
        let session: GraphicsCaptureSession = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsCursorCaptureEnabled(true);
        let _ = session.SetIsBorderRequired(false);
        // Phase lock: Windows 11 24H2+ reports which parts of each capture
        // changed. Older systems: every capture counts (timestamps only).
        let dirty_regions = pacing == "phase-lock"
            && session
                .SetDirtyRegionMode(GraphicsCaptureDirtyRegionMode::ReportOnly)
                .is_ok();
        if dirty_regions {
            recorder.change_hints = "wgc-dirty-regions";
        }
        if pacing == "phase-lock" {
            recorder.grid = Some(grid::Grid::with_phase_lock(settings.fps, dirty_regions));
        }
        let phase_source = argument("--phase-source", "spatial".to_owned())?;
        if !["global", "spatial"].contains(&phase_source.as_str()) {
            bail!("Unknown phase source")
        }
        let mut spatial = (pacing == "phase-lock" && dirty_regions && phase_source == "spatial")
            .then(|| cadence::Cadence::new(settings.fps));
        if spatial.is_some() {
            recorder.change_hints = "wgc-spatial-cadence";
        }
        let interval_set = session
            .SetMinUpdateInterval(TimeSpan {
                Duration: interval_us as i64 * 10,
            })
            .is_ok();
        // Wake the recording thread when a frame arrives.
        let signal = Arc::new((Mutex::new(false), Condvar::new()));
        let wake = signal.clone();
        let arrival_trace = recorder.encoder.trace_log.clone();
        let arrived_token = pool.FrameArrived(&TypedEventHandler::new(move |_, _| {
            if let Some(log) = &arrival_trace {
                let _ = log.emit("frame_arrived", json!({}));
            }
            let (lock, cv) = &*wake;
            *lock.lock().unwrap() = true;
            cv.notify_one();
            Ok(())
        }))?;
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let closed_token = item.Closed(&TypedEventHandler::new(move |_, _| {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }))?;
        session.StartCapture()?;
        let cpu_before = cpu_ms();
        let thread_before = thread_cpu_ms();
        let began = Instant::now();
        let mut resized = 0usize;
        // Learn during Starting, before publishing either stream's frame-zero
        // clock. Do not drop or retime any frame after recording has begun.
        let mut phase_calibrating = pacing == "phase-lock";
        let mut calibration_frames = 0u64;
        let mut calibration_ms = 0.0;
        let (mut dirty_reads, mut dirty_errors, mut dirty_empty, mut dirty_changed) =
            (0u64, 0u64, 0u64, 0u64);
        let mut process_ms = Vec::new();
        let mut last_guard = Instant::now();
        let result = (|| -> Result<()> {
            while began.elapsed() < Duration::from_secs(seconds) {
                recorder
                    .encoder
                    .check(Instant::now() + Duration::from_secs(5))?;
                if recorder.encoder.stop_requested {
                    break;
                }
                if closed.load(Ordering::SeqCst) {
                    bail!("The capture item was closed")
                }
                // Still in front? Checked at most four times a second.
                if last_guard.elapsed() >= Duration::from_millis(250) {
                    last_guard = Instant::now();
                    if !desktop && window_in_front(pid).is_none() {
                        bail!("The game is no longer in front; recording stopped")
                    }
                }
                {
                    let (lock, cv) = &*signal;
                    let mut ready = lock.lock().unwrap();
                    if !*ready {
                        ready = cv.wait_timeout(ready, recorder.next_wake()).unwrap().0;
                    }
                    *ready = false;
                }
                // Every delivered frame, in order (the selector needs them all).
                while let Ok(frame) = pool.TryGetNextFrame() {
                    let frame = CaptureFrame(frame);
                    if began.elapsed() >= Duration::from_secs(seconds) {
                        break;
                    }
                    recorder
                        .encoder
                        .check(Instant::now() + Duration::from_secs(5))?;
                    let t = Instant::now();
                    let time = frame.SystemRelativeTime()?.Duration;
                    if frame.ContentSize()? != size {
                        resized += 1;
                        if app_session {
                            bail!("Capture geometry changed; stopped to preserve correct frames")
                        }
                        continue;
                    }
                    let source: ID3D11Texture2D = frame
                        .Surface()?
                        .cast::<IDirect3DDxgiInterfaceAccess>()?
                        .GetInterface()?;
                    let dirty = if dirty_regions {
                        reported_dirty_report(&frame)
                    } else {
                        None
                    };
                    let global_changed = if dirty_regions {
                        if let Some(ref report) = dirty {
                            dirty_reads += 1;
                            dirty_empty += u64::from(report.count == 0);
                            let changed = report.count > 128
                                || report.area * 20 >= size.Width as i64 * size.Height as i64;
                            dirty_changed += u64::from(changed);
                            changed
                        } else {
                            dirty_errors += 1;
                            true
                        }
                    } else {
                        true
                    };
                    let (mut changed, mut phase_changed) = (global_changed, global_changed);
                    let mut pending_larger = false;
                    if let Some(tracker) = &mut spatial {
                        phase_changed = false;
                        if let Some(report) = dirty.as_ref().filter(|d| d.count <= 128) {
                            let obs = tracker.observe(
                                time,
                                &report.rects[..report.len],
                                size.Width as i64 * size.Height as i64,
                            );
                            if obs.region.is_some() {
                                changed = obs.changed;
                            }
                            phase_changed = obs.phase_changed;
                            pending_larger = obs.pending_larger;
                            if obs.switched && obs.seed_len > 0 {
                                let phase = recorder.grid.as_mut().unwrap().lock.as_mut().unwrap();
                                phase.reseed(&obs.seed[..obs.seed_len]);
                                if !phase_calibrating {
                                    if let Some(log) = &recorder.encoder.trace_log {
                                        log.emit("phase_reseed",json!({"before_capture":recorder.captures,"delta_hns":phase.delta,"observations":phase.absolute_observations()}))?;
                                    }
                                }
                            }
                            if let Some(log) = &recorder.encoder.trace_log {
                                log.emit("spatial_cadence",json!({"before_capture":recorder.captures,"capture_hns":time,"region":obs.region,"changed":changed,"phase_changed":phase_changed,"pending_larger":pending_larger}))?;
                            }
                        }
                    }
                    if phase_calibrating {
                        let lock = recorder.grid.as_mut().unwrap().lock.as_mut().unwrap();
                        if (!lock.ready() || pending_larger)
                            && began.elapsed() < Duration::from_millis(500)
                        {
                            if phase_changed {
                                lock.arrival(time);
                                lock.update();
                            }
                            calibration_frames += 1;
                            if let Some(log) = &recorder.encoder.trace_log {
                                log.emit("phase_calibration",json!({"capture_hns":time,"changed":phase_changed,"global_changed":global_changed,"ordinal":calibration_frames,"dirty_rects":dirty.as_ref().map(|d|&d.rects[..d.len])}))?;
                            }
                            continue;
                        }
                        lock.rebase_before_start(time);
                        if let Some(log) = &recorder.encoder.trace_log {
                            log.emit("phase_start",json!({"capture_hns":time,"delta_hns":lock.delta,"observations":lock.absolute_observations()}))?;
                        }
                        phase_calibrating = false;
                        calibration_ms = began.elapsed().as_secs_f64() * 1000.;
                    }
                    if let Some(log) = &recorder.encoder.trace_log {
                        log.emit("capture_dirty_regions",json!({"capture":recorder.captures,"api_enabled":dirty_regions,"read_ok":dirty.is_some(),"area_sum":dirty.as_ref().map(|d|d.area),"rectangles":dirty.as_ref().map(|d|d.count),"changed":global_changed,"frame_area":size.Width as i64*size.Height as i64,"dirty_rects":dirty.as_ref().map(|d|&d.rects[..d.len])}))?;
                    }
                    recorder.deliver(
                        &device,
                        &context,
                        &source,
                        time,
                        Some(frame),
                        (changed, phase_changed),
                    )?;
                    drop(source);
                    bounded_push(&mut process_ms, t.elapsed().as_secs_f64() * 1000.);
                }
                recorder.advance_grid(
                    &device,
                    &context,
                    trace::qpc() - 10_000_000 / settings.fps as i64,
                    true,
                )?;
            }
            Ok(())
        })();
        let end_capture = trace::qpc();
        if result.is_ok() {
            recorder.seal_end(&device, &context, end_capture)?;
        }
        if let Some(probe) = &mut recorder.probe {
            context.Flush();
            let until = Instant::now() + Duration::from_secs(2);
            while probe.pending() > 0 && Instant::now() < until {
                probe.poll(&context)?;
                std::thread::sleep(Duration::from_millis(1));
            }
            if probe.pending() > 0 {
                bail!("Diagnostic GPU probes did not complete")
            }
        }
        let _ = pool.RemoveFrameArrived(arrived_token);
        let _ = item.RemoveClosed(closed_token);
        let _ = session.Close();
        let finished = if result.is_ok() {
            recorder.finish()
        } else {
            recorder.encoder.output.flush().map_err(Into::into)
        };
        recorder.pending_capture.take();
        let _ = pool.Close();
        let elapsed = began.elapsed().as_secs_f64();
        let cpu = cpu_ms() - cpu_before;
        let loop_cpu = thread_cpu_ms() - thread_before;

        let mut report = recorder.report();
        report["recording_loop_cpu_percent_of_one_core"] = json!(loop_cpu / elapsed / 10.0);
        report["success"] = json!(result.is_ok() && finished.is_ok());
        report["error"] = json!(result
            .as_ref()
            .err()
            .or(finished.as_ref().err())
            .map(|e| format!("{e:#}")));
        report["mode"] = json!(mode);
        report["adapter"] = json!(adapter_name);
        report["capture_size"] = json!([input.0, input.1]);
        report["phase_source"] = json!(if spatial.is_some() {
            "spatial"
        } else if pacing == "phase-lock" {
            "global"
        } else {
            "none"
        });
        report["spatial_cadence"]=spatial.as_ref().map(|c|json!({"selected_region":c.selected_region(),"switches":c.switches,"irregular_phase_observations_quarantined":c.irregular_observations,"max_tracks":24,"history_per_track":32})).unwrap_or(serde_json::Value::Null);
        report["min_update_interval_us"] = json!(interval_us);
        report["min_update_interval_accepted"] = json!(interval_set);
        report["capture_rate_for_selection"] = json!(capture_rate);
        report["resized_frames_skipped"] = json!(resized);
        report["dirty_region_observations"] = json!({"api_enabled":dirty_regions,"successful_reads":dirty_reads,"read_errors":dirty_errors,"empty_reports":dirty_empty,"changed_reports":dirty_changed});
        report["phase_startup_calibration"] = json!({"frames_observed_before_origin":calibration_frames,"milliseconds_before_origin":calibration_ms,"max_wait_ms":500,"clock_established_after_calibration":pacing=="phase-lock"});
        report["seconds"] = json!(elapsed);
        report["captures_per_second"] = json!(recorder.captures as f64 / elapsed);
        report["cpu_percent_of_one_core"] = json!(cpu / elapsed / 10.0);
        report["per_capture_processing"] = json!(stats(process_ms));
        report["output"] = json!(out_path);
        report["audio"] = json!("not recorded by the prototype (video only)");
        println!("{}", serde_json::to_string_pretty(&report)?);
        result?;
        finished?;
    }
    Ok(())
}

// Diagnostic only: compare enumeration scopes without using a globally
// enumerated transform for recording or opening a capture source.
fn probe_mft_activation() -> Result<()> {
    let vendor: String = argument("--vendor", "10de".to_string())?;
    let mode: String = argument("--scope", "adapter".to_string())?;
    let (device, _, adapter) = unsafe { video_device(u32::from_str_radix(&vendor, 16)?) }?;
    let luid = unsafe {
        device
            .cast::<windows::Win32::Graphics::Dxgi::IDXGIDevice>()?
            .GetAdapter()?
            .GetDesc()?
            .AdapterLuid
    };
    let luid = (u64::from(luid.HighPart as u32) << 32) | u64::from(luid.LowPart);
    let scope = match mode.as_str() {
        "adapter" => Some(luid),
        "global" => None,
        _ => bail!("Scope must be adapter or global"),
    };
    unsafe {
        let _runtime = MediaRuntime::start()?;
        let mut encoders = Vec::new();
        for (name, id, _, activate) in MfEncoder::list_for_adapter(scope)? {
            if !id.eq_ignore_ascii_case(&format!("VEN_{vendor}")) {
                continue;
            }
            let stage = match activate.ActivateObject::<IMFTransform>() {
                Ok(transform) => {
                    let _shutdown = TransformShutdown(transform.clone());
                    let capabilities = transform.GetAttributes()?;
                    json!({
                        "activated": true,
                        "d3d11_aware": capabilities.GetUINT32(&MF_SA_D3D11_AWARE).ok(),
                        "async": capabilities.GetUINT32(&MF_TRANSFORM_ASYNC).ok(),
                    })
                }
                Err(error) => json!({
                    "activated": false,
                    "hresult": format!("0x{:08X}", error.code().0 as u32),
                }),
            };
            encoders.push(json!({
                "name": name,
                "vendor": id,
                "clsid": activate.GetGUID(&MFT_TRANSFORM_CLSID_Attribute).ok().map(|v| format!("{v:?}")),
                "hardware_url_present": activate.GetStringLength(&MFT_ENUM_HARDWARE_URL_Attribute).is_ok_and(|n| n > 0),
                "activation_d3d11_aware": activate.GetUINT32(&MF_SA_D3D11_AWARE).ok(),
                "activation_luid_blob_bytes": activate.GetBlobSize(&MFT_ENUM_ADAPTER_LUID).ok(),
                "transform": stage,
            }));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "adapter": adapter,
                "adapter_luid": format!("{luid:016X}"),
                "scope": mode,
                "encoders": encoders,
            }))?
        );
    }
    Ok(())
}

fn feasibility(out_path: &str) -> Result<()> {
    let frames: usize = argument("--frames", 600usize)?;
    let paced = argument("--paced", 0u8)? != 0;
    let vendor_hex: String = argument("--vendor", "8086".to_string())?;
    let vendor_id = u32::from_str_radix(&vendor_hex, 16)?;
    unsafe {
        let _runtime = MediaRuntime::start()?;
        let encoders: Vec<_> = MfEncoder::list()?
            .into_iter()
            .map(|(n, v, d, _)| json!({"name":n, "vendor":v, "d3d11":d}))
            .collect();
        let (adapter, adapter_name) =
            adapter_by_vendor(vendor_id).context("No GPU from that vendor")?;
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
        .with_context(|| format!("Create D3D11 device on {adapter_name}"))?;
        let (device, context): (ID3D11Device, ID3D11DeviceContext) =
            (device.unwrap(), context.unwrap());
        // Media Foundation uses the device from its own threads.
        let _ = device
            .cast::<ID3D11Multithread>()?
            .SetMultithreadProtected(true);
        let settings = EncoderSettings {
            size: (1920, 1080),
            fps: 60,
            bitrate: 15_000_000,
            max_bitrate: 23_000_000,
            gop: 120,
        };
        let mut native = Native::new(&device, &context, (2560, 1600), settings.size, false)
            .with_context(|| format!("Create D3D11 video converter on {adapter_name}"))?;
        let mut encoder =
            MfEncoder::new(&device, &format!("VEN_{vendor_hex}"), &settings, out_path)?;
        // A synthetic "display" frame: BGRA 2560x1600, a new colour per frame.
        let source = texture(
            &device,
            (2560, 1600),
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_USAGE_DEFAULT,
            D3D11_BIND_FLAG(D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0),
            D3D11_CPU_ACCESS_FLAG(0),
        )?;
        let mut rtv = None;
        device.CreateRenderTargetView(&source, None, Some(&mut rtv))?;
        let rtv = rtv.unwrap();
        // NV12 frames the encoder may still be reading: a ring.
        let ring: Vec<ID3D11Texture2D> = (0..8)
            .map(|_| {
                texture(
                    &device,
                    settings.size,
                    DXGI_FORMAT_NV12,
                    D3D11_USAGE_DEFAULT,
                    D3D11_BIND_FLAG(D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0),
                    D3D11_CPU_ACCESS_FLAG(0),
                )
            })
            .collect::<Result<_>>()?;
        let duration = 10_000_000 / settings.fps as i64;
        let cpu_before = cpu_ms();
        let began = Instant::now();
        let mut submit_ms = Vec::new();
        for n in 0..frames {
            if paced {
                let due = began + Duration::from_secs_f64(n as f64 / settings.fps as f64);
                if let Some(d) = due.checked_duration_since(Instant::now()) {
                    std::thread::sleep(d);
                }
            }
            let x = n as f32;
            context.ClearRenderTargetView(
                &rtv,
                &[
                    (x * 0.013).sin() * 0.5 + 0.5,
                    (x * 0.021).sin() * 0.5 + 0.5,
                    (x * 0.034).sin() * 0.5 + 0.5,
                    1.0,
                ],
            );
            let t = Instant::now();
            encoder.wait_surface(&ring[n % ring.len()])?;
            native.convert_to(&device, &context, &source, &ring[n % ring.len()])?;
            context.Flush();
            encoder.encode(&ring[n % ring.len()], n as i64 * duration, duration)?;
            bounded_push(&mut submit_ms, t.elapsed().as_secs_f64() * 1000.);
        }
        encoder.finish()?;
        let elapsed = began.elapsed().as_secs_f64();
        let cpu = cpu_ms() - cpu_before;

        let ordered = encoder.timestamps_increasing;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "feasibility":true,"no_b_frames":"validated in every output access unit","capture_encoder_adapter_luid":format!("{:016X}",encoder.adapter_luid),
                "hardware_h264_encoders":encoders,
                "adapter":adapter_name, "encoder":encoder.name, "encoder_vendor":encoder.vendor,
                "input":"D3D11 NV12 texture (MFCreateDXGISurfaceBuffer), same device as the converter, no CPU readback",
                "settings_accepted":encoder.settings_accepted.iter().map(|(k, v)| json!({k: v})).collect::<Vec<_>>(),
                "frames_in":frames, "frames_out":encoder.frames_out,
                "keyframes_at":encoder.keyframes.iter().take(8).collect::<Vec<_>>(),
                "keyframe_count":encoder.keyframe_count,
                "output_timestamps_increasing":ordered,
                "bytes":encoder.bytes_written,
                "mean_bitrate_mbps":encoder.bytes_written as f64 * 8. / (frames as f64 / settings.fps as f64) / 1e6,
                "paced":paced, "seconds":elapsed, "throughput_fps":frames as f64 / elapsed,
                "cpu_ms_per_frame":cpu / frames as f64,
                "submit_ms":stats(submit_ms), "encode_latency":stats(encoder.latency_ms.clone()),
                "output":out_path,
            }))?
        );
        if encoder.frames_out != frames {
            bail!("{} frames in, {} out", frames, encoder.frames_out)
        }
    }
    Ok(())
}

/// `--select-test TIMES`: run the frame selector on capture times (100 ns,
/// one per line) and print, per output slot, the index of the captured
/// frame it shows, plus FFmpeg's drop/duplicate counters.
fn select_test(path: &str) -> Result<()> {
    let fps = argument("--fps", 60u32)?;
    // gfxcapture's max_framerate: 80 at the recorder's 12.5 ms floor.
    let capture_rate = argument("--capture-rate", 80u32)?;
    let times: Vec<i64> = std::fs::read_to_string(path)?
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .collect();
    let mut s = CfrSelector::new(fps, capture_rate);
    let mut slots = Vec::new();
    for (n, t) in times.iter().enumerate() {
        for (slot, source) in s.push(*t) {
            debug_assert_eq!(slot as usize, slots.len());
            slots.push(match source {
                Source::Current => n,
                Source::Previous => n - 1,
            });
        }
    }
    let last = times.len().saturating_sub(1);
    for (_, _) in s.finish() {
        slots.push(last);
    }
    println!(
        "{}",
        serde_json::to_string(
            &json!({"frames_in":times.len(), "slots":slots, "dup_frames":s.dups, "drop_frames":s.drops, "repeats_of_dropped_frames":s.repeats_of_dropped})
        )?
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let worker = args.iter().any(|a| a == "--native-worker");
    if args.iter().any(|a| a == "--watchdog-test") {
        if !worker {
            return supervise(Duration::from_millis(200));
        }
        std::thread::sleep(Duration::from_secs(60));
        return Ok(());
    }
    if !worker
        && args.iter().any(|a| {
            ["--record", "--synthetic", "--feasibility", "--probe-mft"].contains(&a.as_str())
        })
    {
        let seconds = argument("--seconds", 30u64)?;
        if seconds
            > if args.iter().any(|a| a == "--app-session") {
                86400
            } else {
                600
            }
        {
            bail!("Native operation exceeds the 600 second limit")
        }
        return supervise(Duration::from_secs(seconds + 30));
    }
    if let Some(i) = args.iter().position(|a| a == "--select-test") {
        return select_test(args.get(i + 1).context("times file required")?);
    }
    if let Some(i) = args.iter().position(|a| a == "--pacing-replay") {
        return pacing_replay(args.get(i + 1).context("timeline file required")?);
    }
    if args.iter().any(|a| a == "--record") {
        return record();
    }
    if let Some(i) = args.iter().position(|a| a == "--bench-guard") {
        // Cost of the record loop's per-capture "game still in front" check
        // (diagnostic; captures nothing).
        let n: u32 = args.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(500);
        let pid = std::process::id();
        let measure = |f: &dyn Fn()| {
            let (cpu0, t) = (cpu_ms(), Instant::now());
            for _ in 0..n {
                f();
            }
            json!({"wall_ms_per_call":t.elapsed().as_secs_f64()*1000./n as f64,
                   "cpu_ms_per_call":(cpu_ms()-cpu0)/n as f64})
        };
        println!(
            "{}",
            json!({"calls":n,
                   "process_snapshot_check (once, at start)":measure(&|| { std::hint::black_box(allowed_game(pid)); }),
                   "foreground_check (loop, at most 4/s)":measure(&|| { std::hint::black_box(window_in_front(pid)); })})
        );
        return Ok(());
    }
    if let Some(i) = args.iter().position(|a| a == "--synthetic") {
        return synthetic(
            args.get(i + 1).context("scenario required")?,
            args.get(i + 2).context("output path required")?,
        );
    }
    if args.iter().any(|a| a == "--probe-mft") {
        return probe_mft_activation();
    }
    match args.iter().position(|a| a == "--feasibility") {
        Some(i) => feasibility(args.get(i + 1).context("output path required")?),
        None => bail!("usage: replayx-native-proto --feasibility OUT.h264 [--frames 600] [--paced 0|1] [--vendor 8086] | --select-test TIMES [--fps 60] [--capture-rate 80] | --synthetic SCENARIO OUT.h264 [--seconds 8] [--paced 0|1] | --record --pid PID --seconds N --out FILE.h264 [--mode monitor|window]"),
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;
    #[test]
    fn diagnostic_storage_is_bounded_for_long_recordings() {
        let mut samples = Vec::new();
        for n in 0..100_000 {
            bounded_push(&mut samples, n);
        }
        assert_eq!(samples.len(), DIAGNOSTIC_SAMPLES);
    }
}
