//! Encoder input leases and deadline checks. The callback owns each submitted
//! surface until Media Foundation releases its tracked sample.
use anyhow::{bail, Result};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;
use std::time::Instant;
use windows::{
    core::{implement, Ref},
    Win32::{
        Foundation::E_NOTIMPL, Graphics::Direct3D11::ID3D11Texture2D, Media::MediaFoundation::*,
    },
};

pub struct Lease {
    busy: Arc<AtomicUsize>,
    released: AtomicBool,
    _surface: Option<ID3D11Texture2D>,
    pub release_delay: std::time::Duration,
    pub trace_log: Option<super::trace::Log>,
    pub pts: i64,
}
impl Lease {
    pub fn new(busy: Arc<AtomicUsize>, surface: Option<ID3D11Texture2D>) -> Self {
        busy.fetch_add(1, Ordering::AcqRel);
        Self {
            busy,
            released: AtomicBool::new(false),
            _surface: surface,
            release_delay: std::time::Duration::ZERO,
            trace_log: None,
            pts: 0,
        }
    }
    fn release(&self) {
        if !self.released.swap(true, Ordering::AcqRel) {
            let before = self.busy.fetch_sub(1, Ordering::AcqRel);
            if let Some(log) = &self.trace_log {
                let _ = log.emit(
                    "surface_released",
                    serde_json::json!({"pts":self.pts,"leases_after":before-1}),
                );
            }
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.release();
    }
}
#[implement(IMFAsyncCallback)]
pub struct Released(pub Lease);
impl IMFAsyncCallback_Impl for Released_Impl {
    fn GetParameters(&self, _: *mut u32, _: *mut u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn Invoke(&self, _: Ref<IMFAsyncResult>) -> windows::core::Result<()> {
        if !self.0.release_delay.is_zero() {
            std::thread::sleep(self.0.release_delay);
        }
        self.0.release();
        Ok(())
    }
}

/// A live experimental capture cannot buffer an arbitrary encoder starvation.
/// Half a second already spans 30 output slots; stop explicitly instead of
/// spending the offline five-second wait budget presenting frozen footage.
/// Offline/generated encodes retain their existing backpressure allowance.
pub fn input_wait_budget(live: bool) -> Duration {
    if live {
        Duration::from_millis(500)
    } else {
        Duration::from_secs(5)
    }
}

pub fn check_wait(deadline: Instant, cancelled: bool) -> Result<()> {
    if cancelled {
        bail!("Encoder operation cancelled")
    }
    if Instant::now() >= deadline {
        bail!("Encoder operation timed out")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_live_input_waits_use_the_short_failure_budget() {
        assert_eq!(input_wait_budget(true), Duration::from_millis(500));
        assert_eq!(input_wait_budget(false), Duration::from_secs(5));
    }
    #[test]
    fn multiple_leases_keep_surface_busy_until_last_release() {
        let busy = Arc::new(AtomicUsize::new(0));
        let first = Lease::new(busy.clone(), None);
        let second = Lease::new(busy.clone(), None);
        first.release();
        first.release();
        assert_eq!(busy.load(Ordering::Acquire), 1);
        drop(second);
        assert_eq!(busy.load(Ordering::Acquire), 0);
    }
    #[test]
    fn failed_submission_and_repeated_cycles_release_leases() {
        let busy = Arc::new(AtomicUsize::new(0));
        for _ in 0..100 {
            let lease = Lease::new(busy.clone(), None);
            assert_eq!(busy.load(Ordering::Acquire), 1);
            drop(lease);
        }
        assert_eq!(busy.load(Ordering::Acquire), 0);
    }
    #[test]
    fn stalls_and_cancellation_are_errors() {
        assert!(check_wait(Instant::now() - Duration::from_millis(1), false).is_err());
        assert!(check_wait(Instant::now() + Duration::from_secs(1), true).is_err());
        assert!(check_wait(Instant::now() + Duration::from_secs(1), false).is_ok());
    }
    #[test]
    fn real_tracked_samples_release_after_delayed_consumer() {
        unsafe {
            MFStartup(MF_VERSION, 0).unwrap();
            let busy = Arc::new(AtomicUsize::new(0));
            let tracked = MFCreateTrackedSample().unwrap();
            let sample: IMFSample = windows::core::Interface::cast(&tracked).unwrap();
            let callback: IMFAsyncCallback = Released(Lease::new(busy.clone(), None)).into();
            tracked.SetAllocator(&callback, None).unwrap();
            let consumer = sample.clone();
            drop(sample);
            drop(tracked);
            drop(callback);
            std::thread::sleep(Duration::from_millis(20));
            assert_eq!(busy.load(Ordering::Acquire), 1);
            drop(consumer);
            let deadline = Instant::now() + Duration::from_secs(2);
            while busy.load(Ordering::Acquire) != 0 {
                check_wait(deadline, false).unwrap();
                std::thread::sleep(Duration::from_millis(1));
            }
            MFShutdown().unwrap();
        }
    }
}
