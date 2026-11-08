//! Closing the capture job (including process crash) terminates its encoder.
use anyhow::Result;
use std::{os::windows::io::AsRawHandle, process::Child};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    },
};

pub struct CaptureJob(HANDLE);
// A job handle is owned here, never dereferenced, and only closed on drop.
unsafe impl Send for CaptureJob {}
impl CaptureJob {
    pub fn attach(child: &Child) -> Result<Self> {
        unsafe {
            let job = Self(CreateJobObjectW(None, None)?);
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                std::mem::size_of_val(&limits) as u32,
            )?;
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))?;
            Ok(job)
        }
    }
}
impl Drop for CaptureJob {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Raise the capture/encoder process's CPU scheduling class (above normal).
pub fn raise_cpu_priority(child: &Child) -> bool {
    use windows::Win32::System::Threading::{SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS};
    unsafe { SetPriorityClass(HANDLE(child.as_raw_handle()), ABOVE_NORMAL_PRIORITY_CLASS).is_ok() }
}
/// Raise the process's GPU scheduling class to HIGH, so its small capture and
/// scaling jobs are not queued behind a game that keeps the GPU at 100% (OBS
/// documents the same remedy). Takes effect once the process has opened its
/// GPU device. HIGH, not REALTIME: realtime could starve the compositor.
/// Allowed without admin rights for a process we own. Returns the NTSTATUS.
pub fn raise_gpu_priority(child: &Child) -> i32 {
    use windows::{
        core::{s, w},
        Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW},
    };
    const D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH: i32 = 4;
    unsafe {
        let Ok(gdi) = LoadLibraryW(w!("gdi32.dll")) else {
            return -1;
        };
        let Some(f) = GetProcAddress(gdi, s!("D3DKMTSetProcessSchedulingPriorityClass")) else {
            return -1;
        };
        let set: unsafe extern "system" fn(HANDLE, i32) -> i32 = std::mem::transmute(f);
        set(
            HANDLE(child.as_raw_handle()),
            D3DKMT_SCHEDULINGPRIORITYCLASS_HIGH,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_job_terminates_child() {
        let mut child = crate::media::command(std::path::Path::new("powershell.exe"))
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ])
            .spawn()
            .unwrap();
        let job = CaptureJob::attach(&child).unwrap();
        drop(job);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() > until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Job did not terminate child");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
