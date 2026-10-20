mod inherit_list;

use std::{
    io::{PipeReader, PipeWriter},
    os::windows::{
        ffi::OsStrExt,
        io::{AsHandle, AsRawHandle, OwnedHandle},
    },
    path::Path,
    time::Duration,
};

use windows::{
    Win32::{
        Foundation::{HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation, WAIT_OBJECT_0},
        System::Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
            ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
        },
    },
    core::{PCWSTR, PWSTR},
};
use winpilot_core::{Context, Result};
use winpilot_win32::{adopt, finite_millis, raw};

use inherit_list::InheritList;

use crate::{
    job::{Job, KILLED_EXIT_CODE},
    lifecycle::Spawn,
    link::{WorkerPipes, WorkerProcess},
};

const WORKER_ARGUMENT: &str = "worker";

pub struct ProcessSpawner;

impl Spawn for ProcessSpawner {
    fn spawn(&self) -> Result<WorkerPipes> {
        let exe = std::env::current_exe().context("finding the winpilot executable")?;
        let child = launch_in_job(&exe, WORKER_ARGUMENT)?;
        let process = Box::new(WorkerChild { process: child.process, job: child.job });
        Ok(WorkerPipes { stdin: Box::new(child.stdin), stdout: Box::new(child.stdout), process })
    }
}

struct JobbedChild {
    stdin: PipeWriter,
    stdout: PipeReader,
    process: OwnedHandle,
    job: Job,
}

struct ChildStdio {
    input: OwnedHandle,
    output: OwnedHandle,
    error: Option<OwnedHandle>,
}

struct Suspended {
    process: OwnedHandle,
    thread: OwnedHandle,
}

struct WorkerChild {
    process: OwnedHandle,
    job: Job,
}

fn launch_in_job(exe: &Path, argument: &str) -> Result<JobbedChild> {
    let job = Job::kill_on_close()?;
    let (stdout, child_output) = std::io::pipe().context("creating the worker output pipe")?;
    let (child_input, stdin) = std::io::pipe().context("creating the worker input pipe")?;
    let inherited = ChildStdio {
        input: inheritable(child_input.into())?,
        output: inheritable(child_output.into())?,
        error: inheritable_stderr(),
    };
    let suspended = create_suspended(exe, argument, &inherited)?;
    drop(inherited);
    let process = suspended.release_into(&job)?;
    Ok(JobbedChild { stdin, stdout, process, job })
}

fn create_suspended(exe: &Path, argument: &str, stdio: &ChildStdio) -> Result<Suspended> {
    let application = wide_path(exe);
    let mut command_line = command_line(exe, argument);
    let mut inherit = InheritList::new(stdio.inherited())?;
    let startup = STARTUPINFOEXW { StartupInfo: stdio.startup_info(), lpAttributeList: inherit.raw() };
    let flags = CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT;
    let mut info = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(application.as_ptr()),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            true,
            flags,
            None,
            PCWSTR::null(),
            &raw const startup.StartupInfo,
            &raw mut info,
        )
    }
    .context("starting the desktop worker")?;
    Suspended::adopt(&info)
}

impl ChildStdio {
    fn inherited(&self) -> Vec<HANDLE> {
        let mut handles = vec![raw(&self.input), raw(&self.output)];
        handles.extend(self.error.as_ref().map(raw));
        handles
    }

    fn startup_info(&self) -> STARTUPINFOW {
        STARTUPINFOW {
            cb: size_of::<STARTUPINFOEXW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: raw(&self.input),
            hStdOutput: raw(&self.output),
            hStdError: self.error.as_ref().map_or_else(HANDLE::default, raw),
            ..STARTUPINFOW::default()
        }
    }
}

impl Suspended {
    fn adopt(info: &PROCESS_INFORMATION) -> Result<Self> {
        let process = unsafe { adopt(info.hProcess) }.context("the worker process handle")?;
        let thread = unsafe { adopt(info.hThread) }.context("the worker thread handle")?;
        Ok(Self { process, thread })
    }

