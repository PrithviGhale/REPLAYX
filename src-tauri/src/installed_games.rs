//! Icons for games that are installed but REPLAYX hasn't seen running yet,
//! found only in what Windows and the launchers already keep on this PC:
//! the "installed apps" entries (Steam, Roblox and most installers register
//! the game with its own icon) and the Epic Games Launcher manifests.
//! Read-only; nothing is downloaded, written or sent anywhere.
use std::{
    path::{Path, PathBuf},
    sync::OnceLock,
};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Installed {
    pub name: String,
    /// The file Windows shows as the app's icon (an .exe or .ico).
    pub icon: Option<PathBuf>,
    pub folder: Option<PathBuf>,
}

/// Letters and digits only, lower case ("Counter-Strike 2" -> "counterstrike2").
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Does an installed entry named `display` belong to the game `game`?
/// Exact names, plus the usual launcher suffixes ("Roblox Player").
pub fn same_game(game: &str, display: &str) -> bool {
    let g = norm(game);
    if g.len() < 3 {
        return false;
    }
    // Per-user installs append the account: "Roblox Player for Alex".
    let names = [
        Some(display),
        display.rsplit_once(" for ").map(|(name, _)| name),
    ];
    names.into_iter().flatten().map(norm).any(|d| {
        !d.is_empty()
            && (d == g
                || ["player", "launcher", "game", "pc"]
                    .iter()
                    .any(|suffix| d.strip_suffix(suffix) == Some(g.as_str())))
    })
}

/// The file in a `DisplayIcon` value (`"C:\x\game.exe",0`), if it is an
/// executable or icon file and not an uninstaller or setup program.
pub fn icon_file(raw: &str) -> Option<PathBuf> {
    let s = raw.trim().trim_start_matches('"');
    let s = match s.rsplit_once(',') {
        Some((path, index))
            if !index.trim().is_empty()
                && index
                    .trim()
                    .trim_start_matches('-')
                    .chars()
                    .all(|c| c.is_ascii_digit()) =>
        {
            path
        }
        _ => s,
    };
    let path = PathBuf::from(s.trim().trim_matches('"'));
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    if name.starts_with("unins") || name.starts_with("uninstall") || name.contains("setup") {
        return None;
    }
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    matches!(ext.as_str(), "exe" | "ico").then_some(path)
}

/// One of `exes` (lower-case file names) inside `folder`, looking at most
/// four folders deep and at a bounded number of entries.
pub fn find_exe(folder: &Path, exes: &[String]) -> Option<PathBuf> {
    if exes.is_empty() {
        return None;
    }
    let mut stack = vec![(folder.to_path_buf(), 0)];
    let mut budget = 6000;
    while let Some((dir, depth)) = stack.pop() {
        for entry in std::fs::read_dir(&dir).ok()?.flatten() {
            budget -= 1;
            if budget == 0 {
                return None;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() && depth < 4 {
                stack.push((path, depth + 1));
            } else if kind.is_file()
                && exes.contains(&entry.file_name().to_string_lossy().to_lowercase())
            {
                return Some(path);
            }
        }
    }
    None
}

/// Every installed app and Epic game this PC lists (read once per run).
pub fn entries() -> &'static [Installed] {
    static ALL: OnceLock<Vec<Installed>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut all = registry_entries();
        all.extend(epic_entries());
        all
    })
}

/// The best local icon source for a game: its own executable in its install
/// folder, otherwise the icon its installer registered.
pub fn icon_source(game: &str) -> Option<PathBuf> {
    let catalog = crate::games::catalog().by_name(game);
    let exes = catalog.map(|g| g.exe.clone()).unwrap_or_default();
    for e in entries().iter().filter(|e| {
        same_game(game, &e.name)
            || e.icon
                .as_ref()
                .and_then(|p| p.file_name())
                .is_some_and(|f| exes.contains(&f.to_string_lossy().to_lowercase()))
    }) {
        if let Some(exe) = e.folder.as_deref().and_then(|f| find_exe(f, &exes)) {
            return Some(exe);
        }
        if let Some(icon) = e.icon.as_ref().filter(|p| p.is_file()) {
            return Some(icon.clone());
        }
    }
    None
}

