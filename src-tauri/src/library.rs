//! Clip library: clip identity, safe path resolution and file operations.
//!
//! A clip id is its path relative to the recordings folder, using `/`:
//! `Fortnite/Fortnite_2026-11-07_15-30-22.mkv`, or a bare file name for clips
//! saved by earlier versions directly in the folder. Ids are validated before
//! any filesystem access and must resolve inside the recordings folder.
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Clips saved directly in the recordings folder (before 0.2.1).
pub const UNSORTED: &str = "Unsorted";

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Convert any name into a safe Windows path component (game folders, files).
pub fn sanitize_component(name: &str) -> String {
    let mut out: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    out = out.trim_matches(['.', ' ']).chars().take(64).collect();
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if out.is_empty() || RESERVED.contains(&stem.as_str()) {
        out = format!("_{out}");
    }
    out.trim_end_matches(['.', ' ']).to_string()
}

fn valid_component(c: &str) -> bool {
    !c.is_empty()
        && c != "."
        && c != ".."
        && !c.starts_with('.')
        && !c.ends_with(['.', ' '])
        && !c.contains(['<', '>', ':', '"', '\\', '|', '?', '*'])
        && !c.chars().any(char::is_control)
        && sanitize_component(c) == c
}

/// Split and validate a clip id: `file.mkv` or `Category/file.mkv`.
pub fn parse_id(id: &str) -> Result<(Option<&str>, &str)> {
    let parts: Vec<&str> = id.split('/').collect();
    let (folder, file) = match parts.as_slice() {
        [file] => (None, *file),
        [folder, file] => (Some(*folder), *file),
        _ => bail!("Invalid clip id"),
    };
    if folder.is_some_and(|f| !valid_component(f)) || !valid_component(file) {
        bail!("Invalid clip id")
    }
    let ext = Path::new(file)
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !["mkv", "mp4"].contains(&ext.as_str()) {
        bail!("Only .mkv and .mp4 clips can be managed")
    }
    Ok((folder, file))
}

/// Remove the `\\?\` verbatim prefix that `canonicalize` adds. Shell APIs such
/// as the Recycle Bin reject verbatim paths (SHFileOperation error 124).
pub fn shell_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

fn is_reparse(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & 0x400 != 0)
}

/// Resolve a validated clip id to an existing regular file inside `root`.
/// The returned path is shell-safe (no verbatim prefix).
pub fn resolve(root: &Path, id: &str) -> Result<PathBuf> {
    let (folder, file) = parse_id(id)?;
    let dir = match folder {
        Some(f) => root.join(f),
        None => root.to_path_buf(),
    };
    let path = dir.join(file);
    if !path.exists() {
        bail!("The clip no longer exists. It may have been moved or deleted outside REPLAYX.")
    }
    if is_reparse(&path) || (folder.is_some() && is_reparse(&dir)) {
        bail!("Links and junctions in the recordings folder are not followed")
    }
    let canonical_root = root
        .canonicalize()
        .context("Recordings folder unavailable")?;
    let canonical = path.canonicalize()?;
    let parent = canonical.parent().context("Invalid clip path")?;
    let inside = parent == canonical_root
        || (folder.is_some() && parent.parent() == Some(canonical_root.as_path()));
    if !inside || !canonical.is_file() {
        bail!("Clip is outside the recordings folder")
    }
    Ok(shell_path(&canonical))
}

/// The clip id for a file inside `root` (None if it is not a library clip).
pub fn id_for(root: &Path, file: &Path) -> Option<String> {
    let root = shell_path(&root.canonicalize().ok()?);
    let file = shell_path(&file.canonicalize().ok()?);
    let rel = file.strip_prefix(&root).ok()?;
    let id = rel.to_string_lossy().replace('\\', "/");
    parse_id(&id).ok()?;
    Some(id)
}

