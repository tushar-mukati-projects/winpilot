use std::{
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
};

use deckhand_core::{Context, Error, Result};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::{
    job::Job,
    link::{WorkerPipes, WorkerProcess},
    supervisor::Spawn,
};

pub struct ProcessSpawner;

impl Spawn for ProcessSpawner {
    fn spawn(&self) -> Result<WorkerPipes> {
        let exe = std::env::current_exe().context("finding the deckhand executable")?;
        let mut child = worker_command(&exe).spawn().context("starting the desktop worker")?;
        let job = match contain(&child) {
            Ok(job) => job,
            Err(error) => {
                kill_quietly(&mut child);
                return Err(error);
            }
        };
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            kill_quietly(&mut child);
            return Err(Error::internal("the desktop worker was started without stdio pipes"));
        };
        let process = Box::new(WorkerChild { child, job });
        Ok(WorkerPipes { stdin: Box::new(stdin), stdout: Box::new(stdout), process })
    }
}

fn worker_command(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    command
        .arg("worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .creation_flags(CREATE_NO_WINDOW.0);
    command
}

fn contain(child: &Child) -> Result<Job> {
    let job = Job::kill_on_close()?;
    job.assign(child)?;
    Ok(job)
}

fn kill_quietly(child: &mut Child) {
    if let Err(error) = child.kill() {
        eprintln!("deckhand: could not stop a half-started worker: {error}");
    }
}

struct WorkerChild {
    child: Child,
    job: Job,
}

impl WorkerProcess for WorkerChild {
    fn terminate(&mut self) {
        let Err(error) = self.job.terminate() else { return };
        eprintln!("deckhand: {error}");
        if let Err(error) = self.child.kill() {
            eprintln!("deckhand: could not stop the desktop worker: {error}");
        }
    }
}
