//! Frame selection for the native prototype: a port of what today's
//! recorder does to WGC frames, so the native path records the same frames.
//!
//! Today's chain for WGC at 1x:
//! - gfxcapture stamps each frame with Windows' capture time (100 ns units)
//!   and no duration; FFmpeg rebases the input to start at 0 (no -copyts)
//!   and, at the filter graph's output, gives every frame without a
//!   duration one frame of the graph's frame rate: gfxcapture's
//!   max_framerate (80 at the 12.5 ms floor, 120 at 2x), i.e. 0.75 or 0.5
//!   of an output slot;
//! - media::frame_timing re-times it to the nearest output slot,
//!   `setpts=round(PTS*TB*fps)/(TB*fps)` (truncated back to 100 ns);
//! - `-r fps -fps_mode cfr`: `video_sync_process` and `fg_output_frame` in
//!   fftools/ffmpeg_filter.c (FFmpeg n9.0.2) place each frame at the next
//!   slot (a frame that overlaps it is clipped into it), drop frames that
//!   end more than 1.1 slots early, repeat across gaps, and count drops and
//!   duplicates.
//!
//! Pure: no Windows calls. Checked frame by frame against FFmpeg itself by
//! scripts/bench/native-select-check.py.

/// Where an output slot's picture comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The frame just pushed.
    Current,
    /// The frame pushed before it (a repeat).
    Previous,
}

/// av_rescale_q with AV_ROUND_NEAR_INF: a * bq / cq.
fn rescale(a: i64, bq: (i64, i64), cq: (i64, i64)) -> i64 {
    let num = a as i128 * bq.0 as i128 * cq.1 as i128;
    let den = bq.1 as i128 * cq.0 as i128;
    let r = (num.abs() * 2 + den) / (den * 2);
    (if num < 0 { -r } else { r }) as i64
}

#[derive(Debug, Default)]
pub struct CfrSelector {
    fps: i64,
    /// Each frame's duration in 100 ns (FFmpeg: 1 / the graph's frame rate).
    frame_duration: i64,
    /// First frame's capture time (FFmpeg's input start time).
    start: Option<i64>,
    next_pts: i64,
    prev_hist: [i64; 3],
    last_dropped: bool,
    have_prev: bool,
    /// Whether the previous frame filled any slot.
    prev_emitted: bool,
    /// Repeats of a previous frame that itself filled no slot (FFmpeg keeps
    /// every capture and can show such a frame; a native path that never
    /// converts dropped captures repeats the last recorded frame instead).
    pub repeats_of_dropped: u64,
    /// FFmpeg's `dup_frames` and `drop_frames` counters.
    pub dups: u64,
    pub drops: u64,
}

fn median3(a: i64, b: i64, c: i64) -> i64 {
    let (max2, min2) = if a >= b { (a, b) } else { (b, a) };
    let m = if c >= max2 { max2 } else { c };
    if m >= min2 {
        m
    } else {
        min2
    }
}

/// llrintf: round half to even.
fn llrintf(v: f64) -> i64 {
    (v as f32).round_ties_even() as i64
}

const TB: (i64, i64) = (1, 10_000_000); // gfxcapture / WGC time base

impl CfrSelector {
    /// `capture_rate`: the capture source's advertised frame rate
    /// (gfxcapture max_framerate), which FFmpeg turns into each frame's
    /// duration.
    pub fn new(fps: u32, capture_rate: u32) -> Self {
        Self {
            fps: fps as i64,
            frame_duration: 10_000_000 / capture_rate.max(1) as i64,
            ..Default::default()
        }
    }

