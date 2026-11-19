//! Source-clock spans with one pending capture and no content-dependent decisions.
//! TS uses a 90 kHz clock: samples sharing a transport tick have zero separately
//! representable display duration. Keep the latest tie, and count it explicitly.
use anyhow::{bail, Result};
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub capture: usize,
    pub pts: i64,
    pub duration: i64,
}
#[derive(Debug, Default)]
pub struct Timeline {
    origin: Option<i64>,
    last: Option<(i64, usize)>,
    pub coalesced: usize,
}
pub fn tick(time: i64) -> i64 {
    ((time as i128 * 90_000 + 5_000_000) / 10_000_000) as i64
}
impl Timeline {
    pub fn push(&mut self, time: i64, index: usize) -> Result<Option<Span>> {
        let origin = *self.origin.get_or_insert(time);
        let result = if let Some((previous, capture)) = self.last {
            if time < previous || time - previous > 50_000_000 {
                bail!("Capture clock reversed or stalled; completed footage preserved")
            }
            if tick(time - origin) == tick(previous - origin) {
                self.coalesced += 1;
                None
            } else {
                Some(Span {
                    capture,
                    pts: previous - origin,
                    duration: time - previous,
                })
            }
        } else {
            None
        };
        self.last = Some((time, index));
        Ok(result)
    }
    pub fn finish(&mut self, end: i64) -> Result<Option<Span>> {
        let Some((time, capture)) = self.last.take() else {
            return Ok(None);
        };
        if end - time > 50_000_000 {
            bail!("Capture stalled before stop")
        }
        Ok(Some(Span {
            capture,
            pts: time - self.origin.unwrap(),
            duration: (end - time).max(1),
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_spans_preserve_all_rates_and_phases_without_inventing_pictures() {
        for (num, den) in [
            (60i64, 1i64),
            (60000, 1001),
            (120, 1),
            (144, 1),
            (240, 1),
            (250, 1),
        ] {
            for phase in [0, 1, 83333, 166665, 166666, 123456789123] {
                let mut t = Timeline::default();
                let times: Vec<_> = (0..num / den * 3)
                    .map(|n| phase + n * 10_000_000 * den / num)
                    .collect();
                let mut spans = Vec::new();
                for (i, &time) in times.iter().enumerate() {
                    if let Some(s) = t.push(time, i).unwrap() {
                        spans.push(s)
                    }
                }
                spans.extend(
                    t.finish(times.last().unwrap() + 10_000_000 * den / num)
                        .unwrap(),
                );
                assert_eq!(spans.len(), times.len());
                for (i, s) in spans.iter().enumerate() {
                    assert_eq!(s.capture, i);
                    assert_eq!(s.pts, times[i] - phase);
                    assert!(s.duration > 0)
                }
            }
        }
    }
    #[test]
    fn ties_keep_latest_and_are_not_assigned_artificial_duration() {
        let mut t = Timeline::default();
        assert!(t.push(1000, 0).unwrap().is_none());
        assert!(t.push(1000, 1).unwrap().is_none());
        assert!(t.push(1001, 2).unwrap().is_none());
        let s = t.push(2000, 3).unwrap().unwrap();
        assert_eq!(s.capture, 2);
        assert_eq!(s.pts, 1);
        assert_eq!(t.coalesced, 2);
    }
    #[test]
    fn gaps_drift_and_arrival_delay_do_not_rewrite_the_source_clock() {
        let mut t = Timeline::default();
        let mut last = None;
        for i in 0..20000 {
            let time = 99999 + i * 166667 + i / 4 + if i >= 10000 { 4_000_000 } else { 0 };
            if let Some(s) = t.push(time, i as usize).unwrap() {
                if let Some(prev) = last {
                    assert_eq!(prev, s.pts)
                }
                last = Some(s.pts + s.duration)
            }
        }
        assert_eq!(t.coalesced, 0);
        assert!(t.push(0, 20001).is_err());
    }
    #[test]
    fn full_output_interval_translation_sweep_is_invariant() {
        let times = [0, 0, 1, 40000, 160000, 320000, 480000, 520000, 680000];
        for phase in 0..166667 {
            let mut timeline = Timeline::default();
            let mut got = Vec::new();
            for (i, t) in times.iter().enumerate() {
                if let Some(s) = timeline.push(phase + t, i).unwrap() {
                    got.push(s.capture)
                }
            }
            got.extend(timeline.finish(phase + 850000).unwrap().map(|s| s.capture));
            assert_eq!(got, vec![2, 3, 4, 5, 6, 7, 8]);
        }
    }
    #[test]
    fn a_full_day_has_no_accumulated_timestamp_rounding() {
        let mut t = Timeline::default();
        let origin = 123456789123i64;
        for i in 0..60 * 86400i64 {
            let time = origin + i * 10_000_000 / 60;
            if let Some(s) = t.push(time, i as usize).unwrap() {
                assert_eq!(s.pts, (i - 1) * 10_000_000 / 60)
            }
        }
        let end = t.finish(origin + 86400 * 10_000_000i64).unwrap().unwrap();
        assert_eq!(end.pts + end.duration, 86400 * 10_000_000i64);
    }
    #[test]
    fn stop_is_bounded_and_idempotent() {
        let mut t = Timeline::default();
        t.push(42, 0).unwrap();
        assert_eq!(t.finish(41).unwrap().unwrap().duration, 1);
        assert!(t.finish(500).unwrap().is_none());
        let mut t = Timeline::default();
        t.push(0, 0).unwrap();
        assert!(t.push(50_000_001, 1).is_err());
    }
}