/// Category shown in the library for a clip id.
pub fn category_of(id: &str) -> String {
    match id.split_once('/') {
        Some((folder, _)) => folder.to_string(),
        None => UNSORTED.to_string(),
    }
}

/// True when another program holds the file without delete sharing
/// (typically a video player), which would make the Recycle Bin fail.
pub fn in_use(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    match fs::OpenOptions::new().read(true).share_mode(0).open(path) {
        Ok(_) => false,
        Err(e) => e.raw_os_error() == Some(32) || e.raw_os_error() == Some(33),
    }
}

/// Validate, check for locks, then move a clip with `trash` (the Recycle Bin
/// in the app; a stub in tests). Returns the path that was trashed.
pub fn delete_clip(
    root: &Path,
    id: &str,
    trash: impl FnOnce(&Path) -> Result<()>,
) -> Result<PathBuf> {
    let path = resolve(root, id)?;
    if in_use(&path) {
        bail!(
            "The clip is open in another program, such as a video player. Close it and try again."
        )
    }
    trash(&path)?;
    if path.exists() {
        bail!("Windows reported success but the clip is still there")
    }
    Ok(path)
}

/// Human-readable reason for an SHFileOperation result code.
pub fn recycle_error(code: i32) -> String {
    let reason = match code {
        2 | 0x7C | 0x402 => "the file was not found or the path is invalid",
        5 | 0x78 => "access was denied",
        32 | 33 => "the file is open in another program",
        0x10000..=0x1ffff => "Windows reported an error accessing the file",
        _ => "Windows could not complete the operation",
    };
    format!("Could not move the clip to the Recycle Bin: {reason} (code {code}).")
}

/// A non-existing destination in `dir` for `stem` + `ext`, never overwriting.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{stem}.{ext}"));
    if !first.exists() && !dir.join(format!("{stem}.partial.{ext}")).exists() {
        return first;
    }
    (2..10_000)
        .map(|n| dir.join(format!("{stem}_{n}.{ext}")))
        .find(|p| !p.exists() && !p.with_extension(format!("partial.{ext}")).exists())
        .unwrap_or_else(|| dir.join(format!("{stem}_{}.{ext}", uuid::Uuid::new_v4())))
}

/// Move a clip into another category folder (creating it). Returns the new id.
pub fn move_to_category(root: &Path, id: &str, category: &str) -> Result<String> {
    let from = resolve(root, id)?;
    let folder = sanitize_component(category);
    if folder.is_empty() || folder == UNSORTED {
        bail!("Choose a game or category name")
    }
    let dir = root.join(&folder);
    if is_reparse(&dir) {
        bail!("Links and junctions in the recordings folder are not followed")
    }
    fs::create_dir_all(&dir)?;
    let file = Path::new(&from);
    let stem = file.file_stem().unwrap().to_string_lossy().to_string();
    let ext = file.extension().unwrap().to_string_lossy().to_string();
    let to = unique_path(&dir, &stem, &ext);
    if in_use(&from) {
        bail!("The clip is open in another program. Close it and try again.")
    }
    fs::rename(&from, &to)?;
    Ok(format!(
        "{folder}/{}",
        to.file_name().unwrap().to_string_lossy()
    ))
}

/// Every clip id under `root`: files in the folder and one level of category
/// folders. Partial (still saving) files and links are skipped.
pub fn scan(root: &Path) -> Result<Vec<(String, PathBuf, fs::Metadata)>> {
    let mut out = Vec::new();
    let mut visit = |dir: &Path, prefix: Option<&str>| -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let meta = entry.metadata()?;
            if !meta.is_file() || name.contains(".partial.") {
                continue;
            }
            let id = match prefix {
                Some(p) => format!("{p}/{name}"),
                None => name,
            };
            if parse_id(&id).is_ok() && !is_reparse(&entry.path()) {
                out.push((id, entry.path(), meta));
            }
        }
        Ok(())
    };
    visit(root, None)?;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        if entry.file_type()?.is_dir() && valid_component(&name) && !is_reparse(&entry.path()) {
            visit(&entry.path(), Some(&name))?;
        }
    }
    Ok(out)
}

