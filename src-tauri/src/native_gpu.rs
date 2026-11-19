//! Startup-only native encoder policy. Capture/conversion/audio/ring remain shared.
//! A detected PCI vendor is NOT a hardware-support claim: the MFT must initialize
//! on the exact capture adapter and produce validated output before going healthy.
use anyhow::{bail, Result};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Intel,
    Nvidia,
    Amd,
}
impl Vendor {
    pub fn from_pci(id: u32) -> Result<Self> {
        match id {
            0x8086 => Ok(Self::Intel),
            0x10de => Ok(Self::Nvidia),
            0x1002 => Ok(Self::Amd),
            _ => bail!("No native hardware encoder policy for PCI vendor {id:04X}"),
        }
    }
    /// Preserve the verified V1 Intel initialization sequence. Other drivers
    /// receive pre-negotiation controls as specified by the codec API.
    pub fn controls_before_media_types(self) -> bool {
        self != Self::Intel
    }
    pub fn encoder(self) -> &'static str {
        match self {
            Self::Intel => "h264_qsv",
            Self::Nvidia => "h264_nvenc",
            Self::Amd => "h264_amf",
        }
    }
    pub fn choose(self, requested: &str) -> Result<&'static str> {
        if requested == "auto"
            || requested == self.encoder()
            || (self == Self::Intel && requested == "h264_qsv_lp")
        {
            Ok(self.encoder())
        } else {
            bail!("Native same-adapter path is {} but {} was requested; use FFmpeg for cross-adapter selection", self.encoder(), requested)
        }
    }
}
/// A missing B-frame control is allowed only with runtime bitstream enforcement.
/// All other rate/quality/latency controls must be accepted; no silent downgrade.
pub fn control_satisfied(label: &str, setter_ok: bool, validates_slices: bool) -> bool {
    setter_ok || (label == "b_frames=0" && validates_slices)
}
/// Try the entries enumerated for one capture adapter in system preference order.
pub fn first_activatable_encoder<T, E>(
    candidates: impl IntoIterator<Item = (String, String, T)>,
    mut activate: impl FnMut(T) -> Result<E>,
) -> Result<(String, String, E)> {
    let mut failures = Vec::new();
    for (name, vendor, candidate) in candidates {
        match activate(candidate) {
            Ok(encoder) => return Ok((name, vendor, encoder)),
            Err(error) => failures.push(format!("Activate {name}: {error:#}")),
        }
    }
    if failures.is_empty() {
        bail!("No matching hardware transform was enumerated");
    }
    bail!("{}", failures.join("; "))
}