    /// One captured frame at `time` (100 ns units, e.g. WGC's
    /// SystemRelativeTime). Returns the output slots it fills, in order,
    /// each with its picture source.
    pub fn push(&mut self, time: i64) -> Vec<(i64, Source)> {
        let start = *self.start.get_or_insert(time);
        let pts = time - start;
        // setpts=round(PTS*TB*fps)/(TB*fps): evaluated in doubles, then
        // truncated to an integer timestamp (D2TS).
        let tb = TB.0 as f64 / TB.1 as f64;
        let fps = self.fps as f64;
        let regridded = ((pts as f64 * tb * fps).round() / (tb * fps)) as i64;
        // adjust_frame_pts_to_encoder_tb: to 1/fps with 16 extra bits.
        let extra = (29 - (63 - (self.fps as u64).leading_zeros() as i64)).clamp(0, 16);
        let fine = (1, self.fps << extra);
        let mut sync_ipts = rescale(regridded, TB, fine) as f64 / (1i64 << extra) as f64;
        if sync_ipts != sync_ipts.round_ties_even() {
            sync_ipts += sync_ipts.signum() / (1u64 << 17) as f64;
        }
        let duration = self.frame_duration as f64 * (TB.0 as f64 / TB.1 as f64) * fps;
        let mut delta0 = sync_ipts - self.next_pts as f64;
        let delta = delta0 + duration;
        if delta0 < 0.0 && delta > 0.0 {
            // A frame that overlaps the next slot is clipped into it.
            delta0 = 0.0;
        }
        let (mut nb, mut nb_prev) = (1i64, 0i64);
        if delta < -1.1 {
            nb = 0;
        } else if delta > 1.1 {
            nb = llrintf(delta);
            if delta0 > 1.1 {
                nb_prev = llrintf(delta0 - 0.6);
            }
        }
        self.account(nb, nb_prev, true);
        self.emit(nb, nb_prev, false)
    }

    /// End of stream: FFmpeg repeats the last frame by the median of the
    /// last three repeat counts. Returns those slots.
    pub fn finish(&mut self) -> Vec<(i64, Source)> {
        if !self.have_prev {
            return Vec::new();
        }
        let n = median3(self.prev_hist[0], self.prev_hist[1], self.prev_hist[2]);
        if n == 0 && self.last_dropped {
            self.drops += 1;
        }
        self.account(n, n, false);
        self.emit(n, n, true)
    }

    /// FFmpeg's bookkeeping after the decision (the `finish:` block).
    fn account(&mut self, nb: i64, nb_prev: i64, frame: bool) {
        self.prev_hist.rotate_right(1);
        self.prev_hist[0] = nb_prev;
        if nb_prev == 0 && self.last_dropped {
            self.drops += 1;
        }
        let reused = i64::from(nb_prev != 0 && self.last_dropped);
        let fresh = i64::from(nb > nb_prev);
        if nb > reused + fresh {
            self.dups += (nb - reused - fresh) as u64;
        }
        self.last_dropped = nb == nb_prev && frame;
    }

    fn emit(&mut self, nb: i64, nb_prev: i64, eof: bool) -> Vec<(i64, Source)> {
        let mut out = Vec::with_capacity(nb.max(0) as usize);
        let mut current = false;
        for i in 0..nb {
            let source = if eof || (i < nb_prev && self.have_prev) {
                if !self.prev_emitted {
                    self.repeats_of_dropped += 1;
                }
                Source::Previous
            } else {
                current = true;
                Source::Current
            };
            out.push((self.next_pts, source));
            self.next_pts += 1;
        }
        if !eof {
            self.have_prev = true;
            self.prev_emitted = current;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regular_input_at_the_output_rate_maps_one_to_one() {
        let mut s = CfrSelector::new(60, 60);
        let mut slots = Vec::new();
        for n in 0..120 {
            slots.extend(s.push(n * 10_000_000 / 60));
        }
        assert_eq!(slots.len(), 120);
        assert!(slots
            .iter()
            .enumerate()
            .all(|(i, (slot, src))| *slot == i as i64 && *src == Source::Current));
        assert_eq!((s.dups, s.drops), (0, 0));
    }

    #[test]
    fn slots_stay_contiguous_through_gaps_and_surplus() {
        // 80 captures/s with a 0.5 s gap: every output slot is filled once,
        // in order; the gap shows as duplicates, the surplus as drops.
        let mut s = CfrSelector::new(60, 80);
        let mut out = Vec::new();
        for n in (0..160).filter(|n| !(60..100).contains(n)) {
            out.extend(s.push(n * 10_000_000 / 80));
        }
        out.extend(s.finish());
        let slots: Vec<i64> = out.iter().map(|(slot, _)| *slot).collect();
        assert_eq!(slots, (0..slots.len() as i64).collect::<Vec<_>>());
        assert!(s.dups > 0 && s.drops > 0);
    }
}
