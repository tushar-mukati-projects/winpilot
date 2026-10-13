#![cfg(test)]

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};

use serde_json::{Value, json};
use winpilot_fixture::{FixtureProcess, Placement};

struct Mcp {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    timings: Vec<(String, Duration)>,
    _home: TempDir,
}

struct Reply {
    text: String,
    images: usize,
    is_error: bool,
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("winpilot-e2e-{name}-{}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Mcp {
    fn start() -> Self {
        let home = TempDir::new("home");
        let mut child = Command::new(env!("CARGO_BIN_EXE_winpilot"))
            .arg("mcp")
            .env("APPDATA", home.0.join("roaming"))
            .env("LOCALAPPDATA", home.0.join("local"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut mcp = Self { child, stdin, stdout, next_id: 0, timings: Vec::new(), _home: home };
        mcp.request(
            "initialize",
            &json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "winpilot-e2e", "version": "0" }
            }),
        );
        mcp.notify("notifications/initialized");
        mcp
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.stdin.as_mut().unwrap();
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str) {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }));
    }

    fn close_input_and_wait(&mut self, patience: Duration) -> bool {
        drop(self.stdin.take());
        let started = Instant::now();
        while started.elapsed() < patience {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn request(&mut self, method: &str, params: &Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(self.stdout.read_line(&mut line).unwrap() > 0, "the server closed stdout");
            let reply: Value = serde_json::from_str(&line).unwrap();
            if reply["id"] == json!(id) {
                assert!(reply.get("error").is_none(), "protocol error: {reply}");
                return reply["result"].clone();
            }
        }
    }

    fn call(&mut self, tool: &str, arguments: &Value) -> Reply {
        let started = Instant::now();
        let result = self.request("tools/call", &json!({ "name": tool, "arguments": *arguments }));
        self.timings.push((tool.to_string(), started.elapsed()));
        let content = result["content"].as_array().cloned().unwrap_or_default();
        let text = content.iter().filter_map(|block| block["text"].as_str()).collect::<Vec<_>>().join("\n");
        let images = content.iter().filter(|block| block["type"] == "image").count();
        Reply { text, images, is_error: result["isError"] == json!(true) }
    }

    fn ok(&mut self, tool: &str, arguments: &Value) -> String {
        let reply = self.call(tool, arguments);
        assert!(!reply.is_error, "{tool} failed:\n{}", reply.text);
        reply.text
    }

    fn report_timings(&self) {
        for (tool, took) in &self.timings {
            eprintln!("{tool:>10} {:>6} ms", took.as_millis());
        }
    }
}

impl Drop for Mcp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Fixture {
    _process: FixtureProcess,
    title: String,
}

impl Fixture {
    fn launch(tag: &str) -> Option<Self> {
        let Some(exe) = winpilot_fixture::fixture_exe() else {
            eprintln!("skipping: build the fixture first (cargo build -p winpilot-fixture)");
            return None;
        };
        let title = format!("WinPilot Fixture {tag} {}", std::process::id());
        let process = FixtureProcess::launch(&exe, &title, Placement::OnScreen)?;
        Some(Self { _process: process, title })
    }

    fn window(&self) -> String {
        format!("window:\"{}\"", self.title)
    }

    fn target(&self, element: &str) -> String {
        format!("{} >> {element}", self.window())
    }
}

fn last_box(bytes: &[u8]) -> Option<[u8; 4]> {
    let mut position = 0usize;
    let mut last = None;
    while let Some(header) = bytes.get(position..position + 8) {
        let size = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        last = Some(header[4..8].try_into().unwrap());
        if size < 8 {
            break;
        }
        position += size;
    }
    last
}

fn wait_for_window(mcp: &mut Mcp, fixture: &Fixture) {
    act(mcp, &json!([{ "do": "wait", "until": "window", "window": fixture.title, "timeout_ms": 10000 }]));
}

fn act(mcp: &mut Mcp, steps: &Value) -> String {
    mcp.ok("act", &json!({ "steps": steps }))
}

fn status(mcp: &mut Mcp, fixture: &Fixture, id: u32) -> String {
    mcp.ok("observe", &json!({ "mode": "text", "target": fixture.target(&format!("id:{id}")), "ocr": "never" }))
}

#[test]
#[ignore = "drives the real desktop through the fixture app"]
fn lists_two_tools_and_reports_status() {
    let mut mcp = Mcp::start();
    let tools = mcp.request("tools/list", &json!({}));
    assert_eq!(tools["tools"].as_array().map(Vec::len), Some(2), "{tools}");
    let status = mcp.ok("observe", &json!({ "mode": "status" }));
    assert!(status.starts_with("version: "), "{status}");
}