    fn release_into(self, job: &Job) -> Result<OwnedHandle> {
        if let Err(error) = job.assign(&self.process).and_then(|()| resume(&self.thread)) {
            kill(&self.process);
            return Err(error);
        }
        Ok(self.process)
    }
}

impl WorkerProcess for WorkerChild {
    fn exits_within(&mut self, patience: Duration) -> bool {
        exits_within(&self.process, patience)
    }

    fn terminate(&mut self) {
        let Err(error) = self.job.terminate() else { return };
        eprintln!("winpilot: {error}");
        kill(&self.process);
    }
}

fn exits_within(process: &OwnedHandle, patience: Duration) -> bool {
    let waited = unsafe { WaitForSingleObject(raw(process), finite_millis(patience)) };
    waited == WAIT_OBJECT_0
}

fn inheritable(handle: OwnedHandle) -> Result<OwnedHandle> {
    unsafe { SetHandleInformation(raw(&handle), HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT) }
        .context("letting the worker inherit a pipe")?;
    Ok(handle)
}

fn inheritable_stderr() -> Option<OwnedHandle> {
    let stderr = std::io::stderr();
    let handle = stderr.as_raw_handle();
    if handle.is_null() || handle == INVALID_HANDLE_VALUE.0 {
        return None;
    }
    let copy = stderr.as_handle().try_clone_to_owned().ok()?;
    inheritable(copy).ok()
}

fn resume(thread: &OwnedHandle) -> Result<()> {
    let previous = unsafe { ResumeThread(raw(thread)) };
    if previous == u32::MAX {
        return Err(std::io::Error::last_os_error()).context("resuming the desktop worker");
    }
    Ok(())
}

fn kill(process: &OwnedHandle) {
    if let Err(error) = unsafe { TerminateProcess(raw(process), KILLED_EXIT_CODE) } {
        eprintln!("winpilot: could not stop the desktop worker: {error}");
    }
}

fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn command_line(exe: &Path, argument: &str) -> Vec<u16> {
    let quote = u16::from(b'"');
    let mut line = vec![quote];
    line.extend(exe.as_os_str().encode_wide());
    line.extend([quote, u16::from(b' ')]);
    line.extend(argument.encode_utf16());
    line.push(0);
    line
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        path::PathBuf,
    };

    use super::*;

    fn cmd() -> PathBuf {
        PathBuf::from(std::env::var_os("ComSpec").unwrap())
    }

    #[test]
    fn command_line_quotes_the_executable() {
        let line = command_line(Path::new("C:\\Program Files\\winpilot\\winpilot.exe"), "worker");
        assert_eq!(line.last(), Some(&0));
        let text = String::from_utf16(&line[..line.len() - 1]).unwrap();
        assert_eq!(text, "\"C:\\Program Files\\winpilot\\winpilot.exe\" worker");
    }

    #[test]
    fn launched_child_runs_inside_its_job_with_working_pipes() {
        let JobbedChild { mut stdin, mut stdout, process, job } = launch_in_job(&cmd(), "/d /q").unwrap();
        assert!(job.contains(&process));
        stdin.write_all(b"echo winpilot-pipe-check\r\nexit\r\n").unwrap();
        drop(stdin);
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).unwrap();
        assert!(String::from_utf8_lossy(&output).contains("winpilot-pipe-check"));
    }

    #[test]
    fn a_child_that_reads_to_the_end_of_its_input_exits_when_stdin_closes() {
        let JobbedChild { stdin, stdout, process, job } = launch_in_job(&cmd(), "/d /q").unwrap();
        let mut child = WorkerChild { process, job };
        assert!(!child.exits_within(Duration::from_millis(50)));
        drop(stdin);
        assert!(child.exits_within(Duration::from_secs(5)));
        drop(stdout);
    }

    #[test]
    fn launch_of_a_missing_program_fails_cleanly() {
        let missing = std::env::temp_dir().join("winpilot-no-such-program.exe");
        let error = launch_in_job(&missing, "worker").err().unwrap();
        assert!(error.message.starts_with("starting the desktop worker"), "{}", error.message);
    }
}
