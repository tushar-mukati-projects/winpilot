use std::{
    io::{PipeReader, PipeWriter},
    os::windows::{
        ffi::OsStrExt,
        io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
};

use deckhand_core::{Context, Error, Result};
use windows::{
    Win32::{
        Foundation::{HANDLE, HANDLE_FLAG_INHERIT, INVALID_HANDLE_VALUE, SetHandleInformation},
        System::Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList, LPPROC_THREAD_ATTRIBUTE_LIST,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            STARTUPINFOW, TerminateProcess, UpdateProcThreadAttribute,
        },
    },
    core::{PCWSTR, PWSTR},
};

use crate::{
    job::Job,
    lifecycle::Spawn,
    link::{WorkerPipes, WorkerProcess},
};

const WORKER_ARGUMENT: &str = "worker";
const KILLED_EXIT_CODE: u32 = 1;

pub struct ProcessSpawner;

impl Spawn for ProcessSpawner {
    fn spawn(&self) -> Result<WorkerPipes> {
        let exe = std::env::current_exe().context("finding the deckhand executable")?;
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

struct InheritList {
    buffer: Vec<usize>,
    handles: Vec<HANDLE>,
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
    Ok(Suspended::adopt(&info))
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
    fn adopt(info: &PROCESS_INFORMATION) -> Self {
        let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess.0) };
        let thread = unsafe { OwnedHandle::from_raw_handle(info.hThread.0) };
        Self { process, thread }
    }

    fn release_into(self, job: &Job) -> Result<OwnedHandle> {
        if let Err(error) = job.assign(&self.process).and_then(|()| resume(&self.thread)) {
            kill(&self.process);
            return Err(error);
        }
        Ok(self.process)
    }
}

impl InheritList {
    fn new(handles: Vec<HANDLE>) -> Result<Self> {
        let mut size = attribute_list_size()?;
        let mut buffer = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list_ptr = LPPROC_THREAD_ATTRIBUTE_LIST(buffer.as_mut_ptr().cast());
        unsafe { InitializeProcThreadAttributeList(Some(list_ptr), 1, None, &raw mut size) }
            .context("preparing the worker handle list")?;
        let mut list = Self { buffer, handles };
        list.limit_to_handles()?;
        Ok(list)
    }

    fn limit_to_handles(&mut self) -> Result<()> {
        let value = self.handles.as_ptr().cast();
        let size = size_of_val(self.handles.as_slice());
        let attribute = PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize;
        unsafe { UpdateProcThreadAttribute(self.raw(), 0, attribute, Some(value), size, None, None) }
            .context("limiting the handles the worker inherits")
    }

    fn raw(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.buffer.as_mut_ptr().cast())
    }
}

impl Drop for InheritList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.raw()) };
    }
}

impl WorkerProcess for WorkerChild {
    fn terminate(&mut self) {
        let Err(error) = self.job.terminate() else { return };
        eprintln!("deckhand: {error}");
        kill(&self.process);
    }
}

fn attribute_list_size() -> Result<usize> {
    let mut size = 0;
    let probe = unsafe { InitializeProcThreadAttributeList(None, 1, None, &raw mut size) };
    if size > 0 {
        return Ok(size);
    }
    probe.context("sizing the worker handle list")?;
    Err(Error::internal("sizing the worker handle list returned no size"))
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
        eprintln!("deckhand: could not stop the desktop worker: {error}");
    }
}

fn raw(handle: &OwnedHandle) -> HANDLE {
    HANDLE(handle.as_raw_handle())
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
        let line = command_line(Path::new("C:\\Program Files\\deckhand\\deckhand.exe"), "worker");
        assert_eq!(line.last(), Some(&0));
        let text = String::from_utf16(&line[..line.len() - 1]).unwrap();
        assert_eq!(text, "\"C:\\Program Files\\deckhand\\deckhand.exe\" worker");
    }

    #[test]
    fn launched_child_runs_inside_its_job_with_working_pipes() {
        let JobbedChild { mut stdin, mut stdout, process, job } = launch_in_job(&cmd(), "/d /q").unwrap();
        assert!(job.contains(&process));
        stdin.write_all(b"echo deckhand-pipe-check\r\nexit\r\n").unwrap();
        drop(stdin);
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).unwrap();
        assert!(String::from_utf8_lossy(&output).contains("deckhand-pipe-check"));
    }

    #[test]
    fn launch_of_a_missing_program_fails_cleanly() {
        let missing = std::env::temp_dir().join("deckhand-no-such-program.exe");
        let error = launch_in_job(&missing, "worker").err().unwrap();
        assert!(error.message.starts_with("starting the desktop worker"), "{}", error.message);
    }
}