/// One library entry as shown in the Clips page.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    /// Relative id (`Category/file.mkv` or `file.mkv`).
    pub id: String,
    pub name: String,
    pub category: String,
    /// "Replay", "Recording", "Export" or "Clip".
    pub kind: String,
    pub modified: u64,
    pub bytes: u64,
    pub probe: Option<crate::media::Probe>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct IndexEntry {
    size: u64,
    modified_ns: u128,
    probe: Option<crate::media::Probe>,
}

/// Persistent probe cache, keyed by clip id and validated by size and
/// modification time, so unchanged clips are never probed again, including
/// across app restarts. Stored as JSON in the app data folder.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Index {
    version: u32,
    entries: std::collections::HashMap<String, IndexEntry>,
    #[serde(skip)]
    dirty: bool,
}
const INDEX_VERSION: u32 = 2;

fn kind_from(name: &str, probe: Option<&crate::media::Probe>) -> String {
    if let Some(k) = probe.and_then(|p| p.kind.clone()) {
        return k;
    }
    if name.to_ascii_lowercase().ends_with(".mp4") {
        return "Export".into();
    }
    ["Replay", "Recording", "Recovered"]
        .into_iter()
        .find(|k| name.starts_with(k))
        .unwrap_or("Clip")
        .into()
}

fn modified_ns(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

impl Index {
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Index>(&b).ok())
            .filter(|i| i.version == INDEX_VERSION)
            .unwrap_or_else(|| Index {
                version: INDEX_VERSION,
                ..Default::default()
            })
    }
    pub fn save(&mut self, path: &Path) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(self)?)?;
        fs::rename(&tmp, path)?;
        self.dirty = false;
        Ok(())
    }
    /// Record metadata for a clip that was just saved (no probe needed).
    pub fn insert(&mut self, id: &str, meta: &fs::Metadata, probe: crate::media::Probe) {
        self.entries.insert(
            id.into(),
            IndexEntry {
                size: meta.len(),
                modified_ns: modified_ns(meta),
                probe: Some(probe),
            },
        );
        self.dirty = true;
    }
    pub fn remove(&mut self, id: &str) {
        self.dirty |= self.entries.remove(id).is_some();
    }
    /// The clip for one id, from the index (used right after a save).
    pub fn clip(&self, root: &Path, id: &str) -> Option<Clip> {
        let meta = fs::metadata(resolve(root, id).ok()?).ok()?;
        let probe = self.entries.get(id).and_then(|e| e.probe.clone());
        Some(make_clip(id.to_string(), &meta, probe))
    }
    /// Indices into `found` whose metadata is missing or out of date.
    pub fn stale(&self, found: &[(String, PathBuf, fs::Metadata)]) -> Vec<usize> {
        found
            .iter()
            .enumerate()
            .filter(|(_, (id, _, meta))| {
                self.entries
                    .get(id)
                    .is_none_or(|e| e.size != meta.len() || e.modified_ns != modified_ns(meta))
            })
            .map(|(i, _)| i)
            .collect()
    }
    /// Store probe results for files found by `scan`.
    pub fn apply(
        &mut self,
        found: &[(String, PathBuf, fs::Metadata)],
        results: Vec<(usize, Option<crate::media::Probe>)>,
    ) {
        for (i, probe) in results {
            let (id, _, meta) = &found[i];
            self.entries.insert(
                id.clone(),
                IndexEntry {
                    size: meta.len(),
                    modified_ns: modified_ns(meta),
                    probe,
                },
            );
            self.dirty = true;
        }
    }
    /// Library entries for `found`, using whatever metadata is indexed
    /// (stale entries appear with `probe: None`); forgets removed files.
    pub fn clips(&mut self, found: &[(String, PathBuf, fs::Metadata)]) -> Vec<Clip> {
        let ids: std::collections::HashSet<&str> =
            found.iter().map(|(id, _, _)| id.as_str()).collect();
        let before = self.entries.len();
        self.entries.retain(|id, _| ids.contains(id.as_str()));
        self.dirty |= self.entries.len() != before;
        let mut out: Vec<Clip> = found
            .iter()
            .map(|(id, _, meta)| {
                let probe = self
                    .entries
                    .get(id)
                    .filter(|e| e.size == meta.len() && e.modified_ns == modified_ns(meta))
                    .and_then(|e| e.probe.clone());
                make_clip(id.clone(), meta, probe)
            })
            .collect();
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.id.cmp(&b.id)));
        out
    }
    /// Scan, probe stale files (up to `threads` in parallel) and list.
    pub fn list(
        &mut self,
        root: &Path,
        threads: usize,
        probe: impl Fn(&Path) -> Option<crate::media::Probe> + Sync,
    ) -> Result<Vec<Clip>> {
        let found = scan(root)?;
        let stale = self.stale(&found);
        let results = probe_parallel(&found, &stale, threads, probe);
        self.apply(&found, results);
        Ok(self.clips(&found))
    }
}

