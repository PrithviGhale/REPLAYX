//! Isolate Desktop Duplication acquisition/copies/release from NVENC.
//! No image readback, file recording, game input, injection or setting changes.
//! Requires the explicitly selected PID (Fortnite, or REPLAYX's own synthetic
//! fixture replayx-gameproxy) to remain in front throughout.
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    ffi::c_void,
    time::{Duration, Instant},
};
use windows::{
    core::Interface,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{
                CreateDXGIFactory1, IDXGIAdapter, IDXGIFactory1, IDXGIOutput1, IDXGIResource,
                DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
            },
        },
        UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsIconic},
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
        "p95_ms":values[(values.len()-1)*95/100], "p99_ms":values[(values.len()-1)*99/100],
        "max_ms":values.last()})
}

fn foreground(pid: u32) -> bool {
    unsafe {
        let window = GetForegroundWindow();
        let mut actual = 0;
        GetWindowThreadProcessId(window, Some(&mut actual));
        actual == pid && !IsIconic(window).as_bool()
    }
}

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
        // Windows bindings treat S_FALSE as Ok. Nonzero completed timestamps
        // and frequency are required; never spin, Flush or wait for the GPU.
        if ready && start > 0 && end >= start && disjoint.Frequency > 0 {
            self.pending = None;
            return (!disjoint.Disjoint.as_bool())
                .then(|| (end - start) as f64 / disjoint.Frequency as f64 * 1000.);
        }
        if began.elapsed() > Duration::from_secs(2) {
            self.timeouts += 1;
            self.pending = None;
            // Never reuse query objects whose prior GPU work has not finished.
            self.disabled = true;
        }
        None
    }
}

fn main() -> Result<()> {
    let pid = argument("--pid", 0u32)?;
    let seconds = argument("--seconds", 20u64)?;
    let hold = argument("--hold", 0u8)? != 0;
    let copies = argument("--copies", 2usize)?;
    if pid == 0 || !(5..=120).contains(&seconds) || !(1..=2).contains(&copies) {
        bail!("Use --pid EXISTING_GAME_PID --seconds 5..120 --hold 0|1 --copies 1|2")
    }
    // Public process metadata only; never open game memory or send any input.
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
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory.EnumAdapters1(0)?;
        let output: IDXGIOutput1 = adapter.EnumOutputs(0)?.cast()?;
        let mut device = None;
        let mut context = None;
        D3D11CreateDevice(
            &adapter.cast::<IDXGIAdapter>()?,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let (device, context) = (device.unwrap(), context.unwrap());
        let duplication = output
            .DuplicateOutput(&device)
            .context("Desktop Duplication unavailable")?;
        let mut timer = Timer::new(&device)?;
        let (mut acquire_ms, mut submit_ms, mut release_ms, mut copy_gpu_ms) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut textures: Vec<ID3D11Texture2D> = Vec::new();
        let mut geometry = None;
        let mut owned = false;
        let (mut acquired, mut accumulated, mut timeout, mut last_present, mut distinct) =
            (0usize, 0u64, 0usize, 0i64, 0usize);
        let began = Instant::now();
        let result = (|| -> Result<()> {
            let mut slot = 0u32;
            while began.elapsed() < Duration::from_secs(seconds) {
                if !foreground(pid) {
                    bail!("Foreground changed; profile invalidated")
                }
                let next = began + Duration::from_secs_f64(slot as f64 / 60.);
                if let Some(delay) = next.checked_duration_since(Instant::now()) {
                    std::thread::sleep(delay);
                }
                slot += 1;
                if owned {
                    let t = Instant::now();
                    duplication.ReleaseFrame()?;
                    release_ms.push(t.elapsed().as_secs_f64() * 1000.);
                    owned = false;
                }
                let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
                let mut resource: Option<IDXGIResource> = None;
                let t = Instant::now();
                let r = duplication.AcquireNextFrame(16, &mut info, &mut resource);
                acquire_ms.push(t.elapsed().as_secs_f64() * 1000.);
                if let Err(e) = r {
                    if e.code() == DXGI_ERROR_WAIT_TIMEOUT {
                        timeout += 1;
                        continue;
                    }
                    return Err(e.into());
                }
                owned = true;
                acquired += 1;
                accumulated += info.AccumulatedFrames as u64;
                if info.LastPresentTime != 0 && info.LastPresentTime != last_present {
                    distinct += 1;
                    last_present = info.LastPresentTime;
                }
                let source: ID3D11Texture2D = resource.unwrap().cast()?;
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                source.GetDesc(&mut desc);
                let size = (desc.Width, desc.Height, desc.Format.0);
                if geometry.is_some_and(|g| g != size) {
                    bail!("Display geometry changed")
                }
                if textures.is_empty() {
                    geometry = Some(size);
                    desc.Usage = D3D11_USAGE_DEFAULT;
                    desc.BindFlags = 0;
                    desc.CPUAccessFlags = 0;
                    desc.MiscFlags = 0;
                    for _ in 0..copies {
                        let mut texture = None;
                        device.CreateTexture2D(&desc, None, Some(&mut texture))?;
                        textures.push(texture.unwrap());
                    }
                }
                if let Some(ms) = timer.poll(&context) {
                    copy_gpu_ms.push(ms);
                }
                let sample =
                    acquired.is_multiple_of(16) && timer.pending.is_none() && !timer.disabled;
                if sample {
                    context.Begin(&timer.disjoint);
                    context.End(&timer.start);
                }
                let t = Instant::now();
                for texture in &textures {
                    context.CopyResource(texture, &source);
                }
                submit_ms.push(t.elapsed().as_secs_f64() * 1000.);
                if sample {
                    context.End(&timer.end);
                    context.End(&timer.disjoint);
                    timer.pending = Some(Instant::now());
                }
                // This probe has no Present/NVENC call to submit its command
                // buffer. Flush submits without waiting; both modes use it.
                context.Flush();
                drop(source);
                if !hold {
                    let t = Instant::now();
                    duplication.ReleaseFrame()?;
                    release_ms.push(t.elapsed().as_secs_f64() * 1000.);
                    owned = false;
                }
            }
            Ok(())
        })();
        if owned {
            let _ = duplication.ReleaseFrame();
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "success":result.is_ok(), "error":result.as_ref().err().map(ToString::to_string),
                "hold_until_next_acquire":hold, "copies_per_acquisition":copies,
                "seconds":began.elapsed().as_secs_f64(), "geometry":geometry,
                "acquisitions":acquired, "distinct_last_present_timestamps":distinct,
                "accumulated_desktop_updates":accumulated, "acquire_timeouts":timeout,
                "cpu_acquire_wall":stats(acquire_ms), "cpu_copy_submission":stats(submit_ms),
                "cpu_release_wall":stats(release_ms), "gpu_copy_span":stats(copy_gpu_ms),
                "gpu_query_timeouts":timer.timeouts,
                "explicit_nonblocking_flush_each_frame":true,
                "limits":"Diagnostic only, not REPLAYX recording or game FPS. CPU waits differ from GPU elapsed query spans; spans include scheduling/preemption. Skipped desktop updates are expected at 60 acquisition FPS and are not recorder drops. No pixel readback, NVENC, audio, source-FPS or encoder-latency measurement."
            }))?
        );
        result
    }
}
