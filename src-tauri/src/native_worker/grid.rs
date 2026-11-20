//! Causal CFR grid: choose the newest *captured* frame valid at each grid time.
//! Every arrival is processed before deciding what can be retired. One pending
//! capture is retained, including captures which filled no previous output slot.
//! A later frame seals earlier grid times; a bounded watermark handles idleness.
//!
//! With a phase lock (`Grid::with_phase_lock`), each slot's selection deadline
//! is its grid time plus the lock's offset, chosen causally from where new
//! pictures arrived (see `phase.rs`). Output PTS stay exactly n/fps.
use super::phase::PhaseLock;
use anyhow::{bail, Result};
const HNS: i64 = 10_000_000;
const MAX_GAP: i64 = 5 * HNS;
/// Rational CFR clock: no accumulation of truncated per-frame durations.
pub fn offset(fps: u32, slot: i64) -> i64 {
    ((slot as i128 * HNS as i128) / fps as i128) as i64
}
#[derive(Debug)]
pub struct Grid {
    fps: i64,
    pub origin: Option<i64>,
    last: Option<(i64, usize)>,
    next: i64,
    pub repeats: u64,
    pub late: u64,
    generation: u64,
    generation_started: Option<i64>,
    last_pick_generation: Option<u64>,
    last_pick_run: u32,
    pub late_recoveries: u64,
    pub lock: Option<PhaseLock>,
    /// Captures pushed but not yet given to the lock (newest last); they are
    /// given when the next slot is sealed, so slot n's deadline uses only
    /// arrivals at or before slot n-1's deadline.
    unfed: Vec<(i64, bool)>,
    /// Deadline of slot `next` (fixed when slot `next - 1` was sealed).
    deadline: Option<i64>,
    /// Deadline each sealed slot used, for diagnostics (newest only).
    pub last_deadline: Option<i64>,
    sealed_deadlines: Vec<i64>,
    idle_not_before: Option<i64>,
    pub idle_deferrals: u64,
}
impl Grid {
    pub fn new(fps: u32) -> Self {
        Self {
            fps: fps as i64,
            origin: None,
            last: None,
            next: 0,
            repeats: 0,
            late: 0,
            generation: 0,
            generation_started: None,
            last_pick_generation: None,
            last_pick_run: 0,
            late_recoveries: 0,
            lock: None,
            unfed: Vec::new(),
            deadline: None,
            last_deadline: None,
            sealed_deadlines: Vec::new(),
            idle_not_before: None,
            idle_deferrals: 0,
        }
    }
    /// `hinted`: `push_hinted` will say which captures show a new picture.
    pub fn with_phase_lock(fps: u32, hinted: bool) -> Self {
        Self {
            lock: Some(PhaseLock::new(fps, hinted)),
            ..Self::new(fps)
        }
    }
    /// Grid time of a slot (its PTS relative to the origin is offset(slot)).
    pub fn time(&self, slot: i64) -> i64 {
        self.origin.unwrap() + offset(self.fps as u32, slot)
    }
    fn deadline_for(&self, slot: i64) -> i64 {
        let grid = self.time(slot);
        match &self.lock {
            // floor: a capture at integer time t is selected iff t <= deadline.
            Some(lock) => (grid as f64 + lock.delta).floor() as i64,
            None => grid,
        }
    }
    /// Selection deadline of the next unsealed slot.
    pub fn next_time(&self) -> Option<i64> {
        self.origin.map(|_| {
            self.deadline
                .unwrap_or_else(|| self.deadline_for(self.next))
        })
    }
    pub fn next_idle_time(&self) -> Option<i64> {
        self.next_time()
            .map(|t| self.idle_not_before.map_or(t, |wait| wait.max(t)))
    }
    /// Deadline of a slot in the most recently returned batch (at most 301).
    pub fn sealed_deadline(&self, slot: i64) -> Option<i64> {
        let first = self.next - self.sealed_deadlines.len() as i64;
        usize::try_from(slot - first)
            .ok()
            .and_then(|i| self.sealed_deadlines.get(i))
            .copied()
    }
    fn seal(&mut self, out: &mut Vec<(i64, usize)>, pick: usize, generation: u64) {
        self.last_pick_run = if self.last_pick_generation == Some(generation) {
            self.last_pick_run.saturating_add(1)
        } else {
            1
        };
        self.last_pick_generation = Some(generation);
        self.last_deadline = self.next_time();
        self.sealed_deadlines.push(self.last_deadline.unwrap());
        out.push((self.next, pick));
        self.next += 1;
        if let Some(lock) = &mut self.lock {
            for (t, changed) in self.unfed.drain(..) {
                if changed {
                    lock.arrival(t);
                }
            }
            lock.update();
        }
        self.deadline = Some(
            self.deadline_for(self.next)
                .max(self.last_deadline.unwrap() + 1),
        );
    }
    #[cfg(test)]
    pub fn push(&mut self, time: i64, index: usize) -> Result<Vec<(i64, usize)>> {
        self.push_hinted(time, index, true)
    }
    /// `changed`: the capture shows a new picture (for the phase lock; pass
    /// true when the platform gives no change information).
    #[cfg(test)]
    pub fn push_hinted(
        &mut self,
        time: i64,
        index: usize,
        changed: bool,
    ) -> Result<Vec<(i64, usize)>> {
        self.push_observed(time, index, changed, changed)
    }
    pub fn push_observed(
        &mut self,
        time: i64,
        index: usize,
        changed: bool,
        phase_changed: bool,
    ) -> Result<Vec<(i64, usize)>> {
        self.sealed_deadlines.clear();
        self.idle_not_before = None;
        if let Some((last, _)) = self.last {
            if time < last || time - last > MAX_GAP {
                bail!("Native capture clock reversed or stalled for over five seconds; completed footage preserved")
            }
        }
        let first = self.origin.is_none();
        if first {
            self.origin = Some(time);
            self.deadline = Some(self.deadline_for(0));
            if let Some(lock) = &mut self.lock {
                lock.start(time);
            }
        }
        let next_generation = self.generation + u64::from(changed && !first);
        let mut out = Vec::new();
        while self.next_time().unwrap() < time {
            let deadline = self.next_time().unwrap();
            let would_repeat = self.last_pick_generation == Some(self.generation);
            let max_lead = offset(self.fps as u32, 1) * 5 / 4;
            let nominal_hold = self
                .lock
                .as_ref()
                .and_then(|l| l.source_nominal_hold_slots());
            if would_repeat
                && nominal_hold.is_some_and(|n| self.last_pick_run >= n)
                && time <= self.time(self.next) + max_lead
                && (!changed || time - deadline < deadline - self.generation_started.unwrap())
            {
                // Do not seal a repeated picture merely because another
                // unchanged capture crossed the nominal deadline. Wait within
                // the SAME bounded phase window for a genuinely new picture.
                // Compare picture age, not the latest redundant capture's age.
                // Equal-time updates still choose the latest capture.
                self.deadline = Some(time);
                if changed {
                    self.late_recoveries += 1;
                }
                break;
            }
            let pick = self.last.unwrap().1;
            self.seal(&mut out, pick, self.generation);
        }
        if self.next > 1 && self.last_deadline.is_some_and(|d| time < d) {
            self.late += 1;
        }
        self.last = Some((time, index));
        self.generation = next_generation;
        if changed || first {
            self.generation_started = Some(time);
        }
        if self.lock.is_some() {
            // The first capture is always a new picture.
            self.unfed.push((time, phase_changed || first));
            if self.unfed.len() > 4096 {
                bail!("Phase lock received captures without a sealed slot for too long")
            }
        }
        Ok(out)
    }
    pub fn advance(&mut self, end: i64) -> Result<Vec<(i64, usize)>> {
        self.advance_inner(end, false)
    }
    pub fn advance_idle(&mut self, end: i64) -> Result<Vec<(i64, usize)>> {
        self.advance_inner(end, true)
    }
    fn advance_inner(&mut self, end: i64, idle: bool) -> Result<Vec<(i64, usize)>> {
        self.sealed_deadlines.clear();
        self.idle_not_before = None;
        let Some((last, index)) = self.last else {
            return Ok(Vec::new());
        };
        if end - last > MAX_GAP {
            bail!("Native capture stopped delivering frames for over five seconds; completed footage preserved")
        }
        let mut out = Vec::new();
        while self.next_time().unwrap() < end {
            let hold = self
                .lock
                .as_ref()
                .and_then(|l| l.source_nominal_hold_slots());
            let bound = self.time(self.next) + offset(self.fps as u32, 1) * 5 / 4;
            if idle
                && self.last_pick_generation == Some(self.generation)
                && hold.is_some_and(|n| self.last_pick_run >= n)
                && end < bound
            {
                // Keep the nominal deadline for a forced final cut. Only the
                // wake time changes, so there is no spin or retimed output.
                self.idle_not_before = Some(bound);
                self.idle_deferrals += 1;
                break;
            }
            self.seal(&mut out, index, self.generation);
            self.repeats += 1;
        }
        Ok(out)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn check(times: &[i64], end: i64) {
        let mut s = Grid::new(60);
        let mut got = Vec::new();
        for (i, &t) in times.iter().enumerate() {
            got.extend(s.push(t, i).unwrap());
        }
        got.extend(s.advance(end).unwrap());
        for (slot, pick) in &got {
            let target = times[0] + slot * HNS / 60;
            let expected = times.iter().rposition(|&t| t <= target).unwrap();
            assert_eq!(*pick, expected, "slot {slot}");
        }
        assert_eq!(
            got.iter().map(|x| x.0).collect::<Vec<_>>(),
            (0..got.len() as i64).collect::<Vec<_>>()
        );
    }
    #[test]
    fn all_refresh_rates_and_phases_follow_independent_oracle() {
        for hz in [60, 120, 144, 240, 250, 500] {
            for phase in [0, 100, 70000, 150000] {
                let t: Vec<_> = (0..hz * 3).map(|n| phase + n * HNS / hz).collect();
                check(&t, phase + 3 * HNS);
            }
        }
    }
    #[test]
    fn irregular_delivery_and_gaps_use_last_capture_not_last_output() {
        let mut t: Vec<_> = (0..240)
            .filter(|n| !(80..120).contains(n))
            .map(|n| n * HNS / 120 + (n % 7) * 3000)
            .collect();
        t.sort();
        check(&t, 2 * HNS);
    }
    #[test]
    fn unselected_capture_is_kept_for_gap_recovery() {
        let mut s = Grid::new(60);
        assert!(s.push(0, 0).unwrap().is_empty());
        assert_eq!(s.push(50000, 1).unwrap(), vec![(0, 0)]);
        assert_eq!(s.push(400000, 2).unwrap(), vec![(1, 1), (2, 1)]);
    }
    #[test]
    fn watermark_static_frames_and_clock_errors_are_bounded() {
        let mut s = Grid::new(60);
        s.push(0, 0).unwrap();
        let o = s.advance(HNS).unwrap();
        assert_eq!(o.len(), 60);
        assert!(o.iter().all(|(_, i)| *i == 0));
        assert!(s.advance(6 * HNS).is_err());
        let mut s = Grid::new(60);
        s.push(HNS, 0).unwrap();
        assert!(s.push(0, 1).is_err());
        assert!(s.push(7 * HNS, 1).is_err());
    }
    #[test]
    fn same_timestamp_updates_are_sealed_only_after_the_latest_capture() {
        let mut s = Grid::new(60);
        assert!(s.push(0, 0).unwrap().is_empty());
        assert!(s.push(0, 1).unwrap().is_empty());
        assert_eq!(s.push(200000, 2).unwrap(), vec![(0, 1), (1, 1)]);
    }
    /// Jittered 60 Hz source on a 250 Hz display, phased so that every
    /// arrival lands within 0..4 ms after the fixed grid's deadlines.
    fn worst_phase_source(seconds: i64) -> (Vec<i64>, Vec<usize>) {
        let vb = HNS / 250;
        let (mut times, mut ids) = (Vec::new(), Vec::new());
        for k in 0..seconds * 60 {
            let present = k * HNS / 60 + (k * 7919 % 13) * 1000;
            let t = (present + vb - 1) / vb * vb;
            if times.last().is_some_and(|&l| t <= l) {
                continue;
            }
            times.push(t);
            ids.push(k as usize);
            // a redundant capture one vblank later (other screen activity)
            if k % 3 == 0 {
                times.push(t + vb);
                ids.push(k as usize);
            }
        }
        (times, ids)
    }
    #[test]
    fn phase_lock_selection_is_causal_ordered_and_matches_oracle_at_its_deadlines() {
        let (times, ids) = worst_phase_source(20);
        let changed: Vec<bool> = (0..times.len())
            .map(|i| i == 0 || ids[i] != ids[i - 1])
            .collect();
        let mut g = Grid::with_phase_lock(60, true);
        let mut slots = Vec::new();
        for (i, &t) in times.iter().enumerate() {
            for (slot, pick) in g.push_hinted(t, i, changed[i]).unwrap() {
                slots.push((slot, pick));
            }
            if let (Some(&(_, pick)), Some(d)) = (slots.last(), g.last_deadline) {
                // newest capture at or before the deadline that slot used
                let oracle = times.iter().rposition(|&x| x <= d).unwrap();
                assert_eq!(pick, oracle, "slot {}", slots.len() - 1);
            }
        }
        assert_eq!(
            slots.iter().map(|x| x.0).collect::<Vec<_>>(),
            (0..slots.len() as i64).collect::<Vec<_>>(),
            "PTS slots must stay consecutive"
        );
        assert!(
            slots.windows(2).all(|w| w[1].1 >= w[0].1),
            "never backwards"
        );
    }
    #[test]
    fn phase_lock_shows_a_sixty_hz_source_one_to_one_where_the_fixed_grid_fails() {
        let (times, ids) = worst_phase_source(30);
        let changed: Vec<bool> = (0..times.len())
            .map(|i| i == 0 || ids[i] != ids[i - 1])
            .collect();
        let shown = |lock: bool| {
            let mut g = if lock {
                Grid::with_phase_lock(60, true)
            } else {
                Grid::new(60)
            };
            let mut out = Vec::new();
            for (i, &t) in times.iter().enumerate() {
                out.extend(
                    g.push_hinted(t, i, changed[i])
                        .unwrap()
                        .into_iter()
                        .map(|(_, p)| ids[p]),
                );
            }
            out
        };
        let events = |v: &[usize]| v.windows(2).filter(|w| w[1] != w[0] + 1).count();
        let fixed = shown(false);
        let lock = shown(true);
        assert!(
            events(&fixed) > 30,
            "fixture must reproduce the failure: {}",
            events(&fixed)
        );
        // After the first second (convergence), every source frame exactly once.
        assert_eq!(events(&lock[60..]), 0, "phase lock repeats or skips");
    }
    #[test]
    fn unchanged_captures_do_not_move_the_deadline() {
        let mut g = Grid::with_phase_lock(60, true);
        let mut t = 0;
        for k in 0..600usize {
            g.push_hinted(t, k, k == 0).unwrap();
            t += HNS / 250;
        }
        let lock = g.lock.as_ref().unwrap();
        assert_eq!(lock.moves, 0);
        assert_eq!(lock.delta, HNS as f64 / 60.0 / 2.0);
    }
    #[test]
    fn late_available_picture_replaces_a_repeat_within_the_existing_bound() {
        let mut g = Grid::with_phase_lock(60, true);
        for n in 0..80 {
            g.push_hinted(n * HNS / 60, n as usize, true).unwrap();
        }
        g.advance(g.next_time().unwrap() + 1).unwrap();
        let slot = g.next;
        let late = g.next_time().unwrap() + 30000;
        assert!(late <= g.time(slot) + offset(60, 1) * 5 / 4);
        assert!(g.push_hinted(late, 80, true).unwrap().is_empty());
        // A newer version at the SAME time must win, without an artificial PTS.
        assert!(g.push_hinted(late, 81, true).unwrap().is_empty());
        let out = g.push_hinted(late + 40000, 82, true).unwrap();
        assert_eq!(out[0], (slot, 81));
        assert_eq!(g.sealed_deadline(slot), Some(late));
        assert_eq!(g.late_recoveries, 1);
    }
    #[test]
    fn redundant_capture_does_not_seal_a_slot_before_the_late_picture() {
        let mut g = Grid::with_phase_lock(60, true);
        for n in 0..80 {
            g.push_hinted(n * HNS / 60, n as usize, true).unwrap();
        }
        g.advance(g.next_time().unwrap() + 1).unwrap();
        let slot = g.next;
        let boundary = g.next_time().unwrap();
        // A redundant capture arrives just past the deadline. The old code
        // sealed the stale picture here, then lost the picture 4 ms later.
        assert!(g
            .push_hinted(boundary + 1000, 80, false)
            .unwrap()
            .is_empty());
        assert!(g
            .push_hinted(boundary + 41000, 81, true)
            .unwrap()
            .is_empty());
        let out = g.push_hinted(boundary + 81000, 82, false).unwrap();
        assert_eq!(out[0], (slot, 81));
        assert_eq!(g.late_recoveries, 1);
        // A real gap must still seal slots and recover, with bounded history.
        let out = g.push_hinted(3 * HNS, 83, true).unwrap();
        assert!(out.len() > 60);
        assert!(out.iter().all(|(_, i)| *i == 82));
        assert!(g.unfed.len() <= 4096);
    }
    #[test]
    fn idle_watermark_does_not_close_the_promised_late_recovery_window() {
        let mut g = Grid::with_phase_lock(60, true);
        for n in 0..80 {
            g.push_hinted(n * HNS / 60, n as usize, true).unwrap();
        }
        g.advance(g.next_time().unwrap() + 1).unwrap();
        let slot = g.next;
        let deadline = g.next_time().unwrap();
        assert!(g.advance_idle(deadline + 1).unwrap().is_empty());
        assert!(g.next_idle_time().unwrap() > deadline);
        assert!(g
            .push_hinted(deadline + 10000, 80, true)
            .unwrap()
            .is_empty());
        let out = g.push_hinted(deadline + 50000, 81, false).unwrap();
        assert_eq!(out[0], (slot, 80));
        assert_eq!(g.idle_deferrals, 1);
    }
    #[test]
    fn forced_stop_does_not_wait_or_change_a_nominal_deadline() {
        let mut g = Grid::with_phase_lock(60, true);
        for n in 0..80 {
            g.push_hinted(n * HNS / 60, n as usize, true).unwrap();
        }
        g.advance(g.next_time().unwrap() + 1).unwrap();
        let deadline = g.next_time().unwrap();
        assert!(g.advance_idle(deadline + 1).unwrap().is_empty());
        let out = g.advance(deadline + 1).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(g.sealed_deadline(out[0].0), Some(deadline));
    }
    #[test]
    fn thirty_hz_late_recovery_waits_for_two_output_slots() {
        let mut g = Grid::with_phase_lock(60, true);
        for n in 0..80 {
            g.push_hinted(n * HNS / 30, n as usize, true).unwrap();
        }
        while g.last_pick_generation != Some(g.generation) || g.last_pick_run < 2 {
            g.advance(g.next_time().unwrap() + 1).unwrap();
        }
        let slot = g.next;
        let late = g.next_time().unwrap() + 1000;
        assert!(g.push_hinted(late, 80, true).unwrap().is_empty());
        let out = g.push_hinted(late + 40000, 81, false).unwrap();
        assert_eq!(out[0], (slot, 80));
        assert_eq!(g.late_recoveries, 1);
        // One hold is legitimate at 30 Hz: it must not advance the next picture.
        let slot = g.next;
        assert_eq!(g.last_pick_run, 1);
        let late = g.next_time().unwrap() + 1000;
        let out = g.push_hinted(late, 82, true).unwrap();
        assert_eq!(out[0], (slot, 81));
        assert_eq!(g.late_recoveries, 1);
    }
    #[test]
    fn unsupported_low_rate_and_unhinted_sources_do_not_use_late_recovery() {
        for hinted in [false, true] {
            let mut g = Grid::with_phase_lock(60, hinted);
            for n in 0..120 {
                g.push_hinted(n * HNS / 15, n as usize, true).unwrap();
            }
            assert_eq!(g.late_recoveries, 0);
        }
    }
    #[test]
    fn recovery_batch_reports_each_slots_own_deadline() {
        let mut g = Grid::with_phase_lock(60, true);
        for i in 0..90 {
            g.push_hinted(i * HNS / 60, i as usize, true).unwrap();
        }
        let first = g.next_time().unwrap();
        let batch = g.push_hinted(3 * HNS, 90, true).unwrap();
        assert!(batch.len() > 60);
        let deadlines: Vec<_> = batch
            .iter()
            .map(|(s, _)| g.sealed_deadline(*s).unwrap())
            .collect();
        assert_eq!(deadlines[0], first);
        assert!(deadlines.windows(2).all(|w| w[1] > w[0]));
        assert_eq!(deadlines.last().copied(), g.last_deadline);
        assert!(batch.iter().all(|(_, i)| *i == 89));
    }
    #[test]
    fn sixty_hz_source_is_one_to_one_and_has_no_drift() {
        let mut s = Grid::new(60);
        let mut out = Vec::new();
        for n in 0..600 {
            out.extend(s.push(n * HNS / 60, n as usize).unwrap());
        }
        out.extend(s.advance(600 * HNS / 60).unwrap());
        assert_eq!(out.len(), 600);
        assert!(out.iter().all(|(slot, id)| *slot as usize == *id));
        assert_eq!(s.time(60 * 60 * 60), 3600 * HNS);
    }
}
