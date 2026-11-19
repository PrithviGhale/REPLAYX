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

#[path = "../native_worker/common.rs"]
mod common;
use common::{adapter_by_vendor, texture, Native};

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

/// `--self-test`: check the native stages without capturing anything or
/// showing a window. Solid colours through the video processor must give
/// BT.709 limited-range NV12 values (within 3 levels), and the bytes moved
/// to the NVIDIA GPU must read back unchanged.
fn self_test() -> Result<()> {
    use windows::Win32::Graphics::Dxgi::Common::{
        DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8_UNORM,
    };
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory.EnumAdapters1(0)?;
        let name = String::from_utf16_lossy(&adapter.GetDesc1()?.Description)
            .trim_end_matches('\0')
            .to_string();
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            &adapter.cast::<IDXGIAdapter>()?,
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
        let input = (2560, 1600);
        let out = (1920u32, 1080u32);
        let transfer = adapter_by_vendor(0x10de).is_some();
        let mut native = Native::new(&device, &context, input, out, transfer)?;
        // Like WGC frames: BGRA, readable by shaders, renderable.
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
        let read = texture(
            &device,
            out,
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12,
            D3D11_USAGE_STAGING,
            D3D11_BIND_FLAG(0),
            D3D11_CPU_ACCESS_READ,
        )?;
        // (name, RGB, expected Y, Cb, Cr) for BT.709 limited range.
        let cases = [
            ("black", [0., 0., 0.], [16, 128, 128]),
            ("white", [1., 1., 1.], [235, 128, 128]),
            ("red", [1., 0., 0.], [63, 102, 240]),
            ("green", [0., 1., 0.], [173, 42, 26]),
            ("blue", [0., 0., 1.], [32, 240, 118]),
            ("grey", [0.5, 0.5, 0.5], [126, 128, 128]),
        ];
        let mut results = Vec::new();
        let mut ok = true;
        for (label, rgb, expected) in cases {
            context.ClearRenderTargetView(&rtv, &[rgb[0], rgb[1], rgb[2], 1.0]);
            native.convert(&device, &context, &source)?;
            context.CopyResource(&read, &native.nv12);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&read, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let pitch = mapped.RowPitch as usize;
            let base = mapped.pData as *const u8;
            let (cx, cy) = (out.0 as usize / 2, out.1 as usize / 2);
            let y = *base.add(cy * pitch + cx);
            let uv = base.add(out.1 as usize * pitch + (cy / 2) * pitch + (cx & !1));
            let (cb, cr) = (*uv, *uv.add(1));
            context.Unmap(&read, 0);
            let got = [y, cb, cr];
            let pass = got
                .iter()
                .zip(expected)
                .all(|(g, e)| (*g as i32 - e).abs() <= 3);
            ok &= pass;
            results.push(json!({"colour":label, "expected":expected, "got":got, "pass":pass}));
        }
        // Transfer: the converted grey frame through the ring to NVIDIA, read
        // back there and compared byte for byte.
        let mut transfer_result = json!({"skipped":"no NVIDIA GPU"});
        if native.transfer.is_some() {
            for _ in 0..6 {
                native.transfer(&context)?;
                context.Flush();
                std::thread::sleep(Duration::from_millis(30));
            }
            let t = native.transfer.as_ref().unwrap();
            let back = texture(
                &t.device,
                (out.0, out.1 * 3 / 2),
                DXGI_FORMAT_R8_UNORM,
                D3D11_USAGE_STAGING,
                D3D11_BIND_FLAG(0),
                D3D11_CPU_ACCESS_READ,
            )?;
            t.context.CopyResource(&back, &t.target);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            t.context
                .Map(&back, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let (w, rows) = (out.0 as usize, out.1 as usize * 3 / 2);
            let mut same = t.uploaded > 0;
            for row in 0..rows {
                let a = std::slice::from_raw_parts(
                    (mapped.pData as *const u8).add(row * mapped.RowPitch as usize),
                    w,
                );
                same &= a == &t.host[row * w..(row + 1) * w];
            }
            t.context.Unmap(&back, 0);
            ok &= same;
            transfer_result =
                json!({"adapter":t.adapter, "frames_uploaded":t.uploaded, "bytes_identical":same});
        }
        // Each stage's own GPU time per frame on the capture GPU (synthetic
        // frames; not game impact): the original probe's full-resolution
        // copy against the native fixed-function conversion. Timed from
        // submission until the GPU reports completion (an event query),
        // because the video processor runs on its own engine, which the 3D
        // queue's timestamp queries do not cover. Includes a small
        // scheduling overhead in both.
        let copy_target = texture(
            &device,
            input,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_USAGE_DEFAULT,
            D3D11_BIND_FLAG(0),
            D3D11_CPU_ACCESS_FLAG(0),
        )?;
        let mut done = None;
        device.CreateQuery(
            &D3D11_QUERY_DESC {
                Query: D3D11_QUERY_EVENT,
                MiscFlags: 0,
            },
            Some(&mut done),
        )?;
        let done: ID3D11Query = done.unwrap();
        let span = |work: &mut dyn FnMut() -> Result<()>| -> Result<Vec<f64>> {
            let mut out = Vec::new();
            for _ in 0..120 {
                // Start from an idle GPU.
                context.End(&done);
                let mut finished = windows::core::BOOL(0);
                while context
                    .GetData(&done, Some(&mut finished as *mut _ as *mut c_void), 4, 0)
                    .is_err()
                    || !finished.as_bool()
                {
                    std::hint::spin_loop();
                }
                let began = Instant::now();
                work()?;
                context.End(&done);
                context.Flush();
                let mut finished = windows::core::BOOL(0);
                while context
                    .GetData(&done, Some(&mut finished as *mut _ as *mut c_void), 4, 0)
                    .is_err()
                    || !finished.as_bool()
                {
                    std::hint::spin_loop();
                }
                out.push(began.elapsed().as_secs_f64() * 1000.);
            }
            Ok(out)
        };
        let copy_ms = span(&mut || {
            context.CopyResource(&copy_target, &source);
            Ok(())
        })?;
        let convert_ms = span(&mut || native.convert(&device, &context, &source))?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "self_test":true, "pass":ok, "capture_adapter":name, "input":[input.0, input.1],
                "output":[out.0, out.1], "conversion":results, "transfer":transfer_result,
                "intermediate_copies":native.intermediate_copies,
                "gpu_time_per_frame":{"full_resolution_copy":stats(copy_ms), "fixed_function_convert":stats(convert_ms)},
                "transfer_upload_cpu":native.transfer.as_ref().map(|t| stats(t.upload_cpu_ms.clone())),
            }))?
        );
        if !ok {
            bail!("self-test failed")
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--self-test") {
        return self_test();
    }
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
    // What happens to each chosen frame (one per --fps slot):
    //   none          release it at once (Windows' capture cost alone)
    //   copy          one full-resolution GPU copy (the original probe)
    //   convert       native path: fixed-function scale + NV12 (VideoProcessor)
    //   convert-xfer  convert, then move the NV12 frame to the NVIDIA GPU
    let stage = argument("--stage", "copy".to_string())?;
    // Windows' minimum update interval; default one --fps slot. REPLAYX's
    // recorder uses 12500 us at 60 FPS (about 80 deliveries/s).
    let interval_us = argument("--interval-us", 1_000_000u32 / fps.max(1))?;
    let out_size = (
        argument("--out-width", 1920u32)?,
        argument("--out-height", 1080u32)?,
    );
    if pid == 0
        || !(5..=120).contains(&seconds)
        || !["monitor", "window"].contains(&mode.as_str())
        || !(1..=240).contains(&fps)
        || !(1..=4).contains(&buffers)
        || !["none", "copy", "convert", "convert-xfer"].contains(&stage.as_str())
        || !(1000..=100_000).contains(&interval_us)
    {
        bail!("Use --pid PID --seconds 5..120 --mode monitor|window --border 1|0 --cursor 1|0 --fps 60 --buffers 1..4 --limit 1|0 --stage none|copy|convert|convert-xfer --interval-us 12500 (or --self-test)")
    }
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    let mut sys = sysinfo::System::new();
    let id = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[id]), true);
    if !sys.process(id).is_some_and(|p| {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        [
            "fortniteclient-win64-shipping.exe",
            "cs2.exe",
            "replayx-gameproxy.exe",
        ]
        .contains(&name.as_str())
    }) || !foreground(pid)
    {
        bail!("The selected game (Fortnite, CS2 or replayx-gameproxy) must already be foreground")
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
            if stage.starts_with("convert") {
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT
            } else {
                D3D11_CREATE_DEVICE_BGRA_SUPPORT
            },
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context): (ID3D11Device, ID3D11DeviceContext) =
            (device.unwrap(), context.unwrap());
        let mut native: Option<Native> = None;
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
            Duration: interval_us as i64 * 10,
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
                    native = None;
                    continue;
                }
                if stage == "none" {
                    frame.Close()?;
                    continue;
                }
                let source: ID3D11Texture2D = frame
                    .Surface()?
                    .cast::<IDirect3DDxgiInterfaceAccess>()?
                    .GetInterface()?;
                if stage.starts_with("convert") && native.is_none() {
                    native = Some(Native::new(
                        &device,
                        &context,
                        (size.Width as u32, size.Height as u32),
                        out_size,
                        stage == "convert-xfer",
                    )?);
                }
                if texture.is_none() && stage == "copy" {
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
                match native.as_mut() {
                    Some(n) => n.convert(&device, &context, &source)?,
                    None => context.CopyResource(texture.as_ref().unwrap(), &source),
                }
                copy_submit_ms.push(t.elapsed().as_secs_f64() * 1000.);
                if sample {
                    context.End(&timer.end);
                    context.End(&timer.disjoint);
                    timer.pending = Some(Instant::now());
                }
                if let Some(n) = native.as_mut() {
                    n.transfer(&context)?;
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
                "stage":stage, "min_update_interval_us":interval_us,
                "native":native.as_ref().map(|n| json!({
                    "output":[n.out.0, n.out.1],
                    "intermediate_copies":n.intermediate_copies,
                    "transfer":n.transfer.as_ref().map(|t| json!({
                        "adapter":t.adapter, "frames_uploaded":t.uploaded,
                        "readbacks_not_ready_in_time":t.not_ready,
                        "upload_cpu":stats(t.upload_cpu_ms.clone()),
                    })),
                })),
                "limits":"Diagnostic only, not REPLAYX recording or game FPS. Latency is from Windows' frame timestamp to this process taking the frame at its sampling slot. The GPU span is the stage's own work (copy, or video-processor pass) and includes scheduling. No encoder or audio; convert-xfer moves NV12 frames through system memory to the NVIDIA GPU (an upper bound for a cross-adapter copy)."
            }))?
        );
        result
    }
}
