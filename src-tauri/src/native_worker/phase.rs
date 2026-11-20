//! Source-aware selection deadline for the 60 FPS CFR grid ("phase lock").
//!
//! The grid shows, in output slot n (PTS exactly n/60 s), the newest capture
//! at or before a deadline. With the deadline at a fixed offset from the first
//! capture, a ~60 Hz source whose pictures arrive near the deadlines (compositor
//! quantization and jitter) alternates a repeat and a skip; which phases fail
//! depends on the display, the game and chance, so no fixed offset is correct.
//!
//! This keeps the deadline where it would have lost the fewest new pictures,
//! then where new pictures do not arrive:
//! - It keeps the last `KEEP` new-picture arrivals seen at or before the
//!   previous slot's deadline (causal: never re-decides a sealed slot), as
//!   phases mod one output period in `BINS` bins.
//! - lost(phase): how many of them that deadline phase would have lost (two
//!   new pictures inside one slot). Only meaningful when captures say which
//!   are new pictures (`hinted`); without hints it is not used.
//! - density(phase) = sum over arrivals of a triangle of half-width `WIDTH_*`
//!   bins (wider with hints, see the constants). Late compositor frames,
//!   which only ever arrive later than their cluster, add cost where they land.
//! - Hinted: if the current phase would have lost max(`LOST_HYSTERESIS`,
//!   best / `LOST_RELATIVE`) more pictures than the best phase, move to the
//!   best-loss phases' lowest density. Otherwise, and always without hints, move only when the current
//!   phase's density exceeds the lowest (among phases losing no more) by
//!   `HYSTERESIS`, and only if there is structure (lowest <= `STRUCTURE` x
//!   mean); 50 Hz, faster-than-60 Hz and static sources hold.
//! - Moves go to the middle of the longest run of equal best bins. Moving
//!   inside an empty gap crosses no arrival, so the move adds no repeat/skip.
//! - The deadline offset stays in [-T/4, 5T/4): leaving it wraps by one
//!   period, which is exactly the one repeat (skip) that a slower (faster)
//!   source needs anyway, e.g. one per 1001 frames at 59.94 Hz.
//!
//! Output timestamps are not touched. Memory is fixed (`KEEP` arrivals) and
//! the work is at most 128 x (65 + 60) operations per output slot.
//! `scripts/bench/cfr-scheduler-eval.py` (`LexLock` with hints, `DensityLock`
//! without) is the reference model; `--pacing-replay` checks this port
//! against it on recorded traces (`phase-lock-crosscheck.py`).
use std::collections::VecDeque;

pub const BINS: usize = 128;
pub const KEEP: usize = 60;
/// Kernel half-width when each capture says whether it is a new picture
/// (about 4.2 ms: keeps a margin from clean arrival clusters).
pub const WIDTH_HINTED: usize = 32;
/// Kernel half-width when every capture counts (about 1 ms): redundant
/// captures fill most phases, so only avoid where captures really land.
pub const WIDTH_TIMESTAMPS: usize = 8;
pub const STRUCTURE: f64 = 0.2;
pub const HYSTERESIS: f64 = 0.5;
/// With hints: move when the current phase would have lost at least
/// max(LOST_HYSTERESIS, best / LOST_RELATIVE) more of the kept pictures than
/// the best phase. The relative part keeps faster-than-60 Hz sources, which
/// lose most pictures at every phase, from chasing one-picture differences.
pub const LOST_HYSTERESIS: usize = 1;
pub const LOST_RELATIVE: f64 = 4.0;
const MIN_ARRIVALS: usize = 8;
const EPS: f64 = 1e-9;

#[derive(Debug)]
pub struct PhaseLock {
    period: f64,
    origin: Option<i64>,
    ring: VecDeque<usize>,
    /// Arrival times relative to the origin (same arrivals as `ring`).
    times: VecDeque<i64>,
    hinted: bool,
    width: usize,
    kernel: Vec<f64>,
    /// Deadline offset from the slot's grid time, in 100 ns units.
    pub delta: f64,
    dirty: bool,
    pub moves: u64,
    pub wraps: u64,
    pub held_without_structure: u64,
}

