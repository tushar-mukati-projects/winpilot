use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use deckhand_core::{Context, Result};
use windows::{
    Win32::{
        Foundation::HANDLE,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation, SetInformationJobObject,
            TerminateJobObject,
        },
    },
    core::PCWSTR,
};

const TERMINATED_EXIT_CODE: u32 = 1;
const WORKER_LIMITS: JOB_OBJECT_LIMIT = JOB_OBJECT_LIMIT(
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE.0
        | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK.0
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION.0,
);

#[derive(Debug)]
pub struct Job {
    handle: OwnedHandle,
}

impl Job {
    pub fn kill_on_close() -> Result<Self> {
        let job = Self::create()?;
        job.set_limits()?;
        Ok(job)
    }

    pub fn assign(&self, process: &impl AsRawHandle) -> Result<()> {
        let process = HANDLE(process.as_raw_handle());
        unsafe { AssignProcessToJobObject(self.raw(), process) }.context("adding the worker to its job object")
    }

    pub fn terminate(&self) -> Result<()> {
        unsafe { TerminateJobObject(self.raw(), TERMINATED_EXIT_CODE) }.context("terminating the worker job")
    }

    fn create() -> Result<Self> {
        let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.context("creating the worker job object")?;
        let handle = unsafe { OwnedHandle::from_raw_handle(raw.0) };
        Ok(Self { handle })
    }

    fn set_limits(&self) -> Result<()> {
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = WORKER_LIMITS;
        let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        let limits_ptr = (&raw const limits).cast();
        unsafe { SetInformationJobObject(self.raw(), JobObjectExtendedLimitInformation, limits_ptr, size) }
            .context("setting the worker job limits")
    }

    fn raw(&self) -> HANDLE {
        HANDLE(self.handle.as_raw_handle())
    }
}

#[cfg(test)]
impl Job {
    pub fn limits(&self) -> JOB_OBJECT_LIMIT {
        use windows::Win32::System::JobObjects::QueryInformationJobObject;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        let size = size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        let limits_ptr = (&raw mut limits).cast();
        unsafe {
            QueryInformationJobObject(Some(self.raw()), JobObjectExtendedLimitInformation, limits_ptr, size, None)
        }
        .unwrap();
        limits.BasicLimitInformation.LimitFlags
    }

    pub fn contains(&self, process: &impl AsRawHandle) -> bool {
        use windows::{Win32::System::JobObjects::IsProcessInJob, core::BOOL};
        let mut inside = BOOL::default();
        unsafe { IsProcessInJob(HANDLE(process.as_raw_handle()), Some(self.raw()), &raw mut inside) }.unwrap();
        inside.as_bool()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        os::windows::process::CommandExt,
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };

    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    use super::*;

    fn idle_child() -> Child {
        Command::new("cmd.exe")
            .args(["/d", "/q", "/k"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .unwrap()
    }

    fn wait_for_exit(child: &mut Child) -> Option<i32> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            if let Some(status) = child.try_wait().unwrap() {
                return status.code();
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn terminate_kills_the_assigned_process() {
        let mut child = idle_child();
        let job = Job::kill_on_close().unwrap();
        job.assign(&child).unwrap();
        assert!(job.contains(&child));
        assert!(child.try_wait().unwrap().is_none());
        job.terminate().unwrap();
        assert_eq!(wait_for_exit(&mut child), Some(1));
    }

    #[test]
    fn closing_the_job_kills_the_assigned_process() {
        let mut child = idle_child();
        let job = Job::kill_on_close().unwrap();
        job.assign(&child).unwrap();
        drop(job);
        assert!(wait_for_exit(&mut child).is_some());
    }

    #[test]
    fn crashes_die_at_once_instead_of_waiting_for_error_reporting() {
        let job = Job::kill_on_close().unwrap();
        let limits = job.limits();
        assert!(limits.contains(JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION));
        assert!(limits.contains(JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE));
        assert!(limits.contains(JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK));
    }
}
