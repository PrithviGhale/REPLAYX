//! A silent, game-like GPU workload for measuring REPLAYX's overhead and the
//! smoothness of what it records, without touching a real game.
//!
//! Renders a heavy pixel shader, uncapped, on the high-performance GPU (the
//! RTX on hybrid laptops) into a borderless full-screen window on the main
//! display, so frames take the same path as a game's: rendered on the
//! discrete GPU, presented through the display GPU. Every frame carries its
//! render time as a 24-bit code (0.25 ms units) in the top-left blocks, at the
//! same positions as scripts/motion-fixture.py, so recordings can be analysed
//! frame by frame (scripts/gameproxy-check.py). Its own frame times are
//! written to --log (FPS, 1% and 0.1% lows).
//!
//! usage: replayx-gameproxy --seconds 30 --log out.json [--iters 200] [--cpu-us 0]
//! Escape closes it early. No audio.
use std::{
    ffi::c_void,
    time::{Duration, Instant},
};
use windows::{
    core::{s, w, Interface, PCSTR},
    Win32::{
        Foundation::{HMODULE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::{
            Direct3D::{
                Fxc::D3DCompile, ID3DBlob, D3D_DRIVER_TYPE_UNKNOWN,
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            },
            Direct3D11::*,
            Dxgi::{Common::*, *},
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            WindowsAndMessaging::*,
        },
    },
};

const SHADER: &str = r#"
cbuffer Frame : register(b0) { uint code; uint iters; float t; float pad; float2 res; float2 pad2; };
float4 vs(uint id : SV_VertexID) : SV_Position {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 ps(float4 pos : SV_Position) : SV_Target {
    float2 q = pos.xy / res * float2(320, 180);
    // 24-bit render-time code, same cells as the desktop motion fixture.
    if (q.y >= 7 && q.y < 15 && q.x >= 8 && q.x < 8 + 24 * 12) {
        uint i = (uint)((q.x - 8) / 12);
        if (q.x - 8 - i * 12 < 8) return ((code >> i) & 1) ? float4(1, 1, 1, 1) : float4(0, 0, 0, 1);
    }
    // Heavy, motion-rich background: moving bars plus shader work.
    float a = 0;
    for (uint k = 0; k < iters; k++) a += sin(q.x * 0.013 + k * 0.7 + t) * cos(q.y * 0.017 - k * 0.3);
    float bar = frac((q.x - t * 90) / 64) < 0.25 ? 1 : 0;
    float3 c = float3(0.06, 0.09, 0.13) + bar * float3(0.8, 0.5, 0.15) + 0.02 * a / max(iters, 1);
    // Optional moving fine detail exercises encoder complexity/bandwidth, unlike
    // the original large flat bars. This is still a synthetic workload.
    if (pad > 0) {
        float2 uv = floor(pos.xy + float2(t * 120, t * 40));
        uint h = (uint(uv.x) * 1664525u) ^ (uint(uv.y) * 1013904223u);
        h ^= h >> 16; h *= 2246822519u; h ^= h >> 13;
        float foliage = float(h & 255u) / 255.0;
        c = lerp(c, float3(foliage * 0.5, foliage, foliage * 0.25), 0.6);
    }
    return float4(c, 1);
}
"#;

#[repr(C)]
struct Constants {
    code: u32,
    iters: u32,
    t: f32,
    pad: f32,
    res: [f32; 2],
    pad2: [f32; 2],
}

/// One bounded, non-blocking timestamp sample in flight. No Flush or GPU wait.
struct GpuTimer {
    disjoint: ID3D11Query,
    begin: ID3D11Query,
    end: ID3D11Query,
    pending: Option<(Instant, f64)>,
}
impl GpuTimer {
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
            begin: query(D3D11_QUERY_TIMESTAMP)?,
            end: query(D3D11_QUERY_TIMESTAMP)?,
            pending: None,
        })
    }
    unsafe fn poll(&mut self, context: &ID3D11DeviceContext) -> Option<(f64, f64)> {
        let (began, t) = self.pending?;
        let mut disjoint = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
        let (mut a, mut b) = (0u64, 0u64);
        let flags = D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32;
        let ok = context
            .GetData(
                &self.disjoint,
                Some(&mut disjoint as *mut _ as *mut c_void),
                std::mem::size_of_val(&disjoint) as u32,
                flags,
            )
            .is_ok()
            && context
                .GetData(&self.begin, Some(&mut a as *mut _ as *mut c_void), 8, flags)
                .is_ok()
            && context
                .GetData(&self.end, Some(&mut b as *mut _ as *mut c_void), 8, flags)
                .is_ok();
        // GetData maps S_FALSE to success in the Windows bindings; unchanged
        // zero output indicates the asynchronous result is not available yet.
        if ok && b > a && a > 0 && disjoint.Frequency > 0 {
            self.pending = None;
            if !disjoint.Disjoint.as_bool() {
                return Some((t, (b - a) as f64 / disjoint.Frequency as f64 * 1000.));
            }
        } else if began.elapsed() > Duration::from_secs(2) {
            self.pending = None;
        }
        None
    }
}