impl PhaseLock {
    /// `hinted`: arrivals are only captures known to show a new picture.
    pub fn new(fps: u32, hinted: bool) -> Self {
        let period = 10_000_000.0 / fps as f64;
        let width = if hinted {
            WIDTH_HINTED
        } else {
            WIDTH_TIMESTAMPS
        };
        let kernel = (-(width as i64)..=width as i64)
            .map(|j| (1.0 - j.abs() as f64 / (width + 1) as f64).max(0.0))
            .collect();
        Self {
            period,
            origin: None,
            ring: VecDeque::with_capacity(KEEP + 1),
            times: VecDeque::with_capacity(KEEP + 1),
            hinted,
            width,
            kernel,
            // The first capture is a new picture at phase 0: start half a
            // period away from it, then follow what arrives.
            delta: period / 2.0,
            dirty: false,
            moves: 0,
            wraps: 0,
            held_without_structure: 0,
        }
    }

    /// The grid's origin (its first capture): phases and `delta` are relative to it.
    pub fn start(&mut self, origin: i64) {
        self.origin.get_or_insert(origin);
    }

    /// Nominal output slots per source picture for late-picture recovery.
    /// Only actual change hints and near-60/30 Hz cadences qualify. A slower
    /// source must already have occupied its nominal slots before recovery.
    pub fn source_nominal_hold_slots(&self) -> Option<u32> {
        if !self.hinted || self.times.len() < MIN_ARRIVALS {
            return None;
        }
        let mut intervals = [0i64; KEEP];
        let mut n = 0;
        let mut previous = None;
        for &t in &self.times {
            if let Some(p) = previous {
                if t > p {
                    intervals[n] = t - p;
                    n += 1;
                }
            }
            previous = Some(t);
        }
        if n < MIN_ARRIVALS - 1 {
            return None;
        }
        let (_, median, _) = intervals[..n].select_nth_unstable(n / 2);
        let ratio = *median as f64 / self.period;
        if (0.75..=1.25).contains(&ratio) {
            Some(1)
        } else if (1.75..=2.25).contains(&ratio) {
            Some(2)
        } else {
            None
        }
    }

    /// A spatial source switch replaces observation history, not the video/audio
    /// origin or a sealed output deadline.
    pub fn reseed(&mut self, times: &[i64]) {
        self.ring.clear();
        self.times.clear();
        self.dirty = false;
        for &time in times {
            self.arrival(time);
        }
        self.update();
    }

    pub fn absolute_observations(&self) -> Vec<i64> {
        let origin = self.origin.unwrap_or(0);
        self.times.iter().map(|t| origin + t).collect()
    }
    /// Diagnostics restore an exact saved state, without rerunning hysteresis.
    pub fn restore(&mut self, origin: i64, delta: f64, times: &[i64]) -> bool {
        if times.len() > KEEP
            || !delta.is_finite()
            || delta < -0.25 * self.period
            || delta >= 1.25 * self.period
            || times.windows(2).any(|w| w[1] < w[0])
        {
            return false;
        }
        self.ring.clear();
        self.times.clear();
        self.origin = Some(origin);
        self.delta = delta;
        for &time in times {
            self.arrival(time);
        }
        self.dirty = false;
        true
    }

    /// Enough observations for the existing estimator; used only before frame zero.
    pub fn ready(&self) -> bool {
        self.ring.len() >= MIN_ARRIVALS
    }
    /// Rebase a learned phase BEFORE recording begins. Preserve the absolute
    /// deadline phase, and start with a nonnegative first deadline. Output/audio
    /// frame zero will both use this new origin, not the calibration interval.
    pub fn rebase_before_start(&mut self, origin: i64) {
        let old = self.origin.unwrap_or(origin);
        let shift = origin - old;
        self.delta = (self.delta - shift as f64).rem_euclid(self.period);
        for t in &mut self.times {
            *t -= shift;
        }
        self.ring = self
            .times
            .iter()
            .map(|t| {
                (((*t as f64).rem_euclid(self.period) * BINS as f64 / self.period) as usize) % BINS
            })
            .collect();
        self.origin = Some(origin);
        self.dirty = true;
    }

