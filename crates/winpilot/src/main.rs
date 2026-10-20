mod app_data;
mod audit;
mod backoff;
mod config;
mod deadline;
mod death;
mod job;
mod lifecycle;
mod link;
mod mcp;
mod render;
mod spawn;
mod status;
mod summary;
mod supervisor;
#[cfg(test)]
mod testing;
mod wsl_paths;

use std::{process::ExitCode, time::Duration};

use winpilot_core::{Context, Result, VERSION};

use crate::{
    audit::Audit,
    mcp::{Settings, WinPilotServer},
    spawn::ProcessSpawner,
    supervisor::Supervisor,
};

const BUILD_ID: &str = env!("WINPILOT_BUILD_ID");
const USAGE: &str = "usage: winpilot [mcp | worker | --version]
  mcp        run the MCP server on stdio (the default)
  worker     run the desktop worker (the server starts it)
  --version  print the version";
const RUNTIME_SHUTDOWN: Duration = Duration::from_secs(2);
const RUNTIME_THREADS: usize = 2;
const USAGE_EXIT_CODE: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Mode {
    Server,
    Worker,
    Version,
    Usage,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_mode(&args) {
        Mode::Server => exit_with(serve()),
        Mode::Worker => exit_with(winpilot_worker::run(BUILD_ID)),
        Mode::Version => {
            eprintln!("winpilot {VERSION} (build {BUILD_ID})");
            ExitCode::SUCCESS
        }
        Mode::Usage => {
            eprintln!("{USAGE}");
            ExitCode::from(USAGE_EXIT_CODE)
        }
    }
}

fn parse_mode(args: &[String]) -> Mode {
    match args {
        [] => Mode::Server,
        [only] => match only.as_str() {
            "mcp" => Mode::Server,
            "worker" => Mode::Worker,
            "--version" => Mode::Version,
            _ => Mode::Usage,
        },
        _ => Mode::Usage,
    }
}

fn exit_with(outcome: Result<()>) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("winpilot: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(RUNTIME_THREADS)
        .thread_name("winpilot-mcp")
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    let outcome = runtime.block_on(run_server());
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
    outcome
}

async fn run_server() -> Result<()> {
    let loaded = config::load();
    let audit = Audit::open();
    if loaded.problem.is_none() {
        audit.prune_expired(loaded.config.log.retention_days);
    }
    let settings = Settings::new(&loaded);
    let supervisor = Supervisor::new(Box::new(ProcessSpawner), loaded.config);
    let shutdown = supervisor.shutdown_handle();
    let served = mcp::serve_stdio(WinPilotServer::new(supervisor, audit, settings)).await;
    shutdown.shut_down();
    served
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_string()).collect()
    }

    #[test]
    fn no_args_or_mcp_runs_the_server() {
        assert_eq!(parse_mode(&args(&[])), Mode::Server);
        assert_eq!(parse_mode(&args(&["mcp"])), Mode::Server);
    }

    #[test]
    fn worker_and_version_are_recognised() {
        assert_eq!(parse_mode(&args(&["worker"])), Mode::Worker);
        assert_eq!(parse_mode(&args(&["--version"])), Mode::Version);
    }

    #[test]
    fn anything_else_is_a_usage_error() {
        assert_eq!(parse_mode(&args(&["serve"])), Mode::Usage);
        assert_eq!(parse_mode(&args(&["mcp", "extra"])), Mode::Usage);
        assert_eq!(parse_mode(&args(&["-h"])), Mode::Usage);
    }
}