#[test]
#[ignore = "drives the real desktop through the fixture app"]
fn batch_of_clicks_typing_set_and_keys() {
    let Some(fixture) = Fixture::launch("batch") else { return };
    let mut mcp = Mcp::start();
    wait_for_window(&mut mcp, &fixture);
    let observed = mcp.ok("observe", &json!({ "window": fixture.title }));
    assert!(observed.contains("\"Click me\""), "{observed}");
    let report = act(
        &mut mcp,
        &json!([
            { "do": "click", "target": fixture.target("name:\"Click me\""), "how": "mouse" },
            { "do": "click", "target": fixture.target("name:\"Click me\""), "how": "mouse" },
            { "do": "set", "target": fixture.target("id:2001"), "value": "Ada Lovelace" },
            { "do": "type", "target": fixture.target("id:2002"), "text": "line one\nline two" },
            { "do": "set", "target": fixture.target("id:2005"), "checked": true },
            { "do": "set", "target": fixture.target("id:2007"), "option": "Blue" },
            { "do": "key", "target": fixture.target("id:2002"), "keys": "f5" }
        ]),
    );
    assert!(!report.contains('✗'), "{report}");
    assert!(status(&mut mcp, &fixture, 2004).contains("Clicks: 2"));
    assert!(status(&mut mcp, &fixture, 2006).contains("Feature: on"));
    assert!(status(&mut mcp, &fixture, 2008).contains("Colour: Blue"));
    assert!(status(&mut mcp, &fixture, 2020).contains("Last key: F5"));
    let name = mcp.ok("observe", &json!({ "mode": "text", "target": fixture.target("id:2001") }));
    assert!(name.contains("Ada Lovelace"), "{name}");
    mcp.report_timings();
}

#[test]
#[ignore = "drives the real desktop through the fixture app"]
fn waits_for_a_delayed_button() {
    let Some(fixture) = Fixture::launch("wait") else { return };
    let mut mcp = Mcp::start();
    wait_for_window(&mut mcp, &fixture);
    let report = act(
        &mut mcp,
        &json!([
            { "do": "click", "target": fixture.target("name:\"Show later\"") },
            { "do": "wait", "until": "appears", "target": fixture.target("name:Appeared"), "timeout_ms": 5000 },
            { "do": "click", "target": fixture.target("name:Appeared") }
        ]),
    );
    assert!(!report.contains('✗'), "{report}");
    assert!(status(&mut mcp, &fixture, 2017).contains("Appeared clicked"));
    mcp.report_timings();
}

#[test]
#[ignore = "drives the real desktop through the fixture app"]
fn picks_files_in_open_and_save_dialogs() {
    let Some(fixture) = Fixture::launch("files") else { return };
    let files = TempDir::new("files");
    let first = files.0.join("first.txt");
    let second = files.0.join("second.txt");
    fs::write(&first, "one").unwrap();
    fs::write(&second, "two").unwrap();
    let mut mcp = Mcp::start();
    wait_for_window(&mut mcp, &fixture);
    act(
        &mut mcp,
        &json!([{ "do": "pick", "paths": [first.display().to_string(), second.display().to_string()],
                  "open_with": fixture.target("name:\"Open file…\"") }]),
    );
    let opened = status(&mut mcp, &fixture, 2012);
    assert!(opened.contains("first.txt") && opened.contains("second.txt"), "{opened}");
    let report = files.0.join("report.txt");
    act(
        &mut mcp,
        &json!([{ "do": "pick", "paths": [report.display().to_string()],
                  "open_with": fixture.target("name:\"Save file…\"") }]),
    );
    assert!(status(&mut mcp, &fixture, 2014).contains("report.txt"));
    assert!(!report.exists(), "the fixture must not write files");
    mcp.report_timings();
}

#[test]
#[ignore = "drives the real desktop through the fixture app"]
fn screenshot_query_and_window_actions() {
    let Some(fixture) = Fixture::launch("windows") else { return };
    let mut mcp = Mcp::start();
    wait_for_window(&mut mcp, &fixture);
    act(&mut mcp, &json!([{ "do": "focus", "target": fixture.window() }]));
    let shot = mcp.call("observe", &json!({ "mode": "screenshot", "target": fixture.title, "marks": true }));
    assert!(!shot.is_error && shot.images == 1, "{}", shot.text);
    let found = mcp.ok("observe", &json!({ "query": "name:\"Click me\"", "all_windows": true }));
    assert!(found.starts_with('e'), "{found}");
    act(&mut mcp, &json!([{ "do": "window", "action": "minimize", "target": fixture.title }]));
    act(&mut mcp, &json!([{ "do": "window", "action": "restore", "target": fixture.title }]));
    act(
        &mut mcp,
        &json!([{ "do": "window", "action": "move", "target": fixture.title,
                  "rect": { "x": 200, "y": 160, "width": 700, "height": 760 } }]),
    );
    act(
        &mut mcp,
        &json!([
            { "do": "window", "action": "close", "target": fixture.title },
            { "do": "wait", "until": "window_closed", "window": fixture.title, "timeout_ms": 5000 }
        ]),
    );
    let listed = mcp.ok("observe", &json!({ "mode": "windows" }));
    assert!(!listed.contains(&fixture.title), "{listed}");
    mcp.report_timings();
}

#[test]
#[ignore = "records the screen"]
fn closing_the_server_finalizes_a_running_recording() {
    let folder = TempDir::new("record-shutdown");
    let path = folder.0.join("shutdown.mp4");
    let mut mcp = Mcp::start();
    let region = json!({ "x": 0, "y": 0, "width": 320, "height": 200 });
    let started = mcp.ok(
        "observe",
        &json!({ "mode": "record", "action": "start", "region": region, "path": path.display().to_string() }),
    );
    assert!(started.starts_with("recording to"), "{started}");
    std::thread::sleep(Duration::from_millis(1500));
    assert!(mcp.close_input_and_wait(Duration::from_secs(10)), "the server did not exit after its input closed");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(last_box(&bytes).as_ref(), Some(b"mfra"), "the recording was not finalized");
}
