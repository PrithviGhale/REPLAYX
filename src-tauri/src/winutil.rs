//! Small Windows integrations used by the desktop shell: audio-session
//! discovery, executable icons, Recycle Bin deletion and per-user autostart.
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::Path;
use windows::{
    core::{Interface, PCWSTR},
    Win32::{
        Foundation::ERROR_FILE_NOT_FOUND,
        Graphics::Gdi::{
            CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO,
            BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
        },
        Media::Audio::{
            eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2,
            IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
        },
        System::{
            Com::{CoCreateInstance, CLSCTX_ALL},
            Registry::{
                RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
                RRF_RT_REG_SZ,
            },
        },
        UI::{
            Shell::{
                ExtractIconExW, SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI,
                FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
            },
            WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
        },
    },
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A process that currently owns an audio session on an active output device.
#[derive(Debug, Clone, Serialize)]
pub struct AudioSession {
    pub pid: u32,
    pub name: String,
    pub path: String,
    /// Validated process start time (seconds since epoch) for capture binding.
    pub started: u64,
    /// True when the session is currently playing; inactive sessions exist but are idle.
    pub active: bool,
}

/// Processes with audio sessions on any active render endpoint, active first.
pub fn audio_sessions() -> Result<Vec<AudioSession>> {
    wasapi::initialize_mta().ok()?;
    let mut found: Vec<(u32, bool)> = Vec::new();
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let devices = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
        for d in 0..devices.GetCount()? {
            let Ok(device) = devices.Item(d) else {
                continue;
            };
            let Ok(manager) = device.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else {
                continue;
            };
            let Ok(sessions) = manager.GetSessionEnumerator() else {
                continue;
            };
            for i in 0..sessions.GetCount()? {
                let Ok(control) = sessions.GetSession(i) else {
                    continue;
                };
                let Ok(control2) = control.cast::<IAudioSessionControl2>() else {
                    continue;
                };
                // S_OK (0) marks the shared Windows system-sounds session.
                if control2.IsSystemSoundsSession().0 == 0 {
                    continue;
                }
                let Ok(pid) = control2.GetProcessId() else {
                    continue;
                };
                if pid == 0 || pid == std::process::id() {
                    continue;
                }
                let active = control
                    .GetState()
                    .is_ok_and(|s| s == AudioSessionStateActive);
                match found.iter_mut().find(|(p, _)| *p == pid) {
                    Some(entry) => entry.1 |= active,
                    None => found.push((pid, active)),
                }
            }
        }
    }
    let pids: Vec<_> = found
        .iter()
        .map(|(p, _)| sysinfo::Pid::from_u32(*p))
        .collect();
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&pids), true);
    let mut out: Vec<_> = found
        .into_iter()
        .filter_map(|(pid, active)| {
            let p = sys.process(sysinfo::Pid::from_u32(pid))?;
            Some(AudioSession {
                pid,
                name: p.name().to_string_lossy().into_owned(),
                path: p
                    .exe()
                    .map(|e| e.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                started: p.start_time(),
                active,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// The executable's first large icon as PNG bytes.
pub fn exe_icon_png(path: &Path) -> Option<Vec<u8>> {
    let file = wide(&path.to_string_lossy());
    unsafe {
        let mut icon = HICON::default();
        if ExtractIconExW(PCWSTR(file.as_ptr()), 0, Some(&mut icon), None, 1) == 0
            || icon.is_invalid()
        {
            return None;
        }
        let mut info = ICONINFO::default();
        let result = (|| {
            GetIconInfo(icon, &mut info).ok()?;
            let mut bitmap = BITMAP::default();
            if GetObjectW(
                HGDIOBJ(info.hbmColor.0),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bitmap as *mut _ as *mut _),
            ) == 0
            {
                return None;
            }
            let (w, h) = (
                bitmap.bmWidth.clamp(1, 256) as u32,
                bitmap.bmHeight.clamp(1, 256) as u32,
            );
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w as i32,
                    biHeight: -(h as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let dc = CreateCompatibleDC(None);
            let lines = GetDIBits(
                dc,
                info.hbmColor,
                0,
                h,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut header,
                DIB_RGB_COLORS,
            );
            let _ = DeleteDC(dc);
            if lines == 0 {
                return None;
            }
            let opaque = pixels.chunks(4).all(|p| p[3] == 0);
            for p in pixels.chunks_mut(4) {
                p.swap(0, 2);
                if opaque {
                    p[3] = 255;
                }
            }
            Some(png_rgba(w, h, &pixels))
        })();
        if !info.hbmColor.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        }
        if !info.hbmMask.is_invalid() {
            let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
        }
        let _ = DestroyIcon(icon);
        result
    }
}

/// Move a file to the Recycle Bin instead of deleting it permanently.
pub fn recycle(path: &Path) -> Result<()> {
    let mut from: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        bail!("Windows could not move the clip to the Recycle Bin (code {code})")
    }
    Ok(())
}

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "REPLAYX";

/// Command line registered for sign-in. `--background` starts in the tray;
/// capture never starts automatically.
pub fn autostart_command(exe: &Path) -> String {
    format!("\"{}\" --background", exe.display())
}

pub fn autostart_enabled() -> bool {
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    let mut size = 0u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
        .is_ok()
    }
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let (key, value) = (wide(RUN_KEY), wide(RUN_VALUE));
    unsafe {
        if enabled {
            let exe = std::env::current_exe().context("Cannot locate REPLAYX executable")?;
            let data = wide(&autostart_command(&exe));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                REG_SZ.0,
                Some(data.as_ptr() as *const _),
                (data.len() * 2) as u32,
            )
            .ok()
            .context("Could not register REPLAYX to start with Windows")?;
        } else {
            let status = RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
            );
            if status.is_err() && status != ERROR_FILE_NOT_FOUND {
                status.ok().context("Could not remove the startup entry")?;
            }
        }
    }
    Ok(())
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Minimal PNG encoder (uncompressed deflate blocks) for small icons.
pub fn png_rgba(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for row in rgba.chunks((w * 4) as usize).take(h as usize) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<_> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        z.push((i + 1 == blocks.len()) as u8);
        let len = block.len() as u16;
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in &raw {
        a = (a + byte as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend(((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut chunk = |kind: &[u8], data: &[u8]| {
        png.extend((data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        png.extend_from_slice(&body);
        png.extend(crc32(&body).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    chunk(b"IHDR", &ihdr);
    chunk(b"IDAT", &z);
    chunk(b"IEND", &[]);
    png
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (input, expected) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
    }

    #[test]
    fn png_has_valid_structure_and_checksums() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        let png = png_rgba(2, 2, &[255; 16]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        // IHDR: 2x2, 8-bit RGBA.
        assert_eq!(&png[16..29], &[0, 0, 0, 2, 0, 0, 0, 2, 8, 6, 0, 0, 0]);
        // Large images split into several stored deflate blocks.
        let big = png_rgba(256, 256, &vec![7; 256 * 256 * 4]);
        assert!(big.len() > 256 * 256 * 4);
    }

    #[test]
    fn autostart_never_requests_capture() {
        let cmd = autostart_command(Path::new(r"C:\Program Files\REPLAYX\replayx.exe"));
        assert_eq!(
            cmd,
            "\"C:\\Program Files\\REPLAYX\\replayx.exe\" --background"
        );
    }

    #[test]
    fn audio_session_enumeration_runs() {
        // Hardware-dependent: hosted CI runners have no audio service/endpoints.
        if std::env::var_os("REPLAYX_NO_HARDWARE").is_some() {
            return;
        }
        let sessions = audio_sessions().unwrap();
        assert!(sessions.iter().all(|s| s.pid != 0 && !s.name.is_empty()));
    }
}
