//! Use the same default DXGI adapter/output numbering as FFmpeg ddagrab.
//! Desktop coordinates are physical pixels, not DPI-scaled window coordinates.
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use windows::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::{
    core::PCWSTR,
    Win32::Graphics::{
        Dxgi::{CreateDXGIFactory1, IDXGIFactory1},
        Gdi::{
            EnumDisplaySettingsExW, DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_FLAGS,
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl CaptureRect {
    pub fn validate(&self, width: u32, height: u32) -> Result<()> {
        if self.width < 16
            || self.height < 16
            || self.x.checked_add(self.width).is_none_or(|x| x > width)
            || self.y.checked_add(self.height).is_none_or(|y| y > height)
        {
            bail!("Capture rectangle is outside the active display ({width}×{height}). Re-select the game region after changing resolution.")
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayGeometry {
    pub index: u32,
    pub device: String,
    pub adapter_luid: String,
    pub left: i32,
    pub top: i32,
    pub desktop_width: u32,
    pub desktop_height: u32,
    pub active_width: u32,
    pub active_height: u32,
    pub rotation: i32,
    pub refresh_hz: u32,
}
impl DisplayGeometry {
    pub fn region(&self, requested: Option<CaptureRect>) -> Result<CaptureRect> {
        // Reject rotated outputs until rotation is explicitly handled in the capture graph.
        if self.rotation != 1 {
            bail!("Rotated display capture is not supported by this geometry path")
        }
        let width = self.desktop_width.min(self.active_width);
        let height = self.desktop_height.min(self.active_height);
        let rect = requested.unwrap_or(CaptureRect {
            x: 0,
            y: 0,
            width,
            height,
        });
        rect.validate(width, height)?;
        Ok(rect)
    }
    pub fn same_display(&self, other: &Self) -> bool {
        self.device == other.device && self.adapter_luid == other.adapter_luid
    }
}
pub fn display(index: u32) -> Result<DisplayGeometry> {
    // GetDesc virtualizes desktop coordinates for DPI-unaware threads. Keep
    // capture geometry in physical pixels even in the CLI/background workers.
    let _dpi = PhysicalPixels::enter()?;
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1()?;
        let adapter = factory.EnumAdapters1(0)?;
        let adapter_desc = adapter.GetDesc1()?;
        let output = adapter
            .EnumOutputs(index)
            .context("Selected display disconnected or unavailable on the default DXGI adapter")?;
        let desc = output.GetDesc()?;
        if !desc.AttachedToDesktop.as_bool() {
            bail!("Selected display is not attached to the desktop")
        }
        let len = desc
            .DeviceName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(desc.DeviceName.len());
        let device = String::from_utf16_lossy(&desc.DeviceName[..len]);
        let mut mode = DEVMODEW {
            dmSize: std::mem::size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        if !EnumDisplaySettingsExW(
            PCWSTR(desc.DeviceName.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut mode,
            ENUM_DISPLAY_SETTINGS_FLAGS(0),
        )
        .as_bool()
        {
            bail!("Windows could not report the active display mode")
        }
        let rect = desc.DesktopCoordinates;
        let width = u32::try_from(rect.right - rect.left).context("Invalid desktop width")?;
        let height = u32::try_from(rect.bottom - rect.top).context("Invalid desktop height")?;
        let result = DisplayGeometry {
            index,
            device,
            adapter_luid: format!(
                "{}:{}",
                adapter_desc.AdapterLuid.HighPart, adapter_desc.AdapterLuid.LowPart
            ),
            left: rect.left,
            top: rect.top,
            desktop_width: width,
            desktop_height: height,
            active_width: mode.dmPelsWidth,
            active_height: mode.dmPelsHeight,
            rotation: desc.Rotation.0,
            refresh_hz: mode.dmDisplayFrequency,
        };
        result.region(None)?;
        Ok(result)
    }
}
struct PhysicalPixels(DPI_AWARENESS_CONTEXT);
impl PhysicalPixels {
    fn enter() -> Result<Self> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.0.is_null() {
            bail!(
                "Could not query display geometry in physical pixels: {}",
                std::io::Error::last_os_error()
            )
        }
        Ok(Self(previous))
    }
}
impl Drop for PhysicalPixels {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}
pub fn displays() -> Vec<DisplayGeometry> {
    (0..16).filter_map(|index| display(index).ok()).collect()
}
pub fn canvas(height: u32) -> (u32, u32) {
    let width = match height {
        480 => 854,
        720 => 1280,
        1080 => 1920,
        1440 => 2560,
        2160 => 3840,
        _ => 0,
    };
    (width, height)
}
pub fn scale_filter(height: u32, mode: &str, gpu: bool) -> String {
    let (w, h) = canvas(height);
    if mode == "stretch" {
        if gpu {
            format!("scale_d3d11=width={w}:height={h}:format=nv12,setsar=1")
        } else {
            format!("scale={w}:{h}:flags=bilinear,setsar=1,format=yuv420p")
        }
    } else if gpu {
        format!("scale_d3d11=width=trunc(iw*{h}/ih/2)*2:height={h}:format=nv12,setsar=1")
    } else {
        format!("scale=-2:{h}:flags=bilinear,setsar=1,format=yuv420p")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_query_scope_restores_thread_dpi_context() {
        use windows::Win32::UI::HiDpi::{
            AreDpiAwarenessContextsEqual, GetThreadDpiAwarenessContext,
        };
        unsafe {
            let before = GetThreadDpiAwarenessContext();
            {
                let _scope = PhysicalPixels::enter().unwrap();
                assert!(AreDpiAwarenessContextsEqual(
                    GetThreadDpiAwarenessContext(),
                    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2
                )
                .as_bool());
            }
            assert!(AreDpiAwarenessContextsEqual(GetThreadDpiAwarenessContext(), before).as_bool());
        }
    }
    fn geom(w: u32, h: u32) -> DisplayGeometry {
        DisplayGeometry {
            index: 0,
            device: "DISPLAY1".into(),
            adapter_luid: "0:1".into(),
            left: -1280,
            top: 0,
            desktop_width: w,
            desktop_height: h,
            active_width: w,
            active_height: h,
            rotation: 1,
            refresh_hz: 144,
        }
    }
    #[test]
    fn stretched_canvases_are_fixed() {
        for (w, h) in [(1280, 960), (1440, 1080)] {
            assert_eq!(
                geom(w, h).region(None).unwrap(),
                CaptureRect {
                    x: 0,
                    y: 0,
                    width: w,
                    height: h
                }
            );
            assert_eq!(
                scale_filter(1080, "stretch", false),
                "scale=1920:1080:flags=bilinear,setsar=1,format=yuv420p"
            );
            assert!(scale_filter(1080, "stretch", true).contains("width=1920:height=1080"));
        }
    }
    #[test]
    fn active_mode_excludes_stale_desktop_extent() {
        let mut g = geom(2560, 1440);
        g.active_width = 1280;
        g.active_height = 960;
        assert_eq!(g.region(None).unwrap().width, 1280);
        assert_eq!(g.region(None).unwrap().height, 960);
    }
    #[test]
    fn offset_is_output_local_even_on_negative_monitor() {
        let g = geom(1920, 1080);
        assert_eq!(
            g.region(Some(CaptureRect {
                x: 320,
                y: 60,
                width: 1280,
                height: 960
            }))
            .unwrap()
            .x,
            320
        );
    }
    #[test]
    fn invalid_region_and_overflow_fail() {
        for r in [
            CaptureRect {
                x: 1280,
                y: 0,
                width: 1280,
                height: 960,
            },
            CaptureRect {
                x: u32::MAX,
                y: 0,
                width: 1280,
                height: 960,
            },
            CaptureRect {
                x: 0,
                y: 0,
                width: 0,
                height: 960,
            },
        ] {
            assert!(r.validate(1920, 1080).is_err());
        }
    }
    #[test]
    fn mode_change_and_monitor_replacement_are_distinct() {
        let a = geom(1920, 1080);
        let mut b = geom(1280, 960);
        assert_ne!(a, b);
        assert!(a.same_display(&b));
        b.device = "DISPLAY2".into();
        assert!(!a.same_display(&b));
    }
    #[test]
    fn portrait_is_rejected() {
        let mut g = geom(1080, 1920);
        g.rotation = 2;
        assert!(g.region(None).is_err());
    }
}
