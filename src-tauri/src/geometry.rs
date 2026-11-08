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
    /// PCI vendor of the GPU driving this display (0x8086 Intel, 0x10DE
    /// NVIDIA, 0x1002 AMD). Hybrid laptops can move the display between GPUs
    /// (for example NVIDIA Advanced Optimus while a full-screen game runs).
    #[serde(default)]
    pub adapter_vendor: u32,
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
            adapter_vendor: adapter_desc.VendorId,
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
/// Aspect handling for the encoded frame.
/// - `stretch`: fill the 16:9 canvas exactly (stretched 4:3 gameplay).
/// - `fit`: preserve source aspect inside the 16:9 canvas with black bars.
/// - `crop`: preserve aspect and fill the 16:9 canvas, cropping the source
///   centre at capture time so cropped pixels are never copied.
/// - `native`: preserve source aspect at the selected height, no bars.
pub const CANVAS_MODES: [&str; 4] = ["stretch", "fit", "crop", "native"];

/// What is captured and how it is placed in the encoded frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoPlan {
    /// Captured source rectangle in output-local physical pixels.
    pub source: CaptureRect,
    /// Encoded frame size.
    pub width: u32,
    pub height: u32,
    /// Scaled image size inside the frame; smaller than the frame only for `fit`.
    pub image_width: u32,
    pub image_height: u32,
}
impl VideoPlan {
    pub fn padded(&self) -> bool {
        (self.image_width, self.image_height) != (self.width, self.height)
    }
    pub fn pad_filter(&self) -> Option<String> {
        self.padded().then(|| {
            format!(
                "pad={}:{}:{}:{}:black",
                self.width,
                self.height,
                ((self.width - self.image_width) / 2) & !1,
                ((self.height - self.image_height) / 2) & !1
            )
        })
    }
}
fn even(v: u32) -> u32 {
    (v & !1).max(2)
}
fn scaled(v: u32, num: u32, den: u32) -> u32 {
    even(((v as u64 * num as u64 + den as u64 / 2) / den as u64) as u32)
}
/// Plan the encoded frame. `width`×`height` is the output size for
/// stretch/fit/crop; `native` keeps the display's shape at `height`.
pub fn plan(source: CaptureRect, width: u32, height: u32, mode: &str) -> Result<VideoPlan> {
    let (cw, ch) = (width, height);
    if cw < 2 || ch < 2 || !cw.is_multiple_of(2) || !ch.is_multiple_of(2) {
        bail!("Unsupported output size {width}x{height}")
    }
    let (sw, sh) = (source.width, source.height);
    let fixed = |source: CaptureRect, iw: u32, ih: u32| VideoPlan {
        source,
        width: cw,
        height: ch,
        image_width: iw,
        image_height: ih,
    };
    Ok(match mode {
        "stretch" => fixed(source, cw, ch),
        "fit" => {
            // Largest image with the source aspect that fits the canvas.
            if sw as u64 * ch as u64 >= sh as u64 * cw as u64 {
                fixed(source, cw, scaled(sh, cw, sw).min(ch))
            } else {
                fixed(source, scaled(sw, ch, sh).min(cw), ch)
            }
        }
        "crop" => {
            let mut s = source;
            if sw as u64 * ch as u64 > sh as u64 * cw as u64 {
                s.width = scaled(sh, cw, ch).min(sw);
                s.x += (sw - s.width) / 2;
            } else {
                s.height = scaled(sw, ch, cw).min(sh);
                s.y += (sh - s.height) / 2;
            }
            fixed(s, cw, ch)
        }
        "native" => {
            let w = scaled(sw, ch, sh);
            VideoPlan {
                source,
                width: w,
                height: ch,
                image_width: w,
                image_height: ch,
            }
        }
        _ => bail!("Unknown aspect mode"),
    })
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
            adapter_vendor: 0x8086,
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
            let p = plan(
                geom(w, h).region(None).unwrap(),
                canvas(1080).0,
                1080,
                "stretch",
            )
            .unwrap();
            assert_eq!((p.width, p.height), (1920, 1080));
            assert_eq!((p.image_width, p.image_height), (1920, 1080));
            assert_eq!((p.source.width, p.source.height), (w, h));
            assert!(!p.padded());
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
    fn full(w: u32, h: u32) -> CaptureRect {
        CaptureRect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        }
    }
    #[test]
    fn fit_letterboxes_or_pillarboxes_without_distortion() {
        // 4:3 game into 16:9: pillarbox 1440x1080 centred.
        let p = plan(full(1440, 1080), canvas(1080).0, 1080, "fit").unwrap();
        assert_eq!(
            (p.width, p.height, p.image_width, p.image_height),
            (1920, 1080, 1440, 1080)
        );
        assert_eq!(p.pad_filter().unwrap(), "pad=1920:1080:240:0:black");
        // Offsets are even and the image always fits inside the frame.
        for (w, h) in [(1440, 1080), (3440, 1440), (1366, 768), (1280, 1024)] {
            let p = plan(full(w, h), canvas(720).0, 720, "fit").unwrap();
            let pad = p.pad_filter().unwrap_or_default();
            if let [_, _, x, y] = pad
                .trim_start_matches("pad=")
                .split(':')
                .take(4)
                .collect::<Vec<_>>()[..]
            {
                let (x, y): (u32, u32) = (x.parse().unwrap(), y.parse().unwrap());
                assert!(x % 2 == 0 && y % 2 == 0);
                assert!(x + p.image_width <= p.width && y + p.image_height <= p.height);
            }
        }
        // 16:10 laptop panel into 16:9: pillarbox 1728x1080.
        let p = plan(full(2560, 1600), canvas(1080).0, 1080, "fit").unwrap();
        assert_eq!((p.image_width, p.image_height), (1728, 1080));
        // Ultrawide 21:9: letterbox.
        let p = plan(full(3440, 1440), canvas(1080).0, 1080, "fit").unwrap();
        assert_eq!((p.image_width, p.image_height), (1920, 804));
        // 16:9 source: no bars.
        assert!(!plan(full(2560, 1440), canvas(1080).0, 1080, "fit")
            .unwrap()
            .padded());
    }
    #[test]
    fn crop_fills_canvas_by_cropping_source_centre_at_capture() {
        let p = plan(full(2560, 1600), canvas(1080).0, 1080, "crop").unwrap();
        assert_eq!(
            p.source,
            CaptureRect {
                x: 0,
                y: 80,
                width: 2560,
                height: 1440
            }
        );
        assert_eq!((p.width, p.height), (1920, 1080));
        assert!(!p.padded());
        let p = plan(full(1440, 1080), canvas(1080).0, 1080, "crop").unwrap();
        assert_eq!(
            p.source,
            CaptureRect {
                x: 0,
                y: 135,
                width: 1440,
                height: 810
            }
        );
        // Crop respects a manual region offset.
        let r = CaptureRect {
            x: 100,
            y: 50,
            width: 1440,
            height: 1080,
        };
        assert_eq!(
            plan(r, canvas(720).0, 720, "crop").unwrap().source.y,
            50 + 135
        );
    }
    #[test]
    fn native_keeps_source_aspect_at_selected_height() {
        let p = plan(full(1440, 1080), canvas(1080).0, 1080, "native").unwrap();
        assert_eq!((p.width, p.height), (1440, 1080));
        let p = plan(full(2560, 1600), canvas(1080).0, 1080, "native").unwrap();
        assert_eq!((p.width, p.height), (1728, 1080));
        let p = plan(full(1280, 960), canvas(720).0, 720, "native").unwrap();
        assert_eq!((p.width, p.height), (960, 720));
    }
    #[test]
    fn every_mode_produces_even_encoder_dimensions() {
        for mode in CANVAS_MODES {
            for (w, h) in [
                (1366, 768),
                (1281, 961),
                (2560, 1080),
                (1440, 1080),
                (3840, 2160),
            ] {
                for height in [480, 720, 1080, 1440] {
                    let p = plan(full(w, h), canvas(height).0, height, mode).unwrap();
                    for v in [p.width, p.height, p.image_width, p.image_height] {
                        assert_eq!(v % 2, 0, "{mode} {w}x{h}@{height}");
                    }
                    assert!(p.image_width <= p.width && p.image_height <= p.height);
                    assert!(p.source.width <= w && p.source.height <= h);
                }
            }
        }
        assert!(plan(full(1920, 1080), canvas(1080).0, 1080, "zoom").is_err());
    }
    #[test]
    fn a_16_10_display_records_a_true_1920x1080() {
        let src = full(2560, 1600);
        for mode in ["stretch", "fit", "crop"] {
            let p = plan(src, 1920, 1080, mode).unwrap();
            assert_eq!((p.width, p.height), (1920, 1080), "{mode}");
        }
        let fit = plan(src, 1920, 1080, "fit").unwrap();
        assert_eq!(
            (fit.image_width, fit.image_height),
            (1728, 1080),
            "bars left and right"
        );
        let crop = plan(src, 1920, 1080, "crop").unwrap();
        assert_eq!((crop.source.width, crop.source.height), (2560, 1440));
        assert_eq!(crop.source.y, 80, "centre crop, never an offset region");
        let native = plan(src, 1920, 1080, "native").unwrap();
        assert_eq!(
            (native.width, native.height),
            (1728, 1080),
            "native keeps the display shape"
        );
        // Stretched 4:3 gameplay fills a 16:9 frame without bars.
        let s43 = plan(full(1440, 1080), 1920, 1080, "stretch").unwrap();
        assert!(!s43.padded());
        assert!(plan(src, 1921, 1080, "stretch").is_err());
    }
}
