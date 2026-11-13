//! Windows Graphics Capture counterpart of `replayx-dda-profile`: capture only,
//! no encoder, so its cost can be compared with Desktop Duplication's.
//! Public, documented APIs only (Windows.Graphics.Capture + Direct3D 11). No
//! pixel readback, file output, input, injection or setting changes. The
//! selected process (REPLAYX's synthetic fixture or an already running game)
//! must stay in front; Windows' capture border stays on unless --border 0.
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{factory, Interface},
    Foundation::{TimeSpan, TypedEventHandler},
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
        SizeInt32,
    },
    Win32::{
        Foundation::{HMODULE, HWND, POINT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{CreateDXGIFactory1, IDXGIAdapter, IDXGIDevice, IDXGIFactory1},
            Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
        },
        System::{
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
            WinRT::{
                Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
                Graphics::Capture::IGraphicsCaptureItemInterop,
                RoInitialize, RO_INIT_MULTITHREADED,
            },
        },
        UI::{
            HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            WindowsAndMessaging::{
                GetAncestor, GetForegroundWindow, GetSystemMetrics, GetWindowThreadProcessId,
                IsIconic, WindowFromPoint, GA_ROOT, SM_CXSCREEN, SM_CYSCREEN,
            },
        },
    },
};

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

/// The selected process's window: the foreground window, or (REPLAYX's
/// fixture kept topmost when Windows refuses it the foreground) the
/// top-level window covering the centre of the primary display.
fn target_window(pid: u32) -> Option<HWND> {
    unsafe {
        let owner = |window: HWND| {
            let mut actual = 0;
            GetWindowThreadProcessId(window, Some(&mut actual));
            actual
        };
        let foreground = GetForegroundWindow();
        if owner(foreground) == pid && !IsIconic(foreground).as_bool() {
            return Some(foreground);
        }
        let centre = POINT {
            x: GetSystemMetrics(SM_CXSCREEN) / 2,
            y: GetSystemMetrics(SM_CYSCREEN) / 2,
        };
        let window = GetAncestor(WindowFromPoint(centre), GA_ROOT);
        (owner(window) == pid && !IsIconic(window).as_bool()).then_some(window)
    }
}
fn foreground(pid: u32) -> bool {
    target_window(pid).is_some()
}

/// QueryPerformanceCounter in 100 ns units: the clock of SystemRelativeTime.
fn now_100ns() -> i64 {
    let (mut counter, mut frequency) = (0i64, 0i64);
    unsafe {
        let _ = QueryPerformanceCounter(&mut counter);
        let _ = QueryPerformanceFrequency(&mut frequency);
    }
    (counter as i128 * 10_000_000 / frequency.max(1) as i128) as i64
}

/// GPU timestamp span of one copy, read back without waiting (as in
/// replayx-dda-profile).
struct Timer {
    disjoint: ID3D11Query,
    start: ID3D11Query,
    end: ID3D11Query,
    pending: Option<Instant>,
    timeouts: usize,
    disabled: bool,
}
impl Timer {
    unsafe fn new(device: &ID3D11Device) -> windows::core::Result<Self> {
        let query = |kind| {
            let mut q = None;
            device.CreateQuery(
                &D3D11_QUERY_DESC {
                    Query: kind,
                    MiscFlags: 0,
                },
                Some(&mut q),
            )?;
            Ok::<_, windows::core::Error>(q.unwrap())
        };
        Ok(Self {
            disjoint: query(D3D11_QUERY_TIMESTAMP_DISJOINT)?,
            start: query(D3D11_QUERY_TIMESTAMP)?,
            end: query(D3D11_QUERY_TIMESTAMP)?,
            pending: None,
            timeouts: 0,
            disabled: false,
        })
    }
    unsafe fn poll(&mut self, context: &ID3D11DeviceContext) -> Option<f64> {
        let began = self.pending?;
        let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
        let (mut start, mut end) = (0u64, 0u64);
        let flags = D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32;
        let ready = context
            .GetData(
                &self.disjoint,
                Some(&mut disjoint as *mut _ as *mut c_void),
                std::mem::size_of_val(&disjoint) as u32,
                flags,
            )
            .is_ok()
            && context
                .GetData(
                    &self.start,
                    Some(&mut start as *mut _ as *mut c_void),
                    8,
                    flags,
                )
                .is_ok()
            && context
                .GetData(&self.end, Some(&mut end as *mut _ as *mut c_void), 8, flags)
                .is_ok();
        if ready && start > 0 && end >= start && disjoint.Frequency > 0 {
            self.pending = None;
            return (!disjoint.Disjoint.as_bool())
                .then(|| (end - start) as f64 / disjoint.Frequency as f64 * 1000.);
        }
        if began.elapsed() > Duration::from_secs(2) {
            self.timeouts += 1;
            self.pending = None;
            self.disabled = true;
        }
        None
    }
}