/// Reject a display adapter switch between engine preflight and worker startup.
pub fn check_capture_adapter(expected: &str, high: i32, low: u32) -> Result<()> {
    if !expected.is_empty() && expected != format!("{high}:{low}") {
        bail!("The display capture adapter changed before Native Engine started");
    }
    Ok(())
}
/// Passive DXGI inventory: no device creation, encoding or capture. Native UI
/// still uses the legacy default-adapter indices; do not alias other outputs.
pub fn topology() -> Result<serde_json::Value> {
    // Match capture geometry even when called by a DPI-unaware CLI. The
    // existing RAII guard restores only this thread's awareness afterward.
    let _dpi = crate::geometry::PhysicalPixels::enter()?;
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
    unsafe {
        let f: IDXGIFactory1 = CreateDXGIFactory1()?;
        let mut adapters = Vec::new();
        let mut a = 0;
        while let Ok(adapter) = f.EnumAdapters1(a) {
            let d = adapter.GetDesc1()?;
            let mut outputs = Vec::new();
            let mut o = 0;
            while let Ok(output) = adapter.EnumOutputs(o) {
                let v = output.GetDesc()?;
                outputs.push(serde_json::json!({"output_index":o,"name":String::from_utf16_lossy(&v.DeviceName).trim_end_matches('\0'),"attached":v.AttachedToDesktop.as_bool(),"desktop_rect":[v.DesktopCoordinates.left,v.DesktopCoordinates.top,v.DesktopCoordinates.right,v.DesktopCoordinates.bottom],"currently_selectable_in_app":a==0}));
                o += 1;
            }
            adapters.push(serde_json::json!({"dxgi_index":a,"vendor_id":d.VendorId,"adapter_luid":format!("{}:{}",d.AdapterLuid.HighPart,d.AdapterLuid.LowPart),"description":String::from_utf16_lossy(&d.Description).trim_end_matches('\0'),"dedicated_video_bytes":d.DedicatedVideoMemory,"shared_system_bytes":d.SharedSystemMemory,"outputs":outputs}));
            a += 1;
        }
        Ok(
            serde_json::json!({"coordinate_space":"physical_pixels","adapters":adapters,"note":"Detection is not encoding support; exact-adapter MFT initialization and output validation are mandatory."}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn activation_tries_the_next_same_adapter_candidate_once() {
        for vendor in ["VEN_8086", "VEN_10DE", "VEN_1002"] {
            let candidates = || {
                [
                    ("First".into(), vendor.into(), 1),
                    ("Second".into(), vendor.into(), 2),
                ]
            };
            let mut attempted = Vec::new();
            let (name, id, result) = first_activatable_encoder(candidates(), |candidate| {
                attempted.push(candidate);
                if candidate == 1 {
                    bail!("initialization failed");
                }
                Ok(candidate)
            })
            .unwrap();
            assert_eq!(attempted, [1, 2]);
            assert_eq!((name.as_str(), id.as_str(), result), ("Second", vendor, 2));
            let error = first_activatable_encoder::<_, ()>(candidates(), |_| bail!("unavailable"))
                .unwrap_err()
                .to_string();
            assert!(error.contains("First") && error.contains("Second"));
        }
    }
    #[test]
    fn routes_each_vendor_without_downgrading_explicit_requests() {
        for (id, key) in [
            (0x8086, "h264_qsv"),
            (0x10de, "h264_nvenc"),
            (0x1002, "h264_amf"),
        ] {
            let v = Vendor::from_pci(id).unwrap();
            assert_eq!(v.choose("auto").unwrap(), key);
            assert_eq!(v.choose(key).unwrap(), key);
            assert!(v.choose("libx264").is_err());
        }
        assert!(Vendor::Nvidia.choose("h264_qsv").is_err());
        assert!(Vendor::Amd.choose("h264_nvenc").is_err());
        assert!(Vendor::from_pci(0x1414).is_err());
    }
    #[test]
    fn hybrid_startup_requires_the_exact_adapter_even_for_same_vendor_devices() {
        assert!(check_capture_adapter("0:78795", 0, 78795).is_ok());
        assert!(check_capture_adapter("0:78795", 0, 78796).is_err());
        assert!(check_capture_adapter("0:78795", 1, 78795).is_err());
        assert!(check_capture_adapter("", 0, 78796).is_ok());
    }
    #[test]
    fn intel_low_power_override_stays_compatible() {
        assert!(!Vendor::Intel.controls_before_media_types());
        assert!(Vendor::Nvidia.controls_before_media_types());
        assert!(Vendor::Amd.controls_before_media_types());
        assert_eq!(Vendor::Intel.choose("h264_qsv_lp").unwrap(), "h264_qsv");
    }
    #[test]
    fn only_no_b_frames_can_be_enforced_without_a_setter() {
        assert!(control_satisfied("b_frames=0", false, true));
        assert!(!control_satisfied("b_frames=0", false, false));
        for key in [
            "rate_control=peak_constrained_vbr",
            "mean_bitrate",
            "max_bitrate",
            "gop_size",
            "low_latency=off",
        ] {
            assert!(!control_satisfied(key, false, true));
            assert!(control_satisfied(key, true, true));
        }
    }
}
