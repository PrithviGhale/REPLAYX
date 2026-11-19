//! Minimal H.264 MPEG-TS transport: explicit encoder PTS before muxer scheduling.
//! No video decoding, encoding, CPU texture readback or recording accumulation.
use std::io::{self, Write};
#[derive(Default)]
pub struct Transport {
    video_cc: u8,
    pat_cc: u8,
    pmt_cc: u8,
    started: bool,
}
fn crc(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for b in bytes {
        crc ^= (*b as u32) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}
fn table(out: &mut impl Write, pid: u16, cc: &mut u8, section: &[u8]) -> io::Result<()> {
    let mut p = [0xffu8; 188];
    p[..5].copy_from_slice(&[0x47, 0x40 | ((pid >> 8) as u8), pid as u8, 0x10 | *cc, 0]);
    *cc = (*cc + 1) & 15;
    p[5..5 + section.len()].copy_from_slice(section);
    let sum = crc(section);
    p[5 + section.len()..9 + section.len()].copy_from_slice(&sum.to_be_bytes());
    out.write_all(&p)
}
impl Transport {
    pub fn write(
        &mut self,
        out: &mut impl Write,
        time_hns: i64,
        key: bool,
        data: &[u8],
    ) -> io::Result<()> {
        if time_hns < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Negative native timestamp",
            ));
        }
        if !self.started || key {
            table(
                out,
                0,
                &mut self.pat_cc,
                &[0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xf0, 0],
            )?;
            table(
                out,
                0x1000,
                &mut self.pmt_cc,
                &[
                    2, 0xb0, 18, 0, 1, 0xc1, 0, 0, 0xe1, 0, 0xf0, 0, 0x1b, 0xe1, 0, 0xf0, 0,
                ],
            )?;
            self.started = true;
        }
        let pts = ((time_hns as u128 * 90000 + 5_000_000) / 10_000_000) as u64 & ((1u64 << 33) - 1);
        let header = [
            0,
            0,
            1,
            0xe0,
            0,
            0,
            0x80,
            0x80,
            5,
            0x21 | (((pts >> 30) as u8 & 7) << 1),
            (pts >> 22) as u8,
            (((pts >> 15) as u8 & 127) << 1) | 1,
            (pts >> 7) as u8,
            ((pts as u8 & 127) << 1) | 1,
        ];
        let mut offset = 0;
        let total = header.len() + data.len();
        while offset < total {
            let first = offset == 0;
            let capacity = if first { 176 } else { 184 };
            let n = (total - offset).min(capacity);
            let adaptation = 184 - n;
            let mut packet = [0xffu8; 188];
            packet[..4].copy_from_slice(&[
                0x47,
                if first { 0x41 } else { 1 },
                0,
                (if adaptation > 0 { 0x30 } else { 0x10 }) | self.video_cc,
            ]);
            self.video_cc = (self.video_cc + 1) & 15;
            let mut start = 4;
            if adaptation > 0 {
                packet[4] = (adaptation - 1) as u8;
                start += adaptation;
                if adaptation > 1 {
                    packet[5] = if first {
                        0x10 | if key { 0x40 } else { 0 }
                    } else {
                        0
                    };
                }
                if first {
                    packet[6] = (pts >> 25) as u8;
                    packet[7] = (pts >> 17) as u8;
                    packet[8] = (pts >> 9) as u8;
                    packet[9] = (pts >> 1) as u8;
                    packet[10] = ((pts as u8 & 1) << 7) | 0x7e;
                    packet[11] = 0;
                }
            }
            for slot in &mut packet[start..start + n] {
                *slot = if offset < header.len() {
                    header[offset]
                } else {
                    data[offset - header.len()]
                };
                offset += 1;
            }
            out.write_all(&packet)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tables_crc_and_payload_survive_packet_boundaries() {
        for length in [1usize, 160, 170, 184, 188, 1024, 100000] {
            let bytes: Vec<_> = (0..length).map(|n| n as u8).collect();
            let mut result = Vec::new();
            Transport::default()
                .write(&mut result, 0, true, &bytes)
                .unwrap();
            assert_eq!(result.len() % 188, 0);
            assert_eq!(crc(&result[5..21]), 0);
            assert_eq!(crc(&result[193..214]), 0);
            let mut recovered = Vec::new();
            for p in result[376..].as_chunks::<188>().0 {
                assert_eq!(p[0], 0x47);
                let start = 4 + if p[3] & 0x20 != 0 {
                    1 + p[4] as usize
                } else {
                    0
                };
                recovered.extend_from_slice(&p[start..]);
            }
            assert_eq!(&recovered[14..], &bytes);
        }
    }
    #[test]
    fn presentation_clock_is_encoded_before_container_scheduling() {
        let mut output = Vec::new();
        Transport::default()
            .write(&mut output, 10_000_000, true, &[1; 100])
            .unwrap();
        let p = &output[376..564];
        let start = 5 + p[4] as usize;
        let h = &p[start + 9..];
        let pts = ((h[0] as u64 >> 1 & 7) << 30)
            | ((h[1] as u64) << 22)
            | ((h[2] as u64 >> 1) << 15)
            | ((h[3] as u64) << 7)
            | (h[4] as u64 >> 1);
        assert_eq!(pts, 90000);
    }
}