fn main() -> Result<()> {
    let pid = argument("--pid", 0u32)?;
    let seconds = argument("--seconds", 20u64)?;
    let mode = argument("--mode", "monitor".to_string())?;
    let border = argument("--border", 1u8)? != 0;
    let cursor = argument("--cursor", 1u8)? != 0;
    let fps = argument("--fps", 60u32)?;
    let buffers = argument("--buffers", 2i32)?;
    // 1 = limit Windows to `fps` updates (MinUpdateInterval), 0 = every
    // composed update (the display refresh rate or the content's rate).
    let limit = argument("--limit", 1u8)? != 0;
    if pid == 0
        || !(5..=120).contains(&seconds)
        || !["monitor", "window"].contains(&mode.as_str())
        || !(1..=240).contains(&fps)
        || !(1..=4).contains(&buffers)
    {
        bail!("Use --pid PID --seconds 5..120 --mode monitor|window --border 1|0 --cursor 1|0 --fps 60 --buffers 1..4 --limit 1|0")
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let mut sys = sysinfo::System::new();
    let id = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[id]), true);
    if !sys.process(id).is_some_and(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        ["fortniteclient-win64-shipping.exe", "replayx-gameproxy.exe"].contains(&name.as_str())
    }) || !foreground(pid)
    {
        bail!("The selected game (Fortnite or replayx-gameproxy) must already be foreground")
    }
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED).context("WinRT initialization")?;
        if !GraphicsCaptureSession::IsSupported()? {
            bail!("Windows Graphics Capture is not supported on this system")
        }
        let window = target_window(pid).context("The selected window is no longer visible")?;
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        // The adapter driving the primary display, as Desktop Duplication uses.
        let factory_dxgi: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory_dxgi.EnumAdapters1(0)?;
        let adapter_desc = adapter.GetDesc1()?;
        let mut device = None;
        let mut context = None;
        D3D11CreateDevice(
            &adapter.cast::<IDXGIAdapter>()?,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context): (ID3D11Device, ID3D11DeviceContext) =
            (device.unwrap(), context.unwrap());
        let winrt_device: IDirect3DDevice =
            CreateDirect3D11DeviceFromDXGIDevice(&device.cast::<IDXGIDevice>()?)?.cast()?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = if mode == "monitor" {
            interop.CreateForMonitor(monitor)?
        } else {
            interop.CreateForWindow(window)?
        };
        let closed = Arc::new(AtomicBool::new(false));
        let flag = closed.clone();
        let closed_token = item.Closed(&TypedEventHandler::new(move |_, _| {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }))?;
        let mut size = item.Size()?;
        let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
        let pool =
            Direct3D11CaptureFramePool::CreateFreeThreaded(&winrt_device, format, buffers, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        let cursor_set = session.SetIsCursorCaptureEnabled(cursor).is_ok();
        // Windows may refuse to hide its border for this app: report, never force.
        let border_set = session.SetIsBorderRequired(border);
        let interval = TimeSpan {
            Duration: 10_000_000 / fps as i64,
        };
        let limit_set = limit && session.SetMinUpdateInterval(interval).is_ok();
        let mut timer = Timer::new(&device)?;
        let (mut latency_ms, mut interval_ms, mut copy_submit_ms, mut copy_gpu_ms, mut drain) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut texture: Option<ID3D11Texture2D> = None;
        let (mut frames, mut used, mut empty_slots, mut resizes) = (0usize, 0usize, 0usize, 0usize);
        let mut last_time = 0i64;
        session.StartCapture()?;
        let began = Instant::now();
        let result = (|| -> Result<()> {
            let mut slot = 0u32;
            while began.elapsed() < Duration::from_secs(seconds) {
                if closed.load(Ordering::SeqCst) {
                    bail!("The capture item was closed")
                }
                if !foreground(pid) {
                    bail!("Foreground changed; profile invalidated")
                }
                let next = began + Duration::from_secs_f64(slot as f64 / fps as f64);
                if let Some(delay) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(delay);
                }
                slot += 1;
                // Take everything queued and keep the newest, like a recorder
                // sampling at its output rate.
                let mut newest = None;
                let mut taken = 0;
                while let Ok(frame) = pool.TryGetNextFrame() {
                    taken += 1;
                    frames += 1;
                    let t = frame.SystemRelativeTime()?.Duration;
                    if last_time != 0 {
                        interval_ms.push((t - last_time) as f64 / 10_000.);
                    }
                    last_time = t;
                    if let Some(old) = newest.replace(frame) {
                        old.Close()?;
                    }
                }
                drain.push(taken as f64);
                let Some(frame) = newest else {
                    empty_slots += 1;
                    continue;
                };
                used += 1;
                latency_ms.push((now_100ns() - last_time) as f64 / 10_000.);
                let content = frame.ContentSize()?;
                if content != size {
                    // Resize: recreate the pool at the new size; drop this frame.
                    resizes += 1;
                    size = SizeInt32 {
                        Width: content.Width.max(1),
                        Height: content.Height.max(1),
                    };
                    frame.Close()?;
                    pool.Recreate(&winrt_device, format, buffers, size)?;
                    texture = None;
                    continue;
                }
                let source: ID3D11Texture2D = frame
                    .Surface()?
                    .cast::<IDirect3DDxgiInterfaceAccess>()?
                    .GetInterface()?;
                if texture.is_none() {
                    let mut desc = D3D11_TEXTURE2D_DESC::default();
                    source.GetDesc(&mut desc);
                    desc.Usage = D3D11_USAGE_DEFAULT;
                    desc.BindFlags = 0;
                    desc.CPUAccessFlags = 0;
                    desc.MiscFlags = 0;
                    desc.ArraySize = 1;
                    desc.MipLevels = 1;
                    let mut t = None;
                    device.CreateTexture2D(&desc, None, Some(&mut t))?;
                    texture = t;
                }
                if let Some(ms) = timer.poll(&context) {
                    copy_gpu_ms.push(ms);
                }
                let sample = used.is_multiple_of(16) && timer.pending.is_none() && !timer.disabled;
                if sample {
                    context.Begin(&timer.disjoint);
                    context.End(&timer.start);
                }
                let t = Instant::now();
                context.CopyResource(texture.as_ref().unwrap(), &source);
                copy_submit_ms.push(t.elapsed().as_secs_f64() * 1000.);
                if sample {
                    context.End(&timer.end);
                    context.End(&timer.disjoint);
                    timer.pending = Some(Instant::now());
                }
                context.Flush();
                drop(source);
                frame.Close()?;
            }
            Ok(())
        })();
        let _ = item.RemoveClosed(closed_token);
        let _ = session.Close();
        let _ = pool.Close();
        let elapsed = began.elapsed().as_secs_f64();
        let name = String::from_utf16_lossy(&adapter_desc.Description)
            .trim_end_matches('\0')
            .to_string();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "success":result.is_ok(), "error":result.as_ref().err().map(ToString::to_string),
                "api":"windows-graphics-capture", "mode":mode, "adapter":name,
                "border_requested":border, "border_setting_accepted":border_set.is_ok(),
                "border_error":border_set.err().map(|e| e.message().to_string()),
                "cursor_setting_accepted":cursor_set,
                "min_update_interval_requested":limit, "min_update_interval_accepted":limit_set,
                "pool_buffers":buffers, "sample_fps":fps, "seconds":elapsed,
                "item_size":[size.Width, size.Height], "resizes":resizes,
                "frames_delivered":frames, "delivered_fps":frames as f64/elapsed,
                "frames_used":used, "slots_without_new_frame":empty_slots,
                "frames_taken_per_slot":{"mean":drain.iter().sum::<f64>()/drain.len().max(1) as f64, "max":drain.iter().cloned().fold(0., f64::max)},
                "source_frame_interval":stats(interval_ms),
                "capture_to_processing_latency":stats(latency_ms),
                "cpu_copy_submission":stats(copy_submit_ms), "gpu_copy_span":stats(copy_gpu_ms),
                "gpu_query_timeouts":timer.timeouts,
                "limits":"Diagnostic only, not REPLAYX recording or game FPS. Latency is from Windows' frame timestamp to this process taking the frame at its sampling slot. GPU spans include scheduling. No pixel readback, NVENC or audio."
            }))?
        );
        result
    }
}
