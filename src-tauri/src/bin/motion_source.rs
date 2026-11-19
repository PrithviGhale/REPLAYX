//! Lightweight WINDOWED presentation-aware frame-ID source, never a benchmark.
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    time::{Duration, Instant},
};
use windows::{
    core::{w, Interface},
    Win32::{
        Foundation::{HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
            Direct3D11::*,
            Dxgi::{Common::*, *},
            Gdi::ClientToScreen,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Performance::{QueryPerformanceCounter, QueryPerformanceFrequency},
            Threading::WaitForSingleObject,
        },
        UI::{
            HiDpi::{SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2},
            WindowsAndMessaging::*,
        },
    },
};
#[path = "../native_worker/common.rs"]
mod common;
fn arg<T: std::str::FromStr>(key: &str, default: T) -> T {
    let a: Vec<_> = std::env::args().collect();
    a.iter()
        .position(|v| v == key)
        .and_then(|i| a.get(i + 1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}
unsafe extern "system" fn proc_(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match m {
        WM_CLOSE => {
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, m, w, l),
    }
}
fn qpc() -> i64 {
    unsafe {
        let (mut n, mut f) = (0, 0);
        let _ = QueryPerformanceCounter(&mut n);
        let _ = QueryPerformanceFrequency(&mut f);
        (n as i128 * 10_000_000 / f as i128) as i64
    }
}
fn write(out: &mut impl Write, value: serde_json::Value) -> Result<()> {
    serde_json::to_writer(&mut *out, &value)?;
    out.write_all(b"\n")?;
    Ok(())
}
fn main() -> Result<()> {
    let fps = arg("--fps-num", arg("--fps", 60u32));
    let fps_den = arg("--fps-den", 1u32);
    let seconds = arg("--seconds", 12u64);
    let pause_ms = arg("--pause-ms", 0u64);
    let pause_frame = arg("--pause-frame", 180u64);
    let window_left = arg("--left", 32i32);
    let window_top = arg("--top", 32i32);
    let log = arg("--log", String::new());
    let static_id = arg("--static", 0u32) != 0;
    let maximized = std::env::args().any(|a| a == "--maximized");
    let foreground = std::env::args().any(|a| a == "--foreground");
    let approved_game_cover = std::env::args().any(|a| a == "--approved-game-cover");
    if approved_game_cover && !foreground {
        bail!("An approved game-cover test must use its own foreground window")
    }
    let stop_path = arg("--stop-file", String::new());
    if pause_ms > 500
        || fps_den == 0
        || fps < fps_den
        || fps as u64 > 500 * fps_den as u64
        || !(1..=120).contains(&seconds)
        || log.is_empty()
    {
        bail!("Use --fps 1..500 --seconds 1..120 --log NEW_FILE; windowed only")
    }
    if !approved_game_cover
        && matches!(
            replayx_core::games::sample(&[]),
            replayx_core::games::Foreground::Game(_)
        )
    {
        bail!("A game is foreground; the windowed source was not opened")
    }
    let mut out = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(&log)?);
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("ReplayxMotionSource");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        });
        let style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 960,
            bottom: 540,
        };
        AdjustWindowRect(&mut rect, style, false)?;
        let hwnd = CreateWindowExW(
            if foreground {
                WS_EX_TOPMOST
            } else {
                WS_EX_TOPMOST | WS_EX_NOACTIVATE
            },
            class,
            w!("REPLAYX silent frame-ID source (windowed)"),
            style,
            window_left,
            window_top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        if maximized {
            // An ordinary bordered maximized window, never exclusive fullscreen.
            // Cover unrelated UI so the test has one known presentation clock.
            let _ = ShowWindow(hwnd, SW_SHOWMAXIMIZED);
        }
        if foreground {
            let _ = SetForegroundWindow(hwnd);
            if GetForegroundWindow() != hwnd {
                bail!("Controlled source did not become foreground; no foreground-test claim")
            }
        }
        let mut client = RECT::default();
        GetClientRect(hwnd, &mut client)?;
        let client_width = (client.right - client.left) as u32;
        let client_height = (client.bottom - client.top) as u32;
        if client_width < 960 || client_height < 540 {
            bail!("Controlled source needs a client area of at least 960x540")
        }
        let mut point = POINT::default();
        let _ = ClientToScreen(hwnd, &mut point);
        let factory: IDXGIFactory2 = CreateDXGIFactory1()?;
        let (adapter, name) = common::adapter_by_vendor(
            replayx_core::geometry::display(0).map_or(0x10de, |d| d.adapter_vendor),
        )
        .or_else(|| common::adapter_by_vendor(0x1002))
        .or_else(|| common::adapter_by_vendor(0x8086))
        .context("No fixture GPU")?;
        let (mut device, mut context) = (None, None);
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )?;
        let device: ID3D11Device = device.unwrap();
        let context: ID3D11DeviceContext = context.unwrap();
        let c1: ID3D11DeviceContext1 = context.cast()?;
        let swap = factory.CreateSwapChainForHwnd(
            &device,
            hwnd,
            &DXGI_SWAP_CHAIN_DESC1 {
                Width: client_width,
                Height: client_height,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT.0 as u32,
                ..Default::default()
            },
            None,
            None,
        )?;
        let swap2: IDXGISwapChain2 = swap.cast()?;
        swap2.SetMaximumFrameLatency(1)?;
        let wait = swap2.GetFrameLatencyWaitableObject();
        let media = swap.cast::<IDXGISwapChainMedia>().ok();
        let texture: ID3D11Texture2D = swap.GetBuffer(0)?;
        let mut view = None;
        device.CreateRenderTargetView(&texture, None, Some(&mut view))?;
        let view = view.unwrap();
        let start = Instant::now();
        let origin = qpc();
        let mut frequency = 0;
        QueryPerformanceFrequency(&mut frequency)?;
        write(
            &mut out,
            json!({"stage":"source_start","qpc_hns":origin,"fps":fps as f64/fps_den as f64,"fps_num":fps,"fps_den":fps_den,"adapter":name,"left":point.x,"top":point.y,"width":960,"height":540,"windowed":true,"maximized_window":maximized,"foreground_test":foreground,"approved_game_cover":approved_game_cover,"client_width":client_width,"client_height":client_height,"static":static_id,"qpc_frequency":frequency}),
        )?;
        out.flush()?;
        let mut n = 0u64;
        let mut schedule_shift = Duration::ZERO;
        let mut stop_check = Instant::now();
        let sample_stats = |out: &mut BufWriter<std::fs::File>| -> Result<()> {
            if let Some(media) = &media {
                let mut st = DXGI_FRAME_STATISTICS_MEDIA::default();
                if media.GetFrameStatisticsMedia(&mut st).is_ok() {
                    write(
                        out,
                        json!({"stage":"presentation_stats","qpc_hns":qpc(),"present_count":st.PresentCount,"present_refresh":st.PresentRefreshCount,"sync_refresh":st.SyncRefreshCount,"sync_qpc_hns":(st.SyncQPCTime as i128*10_000_000/frequency as i128)as i64,"mode":st.CompositionMode.0}),
                    )?;
                }
            }
            Ok(())
        };
        let mut msg = MSG::default();
        'run: while start.elapsed() < Duration::from_secs(seconds) {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_QUIT {
                    break 'run;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if stop_check.elapsed() >= Duration::from_millis(250) {
                stop_check = Instant::now();
                if !stop_path.is_empty() && std::path::Path::new(&stop_path).exists() {
                    break;
                }
            }
            if pause_ms > 0 && n == pause_frame {
                write(
                    &mut out,
                    json!({"stage":"intentional_source_pause","qpc_hns":qpc(),"before_frame":n,"milliseconds":pause_ms}),
                )?;
                std::thread::sleep(Duration::from_millis(pause_ms));
                // A true source pause, not a catch-up burst of late presents.
                schedule_shift += Duration::from_millis(pause_ms);
            }
            let due = start
                + schedule_shift
                + Duration::from_nanos(n * 1_000_000_000 * fps_den as u64 / fps as u64);
            while let Some(delay) = due.checked_duration_since(Instant::now()) {
                std::thread::sleep(delay.min(Duration::from_millis(2)));
                sample_stats(&mut out)?;
            }
            if WaitForSingleObject(wait, 1000) != windows::Win32::Foundation::WAIT_OBJECT_0 {
                bail!("Fixture presentation wait timed out")
            }
            let code = if static_id { 42 } else { n as u32 };
            let submit = qpc();
            context.ClearRenderTargetView(&view, &[0.0627, 0.0941, 0.1255, 1.]);
            for i in 0..24 {
                let v = if code & (1 << i) != 0 { 1. } else { 0. };
                c1.ClearView(
                    &view,
                    &[v, v, v, 1.],
                    Some(&[RECT {
                        left: (8 + i * 12) * 3,
                        top: 7 * 3,
                        right: (16 + i * 12) * 3,
                        bottom: 15 * 3,
                    }]),
                );
            }
            for b in 0..8 {
                let x = (((if static_id { 0 } else { n }) as i32 * (3 + b)) % 1035) - 75;
                let rect = RECT {
                    left: x.max(0),
                    top: (25 + b * 13) * 3,
                    right: (x + 75).clamp(0, 960),
                    bottom: (34 + b * 13) * 3,
                };
                if rect.right > rect.left {
                    c1.ClearView(&view, &[0.8, 0.3 + b as f32 * 0.04, 0.1, 1.], Some(&[rect]));
                }
            }
            swap.Present(0, DXGI_PRESENT(0)).ok()?;
            let returned = qpc();
            let count = swap.GetLastPresentCount()?;
            write(
                &mut out,
                json!({"stage":"source_present","id":code,"sequence":n,"planned_hns":origin+(schedule_shift.as_nanos()/100)as i64+(n as i128*10_000_000*fps_den as i128/fps as i128)as i64,"submit_hns":submit,"returned_hns":returned,"present_count":count}),
            )?;
            if let Some(media) = &media {
                let mut stats = DXGI_FRAME_STATISTICS_MEDIA::default();
                if media.GetFrameStatisticsMedia(&mut stats).is_ok() {
                    write(
                        &mut out,
                        json!({"stage":"presentation_stats","qpc_hns":qpc(),"present_count":stats.PresentCount,"present_refresh":stats.PresentRefreshCount,"sync_refresh":stats.SyncRefreshCount,"sync_qpc_hns":(stats.SyncQPCTime as i128*10_000_000/frequency as i128)as i64,"mode":stats.CompositionMode.0}),
                    )?;
                }
            }
            n += 1;
            if n.is_multiple_of(60) {
                out.flush()?;
            }
        }
        write(
            &mut out,
            json!({"stage":"source_end","qpc_hns":qpc(),"frames_submitted":n}),
        )?;
        out.flush()?;
        let _ = windows::Win32::Foundation::CloseHandle(wait);
        let _ = DestroyWindow(hwnd);
    }
    Ok(())
}
