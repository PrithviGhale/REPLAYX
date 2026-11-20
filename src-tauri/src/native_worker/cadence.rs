//! Bounded spatial cadence hints. Dirty rectangles describe regions, not whole
//! picture identity. A fast unrelated region must not move a 30/60 Hz clock.
//! All captures/textures still reach the selector; this filters metadata only.
const TRACKS: usize = 24;
const KEEP: usize = 32;
const MIN: usize = 9;
const STALE: i64 = 5_000_000;
pub type Rect = [i32; 4];
fn area(r: Rect) -> i64 {
    i64::from(r[2].max(0)) * i64::from(r[3].max(0))
}
fn intersection(a: Rect, b: Rect) -> i64 {
    let width = (i64::from(a[0]) + i64::from(a[2])).min(i64::from(b[0]) + i64::from(b[2]))
        - i64::from(a[0]).max(i64::from(b[0]));
    let height = (i64::from(a[1]) + i64::from(a[3])).min(i64::from(b[1]) + i64::from(b[3]))
        - i64::from(a[1]).max(i64::from(b[1]));
    width.max(0) * height.max(0)
}
fn same(a: Rect, b: Rect) -> bool {
    a.iter()
        .zip(b)
        .all(|(&x, y)| (i64::from(x) - i64::from(y)).abs() <= 8)
}
/// Union coverage sampled on a fixed 8x8 grid, so overlapping dirty rectangles
/// never count twice. Full/split reports share the same spatial identity.
fn covered(region: Rect, rects: &[Rect]) -> bool {
    let mut mask = 0u64;
    for r in rects {
        if r[2] <= 0 || r[3] <= 0 {
            continue;
        }
        let mut bits = 0u64;
        for x in 0..8 {
            let cx = i64::from(region[0]) + (2 * x + 1) * i64::from(region[2]) / 16;
            if i64::from(r[0]) <= cx && cx < i64::from(r[0]) + i64::from(r[2]) {
                bits |= 1 << x;
            }
        }
        if bits == 0 {
            continue;
        }
        for y in 0..8 {
            let cy = i64::from(region[1]) + (2 * y + 1) * i64::from(region[3]) / 16;
            if i64::from(r[1]) <= cy && cy < i64::from(r[1]) + i64::from(r[3]) {
                mask |= bits << (y * 8);
            }
        }
    }
    mask.count_ones() >= 60
}
#[derive(Clone, Copy)]
struct Track {
    rect: Rect,
    times: [i64; KEEP],
    len: usize,
    updated: bool,
    nominal: Option<u32>,
    qualified: bool,
    fast: bool,
    cooldown: u32,
    trusted: bool,
}
impl Track {
    fn new(rect: Rect) -> Self {
        Self {
            rect,
            times: [0; KEEP],
            len: 0,
            updated: false,
            nominal: None,
            qualified: false,
            fast: false,
            cooldown: 0,
            trusted: false,
        }
    }
    fn last(&self) -> i64 {
        self.times[self.len.saturating_sub(1)]
    }
    fn push(&mut self, time: i64, period: f64) {
        self.trusted = false;
        if self.len > 0 && time == self.last() {
            return;
        }
        let previous = if self.len > 0 {
            Some(self.last())
        } else {
            None
        };
        if self.len == KEEP {
            self.times.copy_within(1..KEEP, 0);
            self.len -= 1;
        }
        self.times[self.len] = time;
        self.len += 1;
        let mut intervals = [0i64; KEEP - 1];
        let mut n = 0;
        for w in self.times[..self.len].windows(2) {
            if w[1] > w[0] {
                intervals[n] = w[1] - w[0];
                n += 1;
            }
        }
        if n < MIN - 1 {
            return;
        }
        intervals[..n].sort_unstable();
        let ratio = intervals[n / 2] as f64 / period;
        self.nominal = if (0.7..=1.3).contains(&ratio) {
            Some(1)
        } else if (1.7..=2.3).contains(&ratio) {
            Some(2)
        } else {
            None
        };
        self.fast = ratio < 0.7
            && intervals[..n]
                .iter()
                .filter(|&&t| (t as f64) < 0.7 * period)
                .count()
                * 4
                >= n * 3;
        self.qualified = self.nominal.is_some_and(|slots| {
            intervals[..n]
                .iter()
                .filter(|&&t| (t as f64 - slots as f64 * period).abs() <= 0.5 * period + 2.0)
                .count()
                * 4
                >= n * 3
        });
        if let (Some(p), Some(slots)) = (previous, self.nominal) {
            if (((time - p) as f64) / period - slots as f64).abs() > 0.3 {
                self.cooldown = 3;
            } else if self.cooldown > 0 {
                self.cooldown -= 1;
            } else {
                self.trusted = true;
            }
        }
    }
}
pub struct Observation {
    pub changed: bool,
    pub phase_changed: bool,
    pub region: Option<Rect>,
    pub seed: [i64; KEEP],
    pub seed_len: usize,
    pub switched: bool,
    pub pending_larger: bool,
}
pub struct Cadence {
    period: f64,
    tracks: [Option<Track>; TRACKS],
    selected: Option<usize>,
    pub switches: u64,
    pub irregular_observations: u64,
}
impl Cadence {
    pub fn new(fps: u32) -> Self {
        Self {
            period: 10_000_000. / fps as f64,
            tracks: [None; TRACKS],
            selected: None,
            switches: 0,
            irregular_observations: 0,
        }
    }
    fn pure(&self, index: usize, time: i64) -> bool {
        let r = self.tracks[index].unwrap().rect;
        !self.tracks.iter().enumerate().any(|(i, t)| {
            i != index
                && t.is_some_and(|t| {
                    t.fast && time - t.last() <= STALE && intersection(r, t.rect) * 3 >= area(r)
                })
        })
    }
    pub fn observe(&mut self, time: i64, rects: &[Rect], frame_area: i64) -> Observation {
        for &r in rects {
            if area(r) == 0 || area(r) * 20 < frame_area {
                continue;
            }
            if self.tracks.iter().flatten().any(|t| same(t.rect, r)) {
                continue;
            }
            let free = self.tracks.iter().position(Option::is_none);
            let oldest = (0..TRACKS)
                .filter(|i| Some(*i) != self.selected)
                .min_by_key(|&i| self.tracks[i].map_or(i64::MIN, |t| t.last()));
            if let Some(i) = free.or(oldest) {
                self.tracks[i] = Some(Track::new(r));
            }
        }
        for track in self.tracks.iter_mut().flatten() {
            track.updated = covered(track.rect, rects);
            track.trusted = false;
            if track.updated {
                track.push(time, self.period);
            }
        }
        let eligible = |i: usize| {
            self.tracks[i].is_some_and(|t| t.qualified && time - t.last() <= STALE)
                && self.pure(i, time)
        };
        let best = (0..TRACKS)
            .filter(|&i| eligible(i))
            .max_by_key(|&i| area(self.tracks[i].unwrap().rect));
        let previous = self.selected;
        if self.selected.is_none_or(|i| !eligible(i)) {
            self.selected = best;
        } else if let (Some(current), Some(best)) = (self.selected, best) {
            if area(self.tracks[best].unwrap().rect) * 4
                >= area(self.tracks[current].unwrap().rect) * 5
            {
                self.selected = Some(best);
            }
        }
        let switched = previous != self.selected;
        let mut obs = Observation {
            changed: false,
            phase_changed: false,
            region: None,
            seed: [0; KEEP],
            seed_len: 0,
            switched,
            pending_larger: false,
        };
        if switched {
            self.switches += 1;
        }
        if let Some(index) = self.selected {
            let t = self.tracks[index].unwrap();
            obs.region = Some(t.rect);
            obs.changed = t.updated;
            obs.phase_changed = t.updated && t.trusted;
            if t.updated && !t.trusted {
                self.irregular_observations += 1;
            }
            if switched {
                for &timestamp in &t.times[..t.len] {
                    if timestamp < time {
                        obs.seed[obs.seed_len] = timestamp;
                        obs.seed_len += 1;
                    }
                }
            }
            obs.pending_larger = self.tracks.iter().enumerate().any(|(i, c)| {
                c.is_some_and(|c| {
                    i != index
                        && c.len >= 3
                        && c.len < MIN
                        && time - c.last() <= STALE
                        && area(c.rect) * 4 >= area(t.rect) * 5
                        && {
                            let mut dt = [0i64; KEEP - 1];
                            let mut n = 0;
                            for w in c.times[..c.len].windows(2) {
                                if w[1] > w[0] {
                                    dt[n] = w[1] - w[0];
                                    n += 1;
                                }
                            }
                            if n < 2 {
                                false
                            } else {
                                dt[..n].sort_unstable();
                                let rate = dt[n / 2] as f64 / self.period;
                                ((0.7..=1.3).contains(&rate) || (1.7..=2.3).contains(&rate))
                                    && self.pure(i, time)
                            }
                        }
                })
            });
        }
        obs
    }
    pub fn selected_region(&self) -> Option<Rect> {
        self.selected.map(|i| self.tracks[i].unwrap().rect)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const MAIN: Rect = [40, 60, 960, 540];
    const FAST: Rect = [1360, 700, 960, 540];
    #[test]
    fn fast_other_window_and_split_rectangles_do_not_become_picture_updates() {
        for hz in [120, 144, 240, 250] {
            for fps in [30., 60_000. / 1001., 60.] {
                for phase in [0., 42_000., 83_333., 166_666.] {
                    let mut c = Cadence::new(60);
                    let mut last = -1i64;
                    for k in 0..hz * 4 {
                        let time = k as i64 * 10_000_000 / hz as i64;
                        let id =
                            (((time as f64 - phase) * fps / 10_000_000.).floor() as i64).max(-1);
                        let changed = id != last && id >= 0;
                        last = id;
                        let mut rects = vec![FAST];
                        if changed {
                            if k % 3 == 0 {
                                rects.extend([[40, 60, 960, 380], [40, 440, 960, 160]]);
                            } else {
                                rects.push(MAIN);
                            }
                        }
                        let obs = c.observe(time, &rects, 2560 * 1600);
                        if time > 10_000_000 {
                            assert_eq!(obs.region, Some(MAIN), "{hz} {fps} {phase}");
                            assert_eq!(obs.changed, changed, "{hz} {fps} {phase} at {time}");
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn overlapping_dirty_rectangles_never_double_count_coverage() {
        assert!(!covered(MAIN, &[[40, 60, 960, 200], [40, 60, 960, 200]]));
        assert!(covered(MAIN, &[[40, 60, 960, 380], [40, 440, 960, 160]]));
    }
    #[test]
    fn capture_burst_is_retained_but_does_not_retrain_clock_phase() {
        let mut c = Cadence::new(60);
        for k in 0..80 {
            c.observe(k * 10_000_000 / 60, &[MAIN], 2560 * 1600);
        }
        let t = 79 * 10_000_000 / 60;
        for dt in [416_667, 458_334, 500_001] {
            let o = c.observe(t + dt, &[MAIN], 2560 * 1600);
            assert!(o.changed);
            assert!(!o.phase_changed);
        }
        assert!(c.irregular_observations >= 3);
    }
    #[test]
    fn state_is_bounded_under_region_churn() {
        let mut c = Cadence::new(60);
        for k in 0..1000 {
            c.observe(
                k * 10000,
                &[[(k * 10) as i32, (k * 10) as i32, 900, 540]],
                4096000,
            );
        }
        assert!(c.tracks.iter().flatten().count() <= TRACKS);
    }
}
