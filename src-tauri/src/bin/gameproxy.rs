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
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            w!("REPLAYX game proxy - Escape closes"),
            WS_POPUP | WS_VISIBLE,
            0,
            0,
            sw,
            sh,
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
        let flags = if tearing_ok {
            DXGI_SWAP_CHAIN_FLAG_ALLOW_TEARING.0 as u32
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
                BufferCount: 3,
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

        let start = Instant::now();
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
                pad: 0.,
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
            context.Draw(3, 0);
            let present_flags = if tearing_ok {
                DXGI_PRESENT_ALLOW_TEARING
            } else {
                DXGI_PRESENT(0)
            };
            let _ = swap.Present(0, present_flags);
            presents.push(start.elapsed().as_secs_f64());
        }
        let _ = DestroyWindow(hwnd);
        // QPC-based wall time of the start, so other tools can align windows.
        let mut qpc = 0i64;
        let mut freq = 0i64;
        let _ = windows::Win32::System::Performance::QueryPerformanceCounter(&mut qpc);
        let _ = windows::Win32::System::Performance::QueryPerformanceFrequency(&mut freq);
        let now = qpc as f64 / freq as f64;
        let start_qpc = now - start.elapsed().as_secs_f64();
        let report = serde_json::json!({
            "adapter": name, "width": sw, "height": sh, "iters": iters, "cpu_us": cpu_us,
            "tearing": tearing_ok, "start_qpc_s": start_qpc, "presents_s": presents,
        });
        std::fs::write(&log, serde_json::to_vec(&report).unwrap()).ok();
        println!(
            "{} frames on {name}, tearing {tearing_ok}, wrote {log}",
            presents.len()
        );
    }
    Ok(())
}
