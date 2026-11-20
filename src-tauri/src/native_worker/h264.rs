//! Bounded, allocation-free validation of the encoded Annex-B slice headers.
//! Some hardware MFTs do not implement the B-frame control. Never infer no B
//! frames from that: enforce the requirement on every access unit before writing.
use anyhow::{bail, Context, Result};

fn start(data: &[u8], from: usize) -> Option<usize> {
    (from..data.len().saturating_sub(2)).find(|&i| data[i..].starts_with(&[0, 0, 1]))
}
struct Bits<'a> {
    data: &'a [u8],
    next: usize,
    byte: u8,
    left: u8,
    zeros: u8,
}
impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        if self.left == 0 {
            let mut b = *self.data.get(self.next)?;
            self.next += 1;
            if self.zeros >= 2 && b == 3 {
                b = *self.data.get(self.next)?;
                self.next += 1;
                if b > 3 {
                    return None;
                }
                self.zeros = 0;
            }
            self.zeros = if b == 0 {
                self.zeros.saturating_add(1)
            } else {
                0
            };
            self.byte = b;
            self.left = 8;
        }
        self.left -= 1;
        Some(((self.byte >> self.left) & 1) as u32)
    }
    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 30 {
                return None;
            }
        }
        let mut value = 1u32;
        for _ in 0..zeros {
            value = (value << 1) | self.bit()?;
        }
        Some(value - 1)
    }
}
pub fn validate_no_b(data: &[u8]) -> Result<()> {
    let mut pos = start(data, 0).context("Encoder output is not supported Annex-B H.264")?;
    let mut slices = 0;
    loop {
        let payload = pos + 3;
        let next = start(data, payload);
        let nal = &data[payload..next.unwrap_or(data.len())];
        let header = *nal.first().context("Empty H.264 NAL unit")?;
        if header & 0x80 != 0 {
            bail!("Invalid H.264 NAL header")
        }
        match header & 31 {
            1 | 5 => {
                let mut bits = Bits {
                    data: &nal[1..],
                    next: 0,
                    byte: 0,
                    left: 0,
                    zeros: 0,
                };
                bits.ue()
                    .context("Truncated H.264 first-macroblock header")?;
                let kind = bits.ue().context("Truncated H.264 slice-type header")?;
                if kind > 9 {
                    bail!("Invalid H.264 slice type")
                }
                if kind % 5 == 1 {
                    bail!(
                        "Encoder emitted a B-frame; native recording requires no frame reordering"
                    )
                }
                slices += 1;
            }
            2..=4 | 19..=21 => bail!("Unsupported partitioned or extended H.264 slices"),
            _ => {}
        }
        match next {
            Some(n) => pos = n,
            None => break,
        }
    }
    if slices == 0 {
        bail!("Encoded access unit contains no verifiable H.264 slices")
    }
    Ok(())
}
/// Check against the bounded FIFO of actual submissions, never guessed FPS.
pub fn validate_submission(time: i64, expected: i64, key: bool, required_key: bool) -> Result<()> {
    if time != expected {
        bail!("Hardware encoder changed a submitted timestamp or reordered pictures")
    }
    if required_key && !key {
        bail!("Hardware encoder missed a required source-clock keyframe")
    }
    Ok(())
}
/// Enforce the configured output clock and GOP before bytes enter the segmenter.
pub fn validate_timing(
    index: usize,
    time: i64,
    step: i64,
    keyframe: bool,
    gop: usize,
) -> Result<()> {
    if time != index as i64 * step {
        bail!("Hardware encoder changed the output clock or reordered frames")
    }
    if gop == 0 || (index.is_multiple_of(gop) && !keyframe) {
        bail!("Hardware encoder missed a required segment keyframe")
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_reordered_timestamps_and_missing_gop_boundaries() {
        assert!(validate_timing(120, 120 * 166666, 166666, true, 120).is_ok());
        assert!(validate_timing(120, 120 * 166666, 166666, false, 120).is_err());
        assert!(validate_timing(1, 0, 166666, false, 120).is_err());
        assert!(validate_timing(0, 0, 166666, true, 0).is_err());
    }
    #[test]
    fn accepts_ip_and_repeated_slice_types() {
        for (nal, bits) in [(0x65, 0xb0), (0x41, 0xc0), (0x41, 0x98)] {
            assert!(validate_no_b(&[0, 0, 0, 1, nal, bits]).is_ok());
        }
    }
    #[test]
    fn rejects_b_and_all_b_variants() {
        for bits in [0xa0, 0x9c] {
            assert!(validate_no_b(&[0, 0, 1, 0x41, bits]).is_err());
        }
    }
    #[test]
    fn checks_all_slices_after_sps() {
        assert!(validate_no_b(&[0, 0, 1, 0x67, 0, 0, 1, 0x41, 0xc0, 0, 0, 1, 0x41, 0xa0]).is_err());
    }
    #[test]
    fn rejects_missing_truncated_and_extended_slices() {
        for d in [
            &[][..],
            &[0, 0, 1, 0x65],
            &[0, 0, 1, 0x67, 1],
            &[0, 0, 1, 0x54, 0xc0],
            &[0, 0, 1, 0x41, 0],
        ] {
            assert!(validate_no_b(d).is_err());
        }
    }
    #[test]
    fn skips_emulation_prevention_and_bounds_exp_golomb() {
        let mut b = Bits {
            data: &[0, 0, 3, 1],
            next: 0,
            byte: 0,
            left: 0,
            zeros: 0,
        };
        for _ in 0..23 {
            assert_eq!(b.bit(), Some(0));
        }
        assert_eq!(b.bit(), Some(1));
        assert_eq!(
            Bits {
                data: &[0; 8],
                next: 0,
                byte: 0,
                left: 0,
                zeros: 0
            }
            .ue(),
            None
        );
    }
}
