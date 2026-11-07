use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub path: PathBuf,
    pub start: f64,
    pub end: f64,
    pub bytes: u64,
    /// Original buffer file this segment (or its hard link) came from.
    #[serde(default)]
    pub origin: Option<PathBuf>,
    /// Container start time, if already probed in the background.
    #[serde(default)]
    pub start_time: Option<f64>,
}
#[derive(Default)]
pub struct Ring {
    pub segments: VecDeque<Segment>,
    pub total_bytes: u64,
}
impl Ring {
    pub fn add(&mut self, s: Segment) {
        self.total_bytes += s.bytes;
        self.segments.push_back(s);
    }
    pub fn duration(&self) -> f64 {
        self.segments
            .front()
            .zip(self.segments.back())
            .map(|(a, b)| b.end - a.start)
            .unwrap_or(0.0)
    }
    pub fn prune(&mut self, seconds: f64, limit: u64) -> Result<()> {
        while self.segments.len() > 1 {
            let first = &self.segments[0];
            let newest = self.segments.back().unwrap();
            if newest.end - first.end < seconds && self.total_bytes <= limit {
                break;
            }
            fs::remove_file(&first.path)?;
            let removed = self.segments.pop_front().unwrap();
            self.total_bytes -= removed.bytes;
        }
        if self.total_bytes > limit {
            bail!("A single segment exceeds the buffer storage budget; lower bitrate or increase budget")
        }
        Ok(())
    }
    pub fn select(&self, seconds: f64) -> Vec<Segment> {
        let end = self.segments.back().map(|s| s.end).unwrap_or(0.0);
        self.segments
            .iter()
            .filter(|s| s.end > end - seconds)
            .cloned()
            .collect()
    }
    pub fn snapshot(&self, seconds: f64, dir: &Path) -> Result<Vec<Segment>> {
        snapshot(&self.select(seconds), dir)
    }
}
pub fn snapshot(segments: &[Segment], dir: &Path) -> Result<Vec<Segment>> {
    fs::create_dir_all(dir)?;
    let mut result: Vec<Segment> = Vec::new();
    for (i, s) in segments.iter().enumerate() {
        let path = dir.join(format!("{i:08}.mkv"));
        // A hard link pins the data even if retention unlinks the buffer entry.
        if fs::hard_link(&s.path, &path).is_err() {
            if let Err(error) = fs::copy(&s.path, &path) {
                // Roll back only files made by this snapshot, preserving every source.
                let _ = fs::remove_file(&path);
                for pinned in &result {
                    let _ = fs::remove_file(&pinned.path);
                }
                let _ = fs::remove_dir(dir);
                return Err(error.into());
            }
        }
        result.push(Segment {
            path,
            origin: s.origin.clone().or_else(|| Some(s.path.clone())),
            ..s.clone()
        });
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ring(d: &Path) -> Ring {
        let mut r = Ring::default();
        for i in 0..10 {
            let p = d.join(format!("{i}.mkv"));
            fs::write(&p, [1; 10]).unwrap();
            r.add(Segment {
                path: p,
                start: i as f64 * 2.,
                end: (i + 1) as f64 * 2.,
                bytes: 10,
                origin: None,
                start_time: None,
            });
        }
        r
    }
    #[test]
    fn duration_rotation() {
        let d = tempfile::tempdir().unwrap();
        let mut r = ring(d.path());
        r.prune(6., 1000).unwrap();
        assert_eq!(r.segments.len(), 3);
        assert_eq!(r.duration(), 6.);
    }
    #[test]
    fn disk_rotation() {
        let d = tempfile::tempdir().unwrap();
        let mut r = ring(d.path());
        r.prune(60., 25).unwrap();
        assert_eq!(r.total_bytes, 20);
    }
    #[test]
    fn boundary_selection() {
        let d = tempfile::tempdir().unwrap();
        let r = ring(d.path());
        assert_eq!(r.select(4.).len(), 2);
        assert_eq!(r.select(5.).len(), 3);
    }
    #[test]
    fn concurrent_saves_pin_segments() {
        let d = tempfile::tempdir().unwrap();
        let mut r = ring(d.path());
        let a = r.snapshot(8., &d.path().join("a")).unwrap();
        let b = r.snapshot(6., &d.path().join("b")).unwrap();
        r.prune(2., 10).unwrap();
        for s in a.iter().chain(&b) {
            assert_eq!(fs::read(&s.path).unwrap().len(), 10);
        }
    }
    #[test]
    fn budget_too_small_errors() {
        let d = tempfile::tempdir().unwrap();
        assert!(ring(d.path()).prune(60., 1).is_err());
    }
    #[test]
    fn failed_snapshot_keeps_sources_and_removes_partial_copy() {
        let d = tempfile::tempdir().unwrap();
        let r = ring(d.path());
        let mut selected = r.select(4.);
        selected[1].path = d.path().join("missing.mkv");
        let out = d.path().join("failed");
        assert!(snapshot(&selected, &out).is_err());
        assert!(!out.exists());
        assert!(selected[0].path.exists());
    }
    #[test]
    fn long_rotation_and_overlapping_snapshots_stay_bounded() {
        let d = tempfile::tempdir().unwrap();
        let mut r = Ring::default();
        let mut saves = Vec::new();
        for n in 0..1800 {
            let path = d.path().join(format!("{n}.mkv"));
            fs::write(&path, [7u8; 32]).unwrap();
            r.add(Segment {
                path,
                start: n as f64 * 2.,
                end: (n + 1) as f64 * 2.,
                bytes: 32,
                origin: None,
                start_time: None,
            });
            r.prune(60., 960).unwrap();
            assert!(r.total_bytes <= 960);
            assert!(r.duration() <= 60.);
            if n % 300 == 299 {
                saves.push(r.snapshot(30., &d.path().join(format!("save{n}"))).unwrap());
            }
        }
        for snapshot in saves {
            for s in snapshot {
                assert_eq!(fs::read(s.path).unwrap(), [7u8; 32]);
            }
        }
    }
}
