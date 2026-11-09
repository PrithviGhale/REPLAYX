//! Game artwork without downloads: the icon of the executable REPLAYX saw
//! for each game (remembered in `games-seen.json`), or an image the user
//! picked (stored under `artwork/`). Nothing is fetched from the internet and
//! no third-party logos are bundled.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Seen {
    /// Executable path of the game the last time it was in front.
    pub exe: String,
    /// Unix milliseconds.
    pub last_seen: i64,
}

pub struct Registry {
    file: PathBuf,
    pub seen: BTreeMap<String, Seen>,
}
impl Registry {
    pub fn open(data: &Path) -> Self {
        let file = data.join("games-seen.json");
        let seen = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self { file, seen }
    }
    /// Remember the executable of a game seen in front. Writes only when the
    /// executable changed or the entry is more than ten minutes old.
    pub fn note(&mut self, game: &str, exe: &str, now_ms: i64) {
        let stale = self
            .seen
            .get(game)
            .is_none_or(|s| s.exe != exe || now_ms - s.last_seen > 600_000);
        if !stale {
            return;
        }
        self.seen.insert(
            game.to_string(),
            Seen {
                exe: exe.to_string(),
                last_seen: now_ms,
            },
        );
        if let Ok(bytes) = serde_json::to_vec_pretty(&self.seen) {
            let tmp = self.file.with_extension("json.tmp");
            if fs::write(&tmp, bytes).is_ok() {
                let _ = fs::rename(&tmp, &self.file);
            }
        }
    }
}

/// Where a game's custom image lives (name hashed: any game name is safe).
pub fn custom_path(data: &Path, game: &str) -> PathBuf {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in game.to_lowercase().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    data.join("artwork").join(format!("{h:016x}.img"))
}

/// Image type from its first bytes (PNG, JPEG or WebP only).
pub fn image_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

pub const MAX_IMAGE: usize = 4 * 1024 * 1024;

pub fn set_custom(data: &Path, game: &str, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_IMAGE {
        bail!("Images up to 4 MB, please")
    }
    if image_mime(bytes).is_none() {
        bail!("Choose a PNG, JPEG or WebP image")
    }
    let path = custom_path(data, game);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, bytes)?;
    Ok(())
}

pub fn clear_custom(data: &Path, game: &str) {
    let _ = fs::remove_file(custom_path(data, game));
}

/// The custom image as a data URL, if the user chose one.
pub fn custom_data_url(data: &Path, game: &str) -> Option<String> {
    let bytes = fs::read(custom_path(data, game)).ok()?;
    let mime = image_mime(&bytes)?;
    Some(format!(
        "data:{mime};base64,{}",
        crate::winutil::base64(&bytes)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembers_executables_and_writes_rarely() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Registry::open(dir.path());
        r.note("Roblox", r"C:\Roblox\RobloxPlayerBeta.exe", 1_000);
        let first = fs::metadata(dir.path().join("games-seen.json"))
            .unwrap()
            .modified()
            .unwrap();
        r.note("Roblox", r"C:\Roblox\RobloxPlayerBeta.exe", 2_000); // same exe, recent: no write
        assert_eq!(r.seen["Roblox"].last_seen, 1_000);
        r.note("Roblox", r"C:\Roblox\v2\RobloxPlayerBeta.exe", 3_000); // Roblox moved: write
        let reopened = Registry::open(dir.path());
        assert_eq!(
            reopened.seen["Roblox"].exe,
            r"C:\Roblox\v2\RobloxPlayerBeta.exe"
        );
        assert!(
            fs::metadata(dir.path().join("games-seen.json"))
                .unwrap()
                .modified()
                .unwrap()
                >= first
        );
    }
    #[test]
    fn custom_images_are_validated_and_removable() {
        let dir = tempfile::tempdir().unwrap();
        assert!(set_custom(dir.path(), "Roblox", b"not an image").is_err());
        assert!(set_custom(dir.path(), "Roblox", &vec![0x89; MAX_IMAGE + 1]).is_err());
        let png = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
        set_custom(dir.path(), "Counter-Strike 2", &png).unwrap();
        assert!(custom_data_url(dir.path(), "counter-strike 2")
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert_ne!(custom_path(dir.path(), "a"), custom_path(dir.path(), "b"));
        clear_custom(dir.path(), "Counter-Strike 2");
        assert!(custom_data_url(dir.path(), "Counter-Strike 2").is_none());
        assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    }
}
