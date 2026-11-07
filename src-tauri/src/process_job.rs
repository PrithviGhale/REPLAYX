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