fn epic_entries() -> Vec<Installed> {
    let root = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        .join(r"Epic\EpicGamesLauncher\Data\Manifests");
    let Ok(dir) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    dir.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "item"))
        .filter_map(|e| {
            let bytes = std::fs::read(e.path()).ok()?;
            if bytes.len() > 1 << 20 {
                return None;
            }
            let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
            let folder = PathBuf::from(v["InstallLocation"].as_str()?);
            let launch = v["LaunchExecutable"].as_str().unwrap_or_default();
            Some(Installed {
                name: v["DisplayName"].as_str()?.to_string(),
                icon: (!launch.is_empty()).then(|| folder.join(launch)),
                folder: Some(folder),
            })
        })
        .collect()
}

fn registry_entries() -> Vec<Installed> {
    use windows::{
        core::PCWSTR,
        Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
            HKEY_LOCAL_MACHINE, KEY_READ, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
        },
    };
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let read = |key: HKEY, name: &str| -> Option<String> {
        let name = wide(name);
        let mut buf = vec![0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        unsafe {
            RegGetValueW(
                key,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
            .ok()
            .ok()?;
        }
        let len = (size as usize / 2).saturating_sub(1).min(buf.len());
        let s = String::from_utf16_lossy(&buf[..len]);
        let s = s.trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let mut out = Vec::new();
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for path in [
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let path = wide(path);
            let mut list = HKEY::default();
            if unsafe { RegOpenKeyExW(root, PCWSTR(path.as_ptr()), None, KEY_READ, &mut list) }
                .is_err()
            {
                continue;
            }
            for index in 0..4000 {
                let mut name = [0u16; 256];
                let mut len = name.len() as u32;
                let found = unsafe {
                    RegEnumKeyExW(
                        list,
                        index,
                        Some(windows::core::PWSTR(name.as_mut_ptr())),
                        &mut len,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if found.is_err() {
                    break;
                }
                let mut app = HKEY::default();
                if unsafe { RegOpenKeyExW(list, PCWSTR(name.as_ptr()), None, KEY_READ, &mut app) }
                    .is_err()
                {
                    continue;
                }
                if let Some(display) = read(app, "DisplayName") {
                    out.push(Installed {
                        name: display,
                        icon: read(app, "DisplayIcon").and_then(|i| icon_file(&i)),
                        folder: read(app, "InstallLocation")
                            .map(|f| PathBuf::from(f.trim_matches('"')))
                            .filter(|f| f.is_dir()),
                    });
                }
                unsafe {
                    let _ = RegCloseKey(app);
                }
            }
            unsafe {
                let _ = RegCloseKey(list);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_entries_match_the_right_game() {
        assert!(same_game("Roblox", "Roblox Player"));
        assert!(same_game("Roblox", "Roblox Player for Alex"));
        assert!(same_game("Roblox Studio", "Roblox Studio for Alex"));
        assert!(!same_game("Roblox", "Roblox Studio for Alex"));
        assert!(same_game("Counter-Strike 2", "Counter-Strike 2"));
        assert!(same_game("Fortnite", "Fortnite"));
        assert!(same_game("Apex Legends", "Apex Legends™"));
        assert!(!same_game("Rust", "Rust Analyzer"));
        assert!(!same_game("Fortnite", "Fortnite Festival Toolkit"));
        assert!(
            !same_game("Go", "Go Programming Language"),
            "short names never fuzzy-match"
        );
    }
    #[test]
    fn display_icon_values_parse_and_skip_uninstallers() {
        assert_eq!(
            icon_file(r#""C:\Games\My Game\game.exe",0"#),
            Some(PathBuf::from(r"C:\Games\My Game\game.exe"))
        );
        assert_eq!(
            icon_file(r"C:\Program Files (x86)\Steam\steam\games\abc.ico"),
            Some(PathBuf::from(
                r"C:\Program Files (x86)\Steam\steam\games\abc.ico"
            ))
        );
        assert_eq!(
            icon_file(r"C:\x\RobloxPlayerLauncher.exe,-101"),
            Some(PathBuf::from(r"C:\x\RobloxPlayerLauncher.exe"))
        );
        assert_eq!(icon_file(r"C:\Games\x\unins000.exe,0"), None);
        assert_eq!(icon_file(r"C:\Games\x\readme.txt"), None);
        assert_eq!(icon_file(""), None);
    }
    #[test]
    fn finds_the_game_executable_inside_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let deep = dir.path().join("Game").join("Binaries").join("Win64");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Game-Win64-Shipping.exe"), b"").unwrap();
        assert_eq!(
            find_exe(dir.path(), &["game-win64-shipping.exe".into()]),
            Some(deep.join("Game-Win64-Shipping.exe"))
        );
        assert_eq!(find_exe(dir.path(), &["other.exe".into()]), None);
    }
}