/// Probe `found[i]` for each i in `stale` using up to `threads` workers.
pub fn probe_parallel(
    found: &[(String, PathBuf, fs::Metadata)],
    stale: &[usize],
    threads: usize,
    probe: impl Fn(&Path) -> Option<crate::media::Probe> + Sync,
) -> Vec<(usize, Option<crate::media::Probe>)> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let probed = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..threads.clamp(1, 8).min(stale.len().max(1)) {
            scope.spawn(|| loop {
                let k = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(&i) = stale.get(k) else { break };
                let result = probe(&found[i].1);
                probed.lock().unwrap().push((i, result));
            });
        }
    });
    probed.into_inner().unwrap()
}

fn make_clip(id: String, meta: &fs::Metadata, probe: Option<crate::media::Probe>) -> Clip {
    let name = id.rsplit('/').next().unwrap_or(&id).to_string();
    Clip {
        category: category_of(&id),
        kind: kind_from(&name, probe.as_ref()),
        modified: (modified_ns(meta) / 1_000_000_000) as u64,
        bytes: meta.len(),
        name,
        id,
        probe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rx-lib-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn verbatim_prefix_is_removed_for_shell_apis() {
        // Root cause of the 0.2.0 Delete bug: canonicalize() returns \\?\ paths.
        assert_eq!(
            shell_path(Path::new(r"\\?\C:\Videos\a.mkv")),
            PathBuf::from(r"C:\Videos\a.mkv")
        );
        assert_eq!(
            shell_path(Path::new(r"\\?\UNC\nas\share\a.mkv")),
            PathBuf::from(r"\\nas\share\a.mkv")
        );
        let root = temp_root();
        fs::write(root.join("a.mkv"), b"x").unwrap();
        let p = resolve(&root, "a.mkv").unwrap();
        assert!(!p.to_string_lossy().starts_with(r"\\?\"), "{}", p.display());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ids_reject_traversal_and_unsafe_names() {
        for bad in [
            "",
            "../a.mkv",
            "a/../../b.mkv",
            "a/b/c.mkv",
            "..\\a.mkv",
            "C:\\a.mkv",
            "C:a.mkv",
            ".hidden.mkv",
            "a.exe",
            "Fortnite/a.txt",
            "CON/a.mkv",
            "a.mkv.",
            "x/ a.mkv",
            "a?.mkv",
            "/a.mkv",
            "a/",
        ] {
            assert!(parse_id(bad).is_err(), "{bad:?} accepted");
        }
        assert_eq!(parse_id("a.mkv").unwrap(), (None, "a.mkv"));
        assert_eq!(
            parse_id("Counter-Strike 2/cs_1.mkv").unwrap(),
            (Some("Counter-Strike 2"), "cs_1.mkv")
        );
    }

    #[test]
    fn delete_moves_only_the_requested_file() {
        let root = temp_root();
        fs::create_dir_all(root.join("Fortnite")).unwrap();
        fs::write(root.join("Fortnite/f.mkv"), b"x").unwrap();
        fs::write(root.join("Fortnite/keep.mkv"), b"x").unwrap();
        let mut trashed = Vec::new();
        let p = delete_clip(&root, "Fortnite/f.mkv", |p| {
            trashed.push(p.to_path_buf());
            fs::remove_file(p)?;
            Ok(())
        })
        .unwrap();
        assert_eq!(trashed, vec![p]);
        assert!(!root.join("Fortnite/f.mkv").exists());
        assert!(root.join("Fortnite/keep.mkv").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_reports_missing_invalid_and_outside_clips_without_trashing() {
        let root = temp_root();
        let outside = temp_root();
        fs::write(outside.join("victim.mkv"), b"x").unwrap();
        let never = |_: &Path| -> Result<()> { panic!("must not trash") };
        assert!(delete_clip(&root, "gone.mkv", never)
            .unwrap_err()
            .to_string()
            .contains("no longer exists"));
        assert!(delete_clip(&root, "../victim.mkv", never).is_err());
        assert!(delete_clip(&root, "x/../../victim.mkv", never).is_err());
        assert!(outside.join("victim.mkv").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn delete_refuses_a_clip_that_is_open_elsewhere() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = temp_root();
        fs::write(root.join("open.mkv"), b"x").unwrap();
        // Like a video player: read access without delete sharing.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(root.join("open.mkv"))
            .unwrap();
        let err = delete_clip(&root, "open.mkv", |_| panic!("must not trash")).unwrap_err();
        assert!(err.to_string().contains("open in another program"), "{err}");
        drop(held);
        delete_clip(&root, "open.mkv", |p| Ok(fs::remove_file(p)?)).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_trash_failure_is_reported_and_the_file_kept() {
        let root = temp_root();
        fs::write(root.join("a.mkv"), b"x").unwrap();
        let err = delete_clip(&root, "a.mkv", |_| bail!(recycle_error(124))).unwrap_err();
        assert!(err.to_string().contains("not found or the path is invalid"));
        assert!(root.join("a.mkv").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn scan_lists_root_and_category_clips_but_not_partials_or_deeper_files() {
        let root = temp_root();
        fs::create_dir_all(root.join("Roblox/deeper")).unwrap();
        for f in [
            "old.mkv",
            "Roblox/r.mkv",
            "Roblox/r.partial.mkv",
            "Roblox/deeper/x.mkv",
            "Roblox/notes.txt",
        ] {
            fs::write(root.join(f), b"x").unwrap();
        }
        let mut ids: Vec<_> = scan(&root)
            .unwrap()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["Roblox/r.mkv", "old.mkv"]);
        assert_eq!(category_of("Roblox/r.mkv"), "Roblox");
        assert_eq!(category_of("old.mkv"), UNSORTED);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_are_sanitized_and_never_overwrite() {
        assert_eq!(sanitize_component("Counter-Strike 2"), "Counter-Strike 2");
        assert_eq!(
            sanitize_component("Tom Clancy's: Rainbow* Six?"),
            "Tom Clancy's Rainbow Six"
        );
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("..."), "_");
        assert!(valid_component(&sanitize_component("  weird / name.  ")));
        let root = temp_root();
        fs::write(root.join("Game_1.mkv"), b"x").unwrap();
        assert_eq!(
            unique_path(&root, "Game_1", "mkv"),
            root.join("Game_1_2.mkv")
        );
        assert_eq!(unique_path(&root, "Game_2", "mkv"), root.join("Game_2.mkv"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn index_probes_only_new_or_changed_clips_and_survives_restart() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        let root = temp_root();
        let store = root.join("index.json");
        let clips = root.join("clips");
        fs::create_dir_all(clips.join("Roblox")).unwrap();
        for i in 0..30 {
            fs::write(clips.join(format!("Roblox/r{i}.mkv")), b"x").unwrap();
        }
        fs::write(clips.join("Replay-old.mkv"), b"x").unwrap();
        let calls = AtomicUsize::new(0);
        let fake = |_: &Path| {
            calls.fetch_add(1, SeqCst);
            None
        };
        let mut index = Index::load(&store);
        let list = index.list(&clips, 4, fake).unwrap();
        assert_eq!(list.len(), 31);
        assert_eq!(calls.load(SeqCst), 31);
        let old = list.iter().find(|c| c.id == "Replay-old.mkv").unwrap();
        assert_eq!(
            (old.kind.as_str(), old.category.as_str()),
            ("Replay", UNSORTED)
        );
        assert_eq!(list.iter().filter(|c| c.category == "Roblox").count(), 30);
        index.save(&store).unwrap();
        // A restart reloads the index: nothing is probed again.
        let mut reloaded = Index::load(&store);
        reloaded.list(&clips, 4, fake).unwrap();
        assert_eq!(calls.load(SeqCst), 31);
        // Changing one file re-probes only that file; removed files drop out.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(clips.join("Roblox/r0.mkv"), b"changed").unwrap();
        fs::remove_file(clips.join("Roblox/r1.mkv")).unwrap();
        let list = reloaded.list(&clips, 4, fake).unwrap();
        assert_eq!(calls.load(SeqCst), 32);
        assert_eq!(list.len(), 30);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn move_to_category_changes_folder_and_keeps_the_file_name() {
        let root = temp_root();
        fs::write(root.join("Desktop_x.mkv"), b"x").unwrap();
        let id = move_to_category(&root, "Desktop_x.mkv", "Fortnite").unwrap();
        assert_eq!(id, "Fortnite/Desktop_x.mkv");
        assert!(root.join("Fortnite/Desktop_x.mkv").exists());
        assert!(move_to_category(&root, &id, UNSORTED).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}

/// Thumbnail cache file for a clip: readable prefix, a hash of the full id
/// (two clips never share a file), size and modification time (a replaced or
/// re-saved clip gets a new image), and the generator version.
pub fn thumbnail_file_name(id: &str, size: u64, modified: u64) -> String {
    // FNV-1a 64: stable across runs and builds (unlike the std hasher).
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    let prefix: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .take(40)
        .collect();
    format!("v2-{prefix}-{h:016x}-{modified}-{size}.jpg")
}

#[cfg(test)]
mod thumbnail_tests {
    use super::thumbnail_file_name;
    #[test]
    fn thumbnail_cache_names_follow_clip_identity() {
        let long_a = format!("Roblox/{}A.mkv", "x".repeat(120));
        let long_b = format!("Roblox/{}B.mkv", "x".repeat(120));
        assert_ne!(
            thumbnail_file_name(&long_a, 10, 1),
            thumbnail_file_name(&long_b, 10, 1),
            "long ids never collide"
        );
        assert_ne!(
            thumbnail_file_name("a.mkv", 10, 1),
            thumbnail_file_name("a.mkv", 11, 1),
            "re-saved file, new image"
        );
        assert_ne!(
            thumbnail_file_name("a.mkv", 10, 1),
            thumbnail_file_name("a.mkv", 10, 2),
            "modified file, new image"
        );
        assert_ne!(
            thumbnail_file_name("Roblox/a.mkv", 10, 1),
            thumbnail_file_name("Fortnite/a.mkv", 10, 1),
            "moved to another game"
        );
        assert_eq!(
            thumbnail_file_name("a.mkv", 10, 1),
            thumbnail_file_name("a.mkv", 10, 1)
        );
        assert!(thumbnail_file_name("Roblox/a b.mkv", 1, 1).starts_with("v2-Roblox_a_b_mkv-"));
    }
}
