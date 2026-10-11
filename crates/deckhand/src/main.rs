mod audit;
mod config;
mod job;
mod link;
mod mcp;
mod render;
mod spawn;
mod summary;
mod supervisor;
#[cfg(test)]
mod testing;

use std::{process::ExitCode, time::Duration};

use deckhand_core::{Context, Result};

use crate::{
    audit::Audit,
    mcp::{DeckhandServer, Settings},
    spawn::ProcessSpawner,
    supervisor::Supervisor,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const USAGE: &str = "usage: deckhand [mcp | worker | --version]
  mcp        run the MCP server on stdio (the default)
  worker     run the desktop worker (the server starts it)
  --version  print the version";
const RUNTIME_SHUTDOWN: Duration = Duration::from_secs(2);

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
        Mode::Worker => exit_with(deckhand_worker::run()),
        Mode::Version => {
            eprintln!("deckhand {VERSION}");
            ExitCode::SUCCESS
        }
        Mode::Usage => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
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
            eprintln!("deckhand: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve() -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("deckhand-mcp")
        .enable_all()
        .build()
        .context("starting the async runtime")?;
    let outcome = runtime.block_on(run_server());
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
    outcome
}

async fn run_server() -> Result<()> {
    let loaded = config::load();
    let audit = Audit::open(&loaded.config.log);
    let settings = Settings::new(&loaded);
    let supervisor = Supervisor::new(Box::new(ProcessSpawner), loaded.config);
    mcp::serve_stdio(DeckhandServer::new(supervisor, audit, settings)).await
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
