//! Pure health policy shared by the watchdog and fault-injection tests.
use anyhow::{bail, Result};
pub const RESERVE: u64 = 128 * 1024 * 1024;
pub fn storage_ready(data: u64, output: u64, starting: bool) -> Result<()> {
    let minimum = if starting { 2 * RESERVE } else { RESERVE };
    if data < minimum || output < minimum {
        bail!("Storage below {} MiB reserve. Free space before restarting; finalized segments are preserved.",minimum/1024/1024)
    }
    Ok(())
}
pub fn stream_health(
    elapsed_ms: u64,
    frame_ms: u64,
    segment_ms: u64,
    audio_write_ms: &[u64],
) -> Result<()> {
    if elapsed_ms.saturating_sub(frame_ms) > 6000 {
        bail!("No newly encoded frames for six seconds")
    }
    if elapsed_ms.saturating_sub(segment_ms) > 8000 {
        bail!("No finalized media segment for eight seconds; capture is not being saved")
    }
    if elapsed_ms > 5000
        && audio_write_ms
            .iter()
            .any(|last| elapsed_ms.saturating_sub(*last) > 3000)
    {
        bail!("Audio output stalled; selected audio is not reaching the muxer")
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn either_full_volume_stops() {
        assert!(storage_ready(RESERVE - 1, 10 * RESERVE, false).is_err());
        assert!(storage_ready(10 * RESERVE, RESERVE - 1, false).is_err());
        assert!(storage_ready(RESERVE, RESERVE, false).is_ok());
        assert!(storage_ready(RESERVE, RESERVE, true).is_err());
    }
    #[test]
    fn frozen_frames_never_healthy() {
        assert!(stream_health(7000, 0, 6000, &[]).is_err());
    }
    #[test]
    fn frames_without_files_are_unhealthy() {
        assert!(stream_health(9000, 8900, 0, &[]).is_err());
    }
    #[test]
    fn audio_write_stall_is_unhealthy() {
        assert!(stream_health(9000, 8900, 8000, &[8900, 5000]).is_err());
    }
    #[test]
    fn silent_loopback_is_healthy_if_written() {
        assert!(stream_health(9000, 8900, 8000, &[8900]).is_ok());
    }
}