    /// A capture that showed a new picture (or every capture when the
    /// platform reports no change information).
    pub fn arrival(&mut self, time: i64) {
        let origin = *self.origin.get_or_insert(time);
        let phase = ((time - origin) as f64).rem_euclid(self.period);
        self.ring
            .push_back((phase * BINS as f64 / self.period) as usize % BINS);
        self.times.push_back(time - origin);
        if self.ring.len() > KEEP {
            self.ring.pop_front();
            self.times.pop_front();
        }
        self.dirty = true;
    }

    fn cost(&self) -> [f64; BINS] {
        let mut hist = [0u32; BINS];
        for &b in &self.ring {
            hist[b] += 1;
        }
        let mut c = [0.0; BINS];
        for (b, &n) in hist.iter().enumerate() {
            if n != 0 {
                for (j, w) in self.kernel.iter().enumerate() {
                    c[(b + BINS + j - self.width) % BINS] += n as f64 * w;
                }
            }
        }
        c
    }

    /// Pictures each deadline phase would have lost on the kept arrivals.
    fn lost(&self) -> [usize; BINS] {
        let mut out = [0; BINS];
        for (b, o) in out.iter_mut().enumerate() {
            let phi = (b as f64 + 0.5) * self.period / BINS as f64;
            let mut slots = 0;
            let mut last = None;
            for &t in &self.times {
                let slot = ((t as f64 - phi) / self.period).ceil() as i64;
                if last != Some(slot) {
                    slots += 1;
                    last = Some(slot);
                }
            }
            *o = self.times.len() - slots;
        }
        out
    }

