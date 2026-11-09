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
    exe_icon_png_sized(path, 0)
}
/// The executable's icon at up to `size` pixels (0 = Windows' large icon
/// size, usually 32). Game tiles ask for 256 so the art stays sharp.
pub fn exe_icon_png_sized(path: &Path, size: u32) -> Option<Vec<u8>> {
    let file = wide(&path.to_string_lossy());
    unsafe {
        let mut icon = HICON::default();
        let sized = size > 0
            && windows::Win32::UI::Shell::SHDefExtractIconW(
                PCWSTR(file.as_ptr()),
                0,
                0,
                Some(&mut icon),
                None,
                size.min(256),
            )
            .is_ok()
            && !icon.is_invalid();
        if !sized
            && (ExtractIconExW(PCWSTR(file.as_ptr()), 0, Some(&mut icon), None, 1) == 0
                || icon.is_invalid())
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
    // SHFileOperation rejects verbatim ("\\?\") paths with error 124, which is what
    // canonicalize() returns; always pass a plain absolute path.
    let path = crate::library::shell_path(path);
    if !path.is_absolute() {
        bail!("Recycle Bin requires an absolute path")
    }
    let mut from: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
    from.extend([0, 0]);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 {
        bail!(crate::library::recycle_error(code))
    }
    if op.fAnyOperationsAborted.as_bool() {
        bail!("Moving the clip to the Recycle Bin was cancelled")
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

/// What to do with the "Start with Windows" entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutostartAction {
    Leave,
    Register(String),
    Remove,
}

/// Is this the installed REPLAYX (a release build in the folder the installer
/// created, with its uninstaller beside it)? Development and copied builds
/// are not: a debug build is a console program, and registering it made a
/// terminal window open at every sign-in.
pub fn is_installed_build(exe: &Path) -> bool {
    !cfg!(debug_assertions)
        && exe
            .parent()
            .is_some_and(|d| d.join("uninstall.exe").is_file())
}

/// A registered command that starts a build-output executable (cargo's
/// `target\debug` or `target\release`), left by an older development run.
pub fn is_build_output_command(command: &str) -> bool {
    let c = command.to_ascii_lowercase().replace('/', "\\");
    c.contains("\\target\\debug\\") || c.contains("\\target\\release\\")
}

/// Pure decision: only the installed app owns the entry; it points the entry
/// at itself when wanted and removes it when not. Other builds never
/// register, and remove only a stale entry that starts a build output.
pub fn autostart_plan(
    wanted: bool,
    exe: &Path,
    installed: bool,
    current: Option<&str>,
) -> AutostartAction {
    if !installed {
        return match current {
            Some(c) if is_build_output_command(c) => AutostartAction::Remove,
            _ => AutostartAction::Leave,
        };
    }
    let command = autostart_command(exe);
    match (wanted, current) {
        (true, Some(c)) if c == command => AutostartAction::Leave,
        (true, _) => AutostartAction::Register(command),
        (false, Some(_)) => AutostartAction::Remove,
        (false, None) => AutostartAction::Leave,
    }
}

fn read_run_value(key: &str, name: &str) -> Option<String> {
    let (key, name) = (wide(key), wide(name));
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()
        .ok()?;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn apply_run_value(key: &str, name: &str, action: &AutostartAction) -> Result<()> {
    let (key, name) = (wide(key), wide(name));
    unsafe {
        match action {
            AutostartAction::Leave => {}
            AutostartAction::Register(command) => {
                let data = wide(command);
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    REG_SZ.0,
                    Some(data.as_ptr() as *const _),
                    (data.len() * 2) as u32,
                )
                .ok()
                .context("Could not register REPLAYX to start with Windows")?;
            }
            AutostartAction::Remove => {
                let status = RegDeleteKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(key.as_ptr()),
                    PCWSTR(name.as_ptr()),
                );
                if status.is_err() && status != ERROR_FILE_NOT_FOUND {
                    status.ok().context("Could not remove the startup entry")?;
                }
            }
        }
    }
    Ok(())
}

/// The command REPLAYX's sign-in entry currently runs, if any.
pub fn autostart_entry() -> Option<String> {
    read_run_value(RUN_KEY, RUN_VALUE)
}

/// Make the sign-in entry match the setting (see `autostart_plan`). Returns
/// what was done. Only the per-user Run value named "REPLAYX" is touched.
pub fn sync_autostart(wanted: bool) -> Result<AutostartAction> {
    let exe = std::env::current_exe().context("Cannot locate REPLAYX executable")?;
    let action = autostart_plan(
        wanted,
        &exe,
        is_installed_build(&exe),
        autostart_entry().as_deref(),
    );
    apply_run_value(RUN_KEY, RUN_VALUE, &action)?;
    Ok(action)
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
    fn only_the_installed_app_owns_the_startup_entry() {
        use AutostartAction::*;
        let app = Path::new(r"C:\Users\A\AppData\Local\REPLAYX\replayx.exe");
        let cmd = autostart_command(app);
        let dev = r#""C:\Users\A\Desktop\VS CODE\REPLAYX\src-tauri\target\debug\replayx.exe" --background"#;
        // Installed: points the entry at itself, keeps it, or removes it when off.
        assert_eq!(autostart_plan(true, app, true, None), Register(cmd.clone()));
        assert_eq!(
            autostart_plan(true, app, true, Some(dev)),
            Register(cmd.clone()),
            "stale dev entry replaced"
        );
        assert_eq!(autostart_plan(true, app, true, Some(&cmd)), Leave);
        assert_eq!(
            autostart_plan(false, app, true, Some(&cmd)),
            Remove,
            "turning it off stops future launches"
        );
        assert_eq!(autostart_plan(false, app, true, None), Leave);
        // Development builds never register, and clean up a dev entry.
        let debug = Path::new(r"C:\x\target\debug\replayx.exe");
        assert_eq!(autostart_plan(true, debug, false, None), Leave);
        assert_eq!(autostart_plan(true, debug, false, Some(dev)), Remove);
        assert_eq!(
            autostart_plan(true, debug, false, Some(&cmd)),
            Leave,
            "the installed app's entry is not touched"
        );
        assert!(is_build_output_command(
            r#""C:/x/target/release/replayx.exe" --background"#
        ));
        assert!(!is_build_output_command(&cmd));
        assert!(
            !is_installed_build(debug),
            "debug builds are never the installed app"
        );
    }
    #[test]
    fn startup_entry_round_trips_in_a_scratch_key() {
        // A throwaway key under HKCU\Software, never the real Run key.
        let key = format!(r"Software\REPLAYX-test-{}", std::process::id());
        let cmd = r#""C:\Program Files\REPLAYX\replayx.exe" --background"#.to_string();
        apply_run_value(&key, "REPLAYX", &AutostartAction::Register(cmd.clone())).unwrap();
        assert_eq!(
            read_run_value(&key, "REPLAYX").as_deref(),
            Some(cmd.as_str())
        );
        apply_run_value(&key, "REPLAYX", &AutostartAction::Remove).unwrap();
        assert_eq!(read_run_value(&key, "REPLAYX"), None);
        apply_run_value(&key, "REPLAYX", &AutostartAction::Remove).unwrap(); // already gone: fine
        unsafe {
            let wkey = wide(&key);
            let _ = windows::Win32::System::Registry::RegDeleteKeyW(
                HKEY_CURRENT_USER,
                PCWSTR(wkey.as_ptr()),
            );
        }
    }

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
    fn recycle_bin_accepts_canonical_paths() {
        // Opt-in: moves one temporary 1-byte file into the user's Recycle Bin.
        if std::env::var_os("REPLAYX_RECYCLE_TEST").is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!("rx-recycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("replayx-test-clip.mkv"), b"x").unwrap();
        let trashed = crate::library::delete_clip(&root, "replayx-test-clip.mkv", recycle).unwrap();
        assert!(!trashed.exists());
        std::fs::remove_dir_all(root).unwrap();
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

/// Decode standard Base64 (padding optional); None on any invalid input.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let bytes: Vec<u8> = text.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    let body = bytes
        .strip_suffix(b"==")
        .or_else(|| bytes.strip_suffix(b"="))
        .unwrap_or(&bytes);
    if body.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    for chunk in body.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= value(*c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod base64_tests {
    #[test]
    fn base64_round_trips() {
        for data in [
            &b""[..],
            b"f",
            b"fo",
            b"foo",
            b"foob",
            b"\x89PNG\r\n\x1a\n\x00\xff",
        ] {
            assert_eq!(
                super::base64_decode(&super::base64(data)).as_deref(),
                Some(data)
            );
        }
        assert!(super::base64_decode("not*base64").is_none());
    }
}