unsafe extern "system" fn proc_(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_KEYDOWN && wp.0 == 0x1B {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

fn compile(entry: PCSTR, target: PCSTR) -> windows::core::Result<ID3DBlob> {
    let mut code = None;
    let mut errors = None;
    let result = unsafe {
        D3DCompile(
            SHADER.as_ptr() as *const c_void,
            SHADER.len(),
            s!("gameproxy"),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    if let Err(e) = result {
        if let Some(errors) = errors {
            let text = unsafe {
                std::slice::from_raw_parts(
                    errors.GetBufferPointer() as *const u8,
                    errors.GetBufferSize(),
                )
            };
            eprintln!("{}", String::from_utf8_lossy(text));
        }
        return Err(e);
    }
    Ok(code.unwrap())
}

fn blob(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize()) }
}

fn arg<T: std::str::FromStr>(name: &str, default: T) -> T {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() -> windows::core::Result<()> {
    let seconds: f64 = arg("--seconds", 30.0);
    let iters: u32 = arg("--iters", 200);
    let cpu_us: u64 = arg("--cpu-us", 0);
    // Fixed CPU *work* per frame (not time): lower CPU clocks lower FPS, as
    // in a CPU-bound game. Plus background threads computing continuously,
    // like an engine's worker threads, which load the CPU package.
    let cpu_work: u64 = arg("--cpu-work", 0);
    let workers: u32 = arg("--workers", 0);
    let stop_workers = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let worker_threads: Vec<_> = (0..workers)
        .map(|i| {
            let stop = stop_workers.clone();
            std::thread::spawn(move || {
                let mut x = 0x9E37_79B9_7F4A_7C15u64 ^ i as u64;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    for _ in 0..100_000 {
                        x = x
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                    }
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    let detail: u32 = arg("--detail", 0);
    let log: String = arg("--log", "gameproxy.json".to_string());
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("ReplayxGameProxy");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        // Borderless full screen (the default, like most games' "fullscreen"),
        // or a 1280x720 client area in a normal window (--windowed 1).
        let windowed = arg("--windowed", 0u32) != 0;
        let (sw, sh) = if windowed {
            (1280, 720)
        } else {
            (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
        };
        let style = if windowed {
            WS_OVERLAPPEDWINDOW | WS_VISIBLE
        } else {
            WS_POPUP | WS_VISIBLE
        };
        let mut outer = windows::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: sw,
            bottom: sh,
        };
        if windowed {
            AdjustWindowRect(&mut outer, style, false)?;
        }
        let origin = if windowed { 100 } else { 0 };
        // --topmost 1 keeps the fixture above other windows when Windows'
        // focus-stealing protection leaves another app in the foreground.
        let topmost = arg("--topmost", 0u32) != 0;
        let hwnd = CreateWindowExW(
            if topmost {
                WS_EX_TOPMOST
            } else {
                WINDOW_EX_STYLE(0)
            },
            class,
            w!("REPLAYX game proxy - Escape closes"),
            style,
            origin,
            origin,
            outer.right - outer.left,
            outer.bottom - outer.top,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let _ = SetForegroundWindow(hwnd);
        let _ = ShowCursor(false);

        // The high-performance adapter, as a game on a hybrid laptop would use.
        let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
        let adapter: IDXGIAdapter1 =
            factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)?;
        let desc = adapter.GetDesc1()?;
        let name = String::from_utf16_lossy(&desc.Description)
            .trim_end_matches('\0')
            .to_string();
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
        let (device, context): (ID3D11Device, ID3D11DeviceContext) =
            (device.unwrap(), context.unwrap());
        if arg("--texture-probe", 0u32) != 0 {
            // Isolate hardware support from FFmpeg's scaler implementation.
            let mut results = Vec::new();
            for array_size in [1, 10] {
                for flags in [
                    D3D11_BIND_RENDER_TARGET.0,
                    D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_VIDEO_ENCODER.0,
                ] {
                    let mut texture = None;
                    let result = device.CreateTexture2D(
                        &D3D11_TEXTURE2D_DESC {
                            Width: 1920,
                            Height: 1080,
                            MipLevels: 1,
                            ArraySize: array_size,
                            Format: DXGI_FORMAT_NV12,
                            SampleDesc: DXGI_SAMPLE_DESC {
                                Count: 1,
                                Quality: 0,
                            },
                            Usage: D3D11_USAGE_DEFAULT,
                            BindFlags: flags as u32,
                            ..Default::default()
                        },
                        None,
                        Some(&mut texture),
                    );
                    results.push(serde_json::json!({"array_size":array_size,"bind_flags":flags,
                        "success":result.is_ok(),"error":result.err().map(|e|format!("{:?}", e.code()))}));
                }
            }
            std::fs::write(
                &log,
                serde_json::to_vec_pretty(&serde_json::json!({"adapter":name,"textures":results}))
                    .unwrap(),
            )
            .ok();
            let _ = DestroyWindow(hwnd);
            return Ok(());
        }
        // One frame in flight, so the time stamped into a frame is close to
        // when it reaches the screen (a deep render-ahead queue would make the
        // stamp drift against display time and look like capture judder).
        device.cast::<IDXGIDevice1>()?.SetMaximumFrameLatency(1)?;
        let mut tearing = windows::core::BOOL(0);
        let tearing_ok = factory
            .cast::<IDXGIFactory5>()?
            .CheckFeatureSupport(
                DXGI_FEATURE_PRESENT_ALLOW_TEARING,
                &mut tearing as *mut _ as *mut c_void,
                std::mem::size_of_val(&tearing) as u32,
            )
            .is_ok()
            && tearing.as_bool();
        // --waitable 1: pace each frame on the swapchain's frame-latency
        // waitable object (latency 1), as engines with low-latency modes do.
        // Its signal depends on how Windows presents the frame (direct flip
        // or DWM composition), so it shows whether capture-forced composition
        // caps such games.
        let waitable = arg("--waitable", 0u32) != 0;
        let flags = (if tearing_ok {
            DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32
        } else {
            0
        }) | if waitable {
            DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32
        } else {
            0
        };
        let swap = factory.CreateSwapChainForHwnd(
            &device,
            hwnd,
            &DXGI_SWAP_CHAIN_DESC1 {
                Width: sw as u32,
                Height: sh as u32,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                // --buffers 2: a two-buffer flip chain, as some engines use.
                BufferCount: arg("--buffers", 3u32).clamp(2, 4),
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: flags,
                ..Default::default()
            },
            None,
            None,
        )?;
        let back: ID3D11Texture2D = swap.GetBuffer(0)?;
        let mut rtv = None;
        device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
        let vs_blob = compile(s!("vs"), s!("vs_5_0"))?;
        let ps_blob = compile(s!("ps"), s!("ps_5_0"))?;
        let (mut vs, mut ps) = (None, None);
        device.CreateVertexShader(blob(&vs_blob), None, Some(&mut vs))?;
        device.CreatePixelShader(blob(&ps_blob), None, Some(&mut ps))?;
        let mut cb = None;
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: std::mem::size_of::<Constants>() as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                ..Default::default()
            },
            None,
            Some(&mut cb),
        )?;
        let cb = cb.unwrap();
        let mut gpu_timer = GpuTimer::new(&device).ok();
        let mut gpu_times = Vec::new();
        // How Windows actually showed our frames (no ETW needed): composed by
        // DWM, an overlay/MPO plane, or presented directly (independent flip).
        // A capture tool that forces composition costs a game GPU time and
        // latency that a direct-flip-only measurement would not show.
        let media = swap.cast::<IDXGISwapChainMedia>().ok();
        let mut presentation = Vec::new();

        let start = Instant::now();
        let latency_wait = if waitable {
            let swap2 = swap.cast::<IDXGISwapChain2>()?;
            swap2.SetMaximumFrameLatency(1)?;
            Some(swap2.GetFrameLatencyWaitableObject())
        } else {
            None
        };
        let mut presents: Vec<f64> = Vec::with_capacity(200_000);
        let mut msg = MSG::default();
        'run: while start.elapsed().as_secs_f64() < seconds {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'run;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if let Some(handle) = latency_wait {
                let _ =
                    windows::Win32::System::Threading::WaitForSingleObjectEx(handle, 1000, true);
            }
            if cpu_work > 0 {
                let mut x = 0x2545_F491_4F6C_DD1Du64;
                for _ in 0..cpu_work {
                    x = x
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                }
                std::hint::black_box(x);
            }
            if cpu_us > 0 {
                // Simulated game logic on the main thread.
                let until = Instant::now() + Duration::from_micros(cpu_us);
                while Instant::now() < until {
                    std::hint::spin_loop();
                }
            }
            let t = start.elapsed().as_secs_f64();
            let constants = Constants {
                code: ((t * 4000.0) as u32) & 0xFF_FFFF,
                iters,
                t: t as f32,
                pad: detail as f32,
                res: [sw as f32, sh as f32],
                pad2: [0.; 2],
            };
            context.UpdateSubresource(&cb, 0, None, &constants as *const _ as *const c_void, 0, 0);
            context.OMSetRenderTargets(Some(&[rtv.clone()]), None);
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: sw as f32,
                Height: sh as f32,
                MaxDepth: 1.,
                ..Default::default()
            }]));
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.VSSetShader(vs.as_ref(), None);
            context.PSSetShader(ps.as_ref(), None);
            context.PSSetConstantBuffers(0, Some(&[Some(cb.clone())]));
            if let Some(timer) = &mut gpu_timer {
                if let Some((t, ms)) = timer.poll(&context) {
                    gpu_times.push(serde_json::json!({"t":t,"ms":ms}));
                }
            }
            let sampled = presents.len().is_multiple_of(64)
                && gpu_timer
                    .as_ref()
                    .is_some_and(|timer| timer.pending.is_none());
            if sampled {
                let timer = gpu_timer.as_mut().unwrap();
                context.Begin(&timer.disjoint);
                context.End(&timer.begin);
                timer.pending = Some((Instant::now(), t));
            }
            context.Draw(3, 0);
            if sampled {
                let timer = gpu_timer.as_ref().unwrap();
                context.End(&timer.end);
                context.End(&timer.disjoint);
            }
            let present_flags = if tearing_ok {
                DXGI_PRESENT_ALLOW_TEARING
            } else {
                DXGI_PRESENT(0)
            };
            let _ = swap.Present(0, present_flags);
            presents.push(start.elapsed().as_secs_f64());
            if presents.len().is_multiple_of(32) {
                if let Some(media) = &media {
                    let mut stats = DXGI_FRAME_STATISTICS_MEDIA::default();
                    if media.GetFrameStatisticsMedia(&mut stats).is_ok() {
                        presentation
                            .push(serde_json::json!({"t":t,"mode":stats.CompositionMode.0}));
                    }
                }
            }
        }
        let _ = DestroyWindow(hwnd);
        // QPC-based wall time of the start, so other tools can align windows.
        let mut qpc = 0i64;
        let mut freq = 0i64;
        let _ = windows::Win32::System::Performance::QueryPerformanceCounter(&mut qpc);
        let _ = windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq);
        let now = qpc as f64 / freq as f64;
        let start_qpc = now - start.elapsed().as_secs_f64();
        stop_workers.store(true, std::sync::atomic::Ordering::Relaxed);
        for t in worker_threads {
            let _ = t.join();
        }
        let report = serde_json::json!({
            "adapter": name, "width": sw, "height": sh, "windowed": windowed, "topmost": topmost, "waitable": waitable, "cpu_work": cpu_work, "workers": workers, "buffers": arg("--buffers", 3u32).clamp(2, 4), "iters": iters, "cpu_us": cpu_us, "detail": detail,
            "tearing": tearing_ok, "start_qpc_s": start_qpc, "presents_s": presents,
            "fixture_draw_gpu_ms":gpu_times,
            // DXGI_FRAME_PRESENTATION_MODE: 0 composed, 1 overlay, 2 none
            // (direct/independent flip), 3 composition failure.
            "presentation_modes": presentation,
        });
        std::fs::write(&log, serde_json::to_vec(&report).unwrap()).ok();
        println!(
            "{} frames on {name}, tearing {tearing_ok}, wrote {log}",
            presents.len()
        );
    }
    Ok(())
}