    /// Recompute the offset for the next slot from the arrivals seen so far.
    pub fn update(&mut self) {
        if !self.dirty || self.ring.len() < MIN_ARRIVALS {
            return;
        }
        self.dirty = false;
        let dens = self.cost();
        let cur = (self.delta.rem_euclid(self.period) * BINS as f64 / self.period) as usize % BINS;
        let lost = if self.hinted { self.lost() } else { [0; BINS] };
        let best_lost = *lost.iter().min().unwrap();
        let threshold = (LOST_HYSTERESIS as f64).max(best_lost as f64 / LOST_RELATIVE);
        let allowed: [bool; BINS] = if lost[cur] as f64 >= best_lost as f64 + threshold {
            std::array::from_fn(|b| lost[b] == best_lost)
        } else {
            let allowed = std::array::from_fn(|b| lost[b] <= lost[cur]);
            let lo = (0..BINS)
                .filter(|&b| allowed[b])
                .map(|b| dens[b])
                .fold(f64::INFINITY, f64::min);
            let mean = dens.iter().sum::<f64>() / BINS as f64;
            if lo > STRUCTURE * mean {
                self.held_without_structure += 1;
                return;
            }
            if dens[cur] <= lo + HYSTERESIS {
                return;
            }
            allowed
        };
        let c: [f64; BINS] =
            std::array::from_fn(|b| if allowed[b] { dens[b] } else { f64::INFINITY });
        let lo = c.iter().copied().fold(f64::INFINITY, f64::min);
        let mut best: Option<(usize, f64)> = None;
        for b in 0..BINS {
            if c[b] <= lo + EPS && c[(b + BINS - 1) % BINS] > lo + EPS {
                let mut e = b;
                while c[(e + 1) % BINS] <= lo + EPS && (e + 1) % BINS != b {
                    e += 1;
                }
                let run = e - b + 1;
                let mid = (b as f64 + (run - 1) as f64 / 2.0 + 0.5) * self.period / BINS as f64;
                if best.is_none_or(|(r, _)| run > r) {
                    best = Some((run, mid));
                }
            }
        }
        let target = best.map_or(self.delta.rem_euclid(self.period), |(_, mid)| mid);
        let shift = ((self.delta - target) / self.period).round_ties_even() * self.period;
        self.delta = target + shift;
        self.moves += 1;
        if self.delta >= 1.25 * self.period {
            self.delta -= self.period;
            self.wraps += 1;
        } else if self.delta < -0.25 * self.period {
            self.delta += self.period;
            self.wraps += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const HNS: i64 = 10_000_000;

    /// Arrivals of a source at `fps` shown on a `hz` display: the next vblank.
    fn arrivals(fps: f64, hz: f64, seconds: f64, phase: f64) -> Vec<i64> {
        let vb = HNS as f64 / hz;
        (0..(seconds * fps) as i64)
            .map(|k| {
                let t = phase + k as f64 * HNS as f64 / fps;
                ((t / vb).ceil() * vb) as i64
            })
            .collect()
    }

    fn margin(lock: &PhaseLock, times: &[i64]) -> f64 {
        let o = times[0];
        let t = lock.period;
        times
            .iter()
            .rev()
            .take(KEEP)
            .map(|&a| {
                let d = ((a - o) as f64 - lock.delta).rem_euclid(t);
                d.min(t - d)
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[test]
    fn sixty_hz_source_deadline_settles_in_the_gap_on_common_displays() {
        for hz in [60.0, 144.0, 165.0, 240.0, 250.0] {
            for phase in [0.0, 13_000.0, 80_000.0, 160_000.0] {
                let t = arrivals(60.0, hz, 4.0, phase);
                let mut lock = PhaseLock::new(60, true);
                for &a in &t {
                    lock.arrival(a);
                    lock.update();
                }
                // At least 2 ms from every arrival (the 144 Hz cluster is 6.9 ms wide).
                assert!(
                    margin(&lock, &t) >= 20_000.0,
                    "{hz} Hz phase {phase}: {}",
                    margin(&lock, &t)
                );
                assert!(
                    lock.moves <= 3,
                    "{hz} Hz phase {phase}: {} moves",
                    lock.moves
                );
            }
        }
    }

    #[test]
    fn no_structure_holds_the_deadline() {
        for fps in [50.0, 144.0, 300.0] {
            let t = arrivals(fps, 250.0, 4.0, 7_000.0);
            let mut lock = PhaseLock::new(60, true);
            for &a in &t {
                lock.arrival(a);
                lock.update();
            }
            assert!(lock.moves <= 1, "{fps}: {} moves", lock.moves);
            assert_eq!(lock.wraps, 0);
        }
    }

    #[test]
    fn slower_source_wraps_once_per_beat_and_offset_stays_bounded() {
        // 59.94 Hz into 60: the arrivals slip one full period every 1001 frames.
        let t = arrivals(60_000.0 / 1001.0, 60_000.0 / 1001.0, 120.0, 50_000.0);
        let mut lock = PhaseLock::new(60, true);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &a in &t {
            lock.arrival(a);
            lock.update();
            lo = lo.min(lock.delta);
            hi = hi.max(lock.delta);
        }
        // 120 s at 0.06 Hz beat: 7 wraps, give or take the partial one.
        assert!((6..=8).contains(&lock.wraps), "{} wraps", lock.wraps);
        assert!(lo >= -0.25 * lock.period && hi < 1.25 * lock.period);
    }

    #[test]
    fn startup_rebase_keeps_the_learned_absolute_phase_and_history() {
        let mut lock = PhaseLock::new(60, true);
        for n in 0..8 {
            lock.arrival(123456789000 + n * 166667);
            lock.update();
        }
        assert!(lock.ready());
        let old = lock.origin.unwrap() as f64 + lock.delta;
        lock.rebase_before_start(123458200123);
        let new = lock.origin.unwrap() as f64 + lock.delta;
        let error = (new - old).rem_euclid(lock.period);
        assert!(error.min(lock.period - error) < 0.001);
        assert!(lock.delta >= 0. && lock.delta < lock.period);
        assert_eq!(lock.ring.len(), 8);
    }
    #[test]
    fn memory_is_bounded() {
        let mut lock = PhaseLock::new(60, true);
        for k in 0..100_000 {
            lock.arrival(k * 166_667);
            lock.update();
        }
        assert!(lock.ring.len() <= KEEP);
    }
}
