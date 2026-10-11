use std::time::Instant;

use deckhand_core::{
    Context, ErrorCode, Result,
    format::ObserveFormat,
    protocol::{
        ActArgs, Call, ClipboardArgs, Condition, FindArgs, ObserveArgs, PickFileArgs, ReadTextArgs, Reply,
        ScreenshotArgs, StatusArgs, WindowsArgs,
    },
};
use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Implementation, MetaObject, ServerCapabilities, ServerConfig},
    service::RequestContext,
    tool, tool_handler, tool_router,
};

use crate::{
    audit::{Audit, Entry},
    config::{self, LoadedConfig},
    render::{self, Extras},
    status, summary,
    supervisor::Supervisor,
};

const SERVER_NAME: &str = "deckhand";
const ALWAYS_LOAD: &str = "anthropic/alwaysLoad";
const DEFAULT_OBSERVE_MAX: u32 = 300;

const OBSERVE: &str = "Read a window as a compact list of elements with refs (e12). Default: foreground window. Use refs or \
selectors in act. query keeps only matching elements, all adds static text, coords adds screen rectangles.";
const ACT: &str = "Run steps in one call (click/type/key/scroll/select/toggle/wait_for…). Targets: e12, window refs, \
selectors like role:button name:\"Save\", or screenshot points. Returns what changed. Batch several steps per call.";
const FIND: &str = "Find elements by selector or text (role:button name~:save, or just Save) without listing the whole \
window. Returns matching refs with role, name and path. all_windows searches every app window.";
const SCREENSHOT: &str = "Capture the foreground window, a window or element, a region or a monitor as an image with an \
img id (i7). Use only when observe's text is not enough. Point targets need that id: {\"x\":640,\"y\":360,\"img\":\"i7\"}. \
marks labels element refs.";
const WAIT_FOR: &str = "Wait until an element appears or disappears, a window opens or closes, text shows up, or the UI \
goes idle. Event-driven on the desktop side: default 5 s, max 60 s. Use instead of sleeping or polling with observe.";
const WINDOWS: &str = "List, focus, launch, close, minimize, maximize, restore or move windows. launch takes a \
Start-menu name like Notepad, an app id or an exe path, waits for the window and returns its elements.";
const PICK_FILE: &str = "Fill a native Open/Save dialog in one call: optionally click open_with first (e.g. an Upload \
button), set the paths, confirm and check the dialog closed. Use for every upload or save-as dialog.";
const READ_TEXT: &str = "Read the full text of an element, window or region: UI Automation text first, OCR as fallback. \
Use for long documents, logs and values that observe shortens.";
const CLIPBOARD: &str = "Get or set the clipboard text.";
const STATUS: &str = "Health check: stopped by the kill hotkey or not, user activity, worker restarts, recent timings \
and errors.";

const INSTRUCTIONS: &str = "Deckhand controls the Windows desktop through UI Automation.\n\
- observe once, then act by ref (e12) or selector (role:button name:\"Save\"); refs stay valid while the element exists.\n\
- Batch several steps into one act call. act returns a diff of what changed, so observe again only when you need the full list.\n\
- When you know the app, act by selector without observing first; find locates elements without listing a whole window.\n\
- Use pick_file for any Open, Save or upload dialog, and windows launch to open apps.\n\
- Use wait_for instead of sleeping or polling.\n\
- Take a screenshot only when the text view is not enough. Point targets need its img id: {\"x\":640,\"y\":360,\"img\":\"i7\"}.\n\
- The user can stop all input at once with the kill hotkey ({hotkey}); act then fails with stopped_by_user. Tell the user instead of retrying.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub log_text: bool,
    pub config_problem: Option<String>,
    pub kill_hotkey: String,
}

impl Settings {
    pub fn new(loaded: &LoadedConfig) -> Self {
        Self {
            log_text: loaded.config.log.log_text,
            config_problem: loaded.problem.clone(),
            kill_hotkey: loaded.config.guard.kill_hotkey.clone(),
        }
    }
}

pub struct DeckhandServer {
    supervisor: Supervisor,
    audit: Audit,
    settings: Settings,
    instructions: String,
    started: Instant,
    tool_router: ToolRouter<Self>,
}

struct Answer {
    outcome: Result<Reply>,
    worker_problem: Option<String>,
}

impl Answer {
    const fn plain(outcome: Result<Reply>) -> Self {
        Self { outcome, worker_problem: None }
    }
}

pub async fn serve_stdio(server: DeckhandServer) -> Result<()> {
    let running = server.serve(rmcp::transport::stdio()).await.context("starting the MCP session")?;
    running.waiting().await.context("running the MCP session")?;
    Ok(())
}

impl DeckhandServer {
    pub fn new(supervisor: Supervisor, audit: Audit, settings: Settings) -> Self {
        let instructions = INSTRUCTIONS.replace("{hotkey}", &settings.kill_hotkey);
        let started = Instant::now();
        Self { supervisor, audit, settings, instructions, started, tool_router: Self::tool_router() }
    }

    async fn run(&self, call: Call, view: ObserveFormat, context: &RequestContext<RoleServer>) -> CallToolResult {
        let started = Instant::now();
        let tool = call.name();
        let summary = summary::describe(&call, self.settings.log_text);
        let answer = self.answer(call, context).await;
        let failure = answer.outcome.as_ref().err().map(|error| error.code);
        self.audit.record(&Entry { tool, summary: &summary, failure, elapsed: started.elapsed() });
        render::outcome(answer.outcome, &self.extras(view, answer.worker_problem.as_deref()))
    }

    async fn answer(&self, call: Call, context: &RequestContext<RoleServer>) -> Answer {
        if let Some(problem) = &self.settings.config_problem {
            return self.refuse(&call, problem);
        }
        let is_status = call == Call::Status;
        match self.supervisor.call(call, context.ct.cancelled()).await {
            Err(error) if is_status && error.code != ErrorCode::Cancelled => {
                Answer { outcome: Ok(self.server_status()), worker_problem: Some(error.message) }
            }
            outcome => Answer::plain(outcome),
        }
    }

    fn refuse(&self, call: &Call, problem: &str) -> Answer {
        match call {
            Call::Status => Answer::plain(Ok(self.server_status())),
            _ => Answer::plain(Err(config::refusal(problem))),
        }
    }

    fn server_status(&self) -> Reply {
        let report =
            status::server_report(self.started.elapsed(), &self.settings.kill_hotkey, self.supervisor.restarts());
        Reply::Status(report)
    }

    fn extras<'a>(&'a self, observe: ObserveFormat, worker_problem: Option<&'a str>) -> Extras<'a> {
        Extras {
            observe,
            restarts: self.supervisor.restarts(),
            config_problem: self.settings.config_problem.as_deref(),
            worker_problem,
        }
    }
}

#[tool_router]
impl DeckhandServer {
    #[tool(
        description = OBSERVE,
        annotations(read_only_hint = true, open_world_hint = false),
        meta = always_load()
    )]
    async fn observe(
        &self,
        Parameters(args): Parameters<ObserveArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let max = args.max.unwrap_or(DEFAULT_OBSERVE_MAX) as usize;
        let view = ObserveFormat { all: args.all, coords: args.coords, max };
        self.run(Call::Observe(args), view, &context).await
    }

    #[tool(description = ACT, annotations(open_world_hint = false), meta = always_load())]
    async fn act(&self, Parameters(args): Parameters<ActArgs>, context: RequestContext<RoleServer>) -> CallToolResult {
        self.run(Call::Act(args), ObserveFormat::default(), &context).await
    }

    #[tool(
        description = FIND,
        annotations(read_only_hint = true, open_world_hint = false),
        meta = always_load()
    )]
    async fn find(
        &self,
        Parameters(args): Parameters<FindArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::Find(args), ObserveFormat::default(), &context).await
    }

    #[tool(
        description = SCREENSHOT,
        annotations(read_only_hint = true, open_world_hint = false),
        meta = always_load()
    )]
    async fn screenshot(
        &self,
        Parameters(args): Parameters<ScreenshotArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::Screenshot(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = WAIT_FOR, annotations(read_only_hint = true, open_world_hint = false))]
    async fn wait_for(
        &self,
        Parameters(args): Parameters<Condition>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::WaitFor(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = WINDOWS, annotations(open_world_hint = false))]
    async fn windows(
        &self,
        Parameters(args): Parameters<WindowsArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::Windows(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = PICK_FILE, annotations(open_world_hint = false))]
    async fn pick_file(
        &self,
        Parameters(args): Parameters<PickFileArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::PickFile(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = READ_TEXT, annotations(read_only_hint = true, open_world_hint = false))]
    async fn read_text(
        &self,
        Parameters(args): Parameters<ReadTextArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::ReadText(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = CLIPBOARD, annotations(open_world_hint = false))]
    async fn clipboard(
        &self,
        Parameters(args): Parameters<ClipboardArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::Clipboard(args), ObserveFormat::default(), &context).await
    }

    #[tool(description = STATUS, annotations(read_only_hint = true, open_world_hint = false))]
    async fn status(
        &self,
        Parameters(StatusArgs {}): Parameters<StatusArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.run(Call::Status, ObserveFormat::default(), &context).await
    }
}

#[tool_handler(router = self.tool_router)]
#[expect(clippy::unused_async_trait_impl)]
impl ServerHandler for DeckhandServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(SERVER_NAME, crate::VERSION))
            .with_instructions(self.instructions.clone())
    }
}

fn always_load() -> MetaObject {
    let mut meta = MetaObject::new();
    meta.insert(ALWAYS_LOAD.to_string(), serde_json::Value::Bool(true));
    meta
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::Duration};

    use deckhand_core::Config;
    use rmcp::model::Tool;
    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf};

    use super::*;
    use crate::testing::{FakeSpawner, FakeWorker, echo_worker};

    const TOOLS: [&str; 10] = [
        "act",
        "clipboard",
        "find",
        "observe",
        "pick_file",
        "read_text",
        "screenshot",
        "status",
        "wait_for",
        "windows",
    ];
    const READ_ONLY: [&str; 6] = ["find", "observe", "read_text", "screenshot", "status", "wait_for"];
    const ALWAYS_LOADED: [&str; 4] = ["act", "find", "observe", "screenshot"];
    const BAD_CONFIG: &str = "C:\\cfg\\config.toml is invalid (line 3: invalid type)";

    fn settings() -> Settings {
        Settings { log_text: false, config_problem: None, kill_hotkey: "ctrl+alt+shift+backspace".to_string() }
    }

    fn server() -> (DeckhandServer, FakeSpawner) {
        server_with(settings(), echo_worker)
    }

    fn server_with(
        settings: Settings,
        behaviour: impl Fn(FakeWorker) + Send + Sync + 'static,
    ) -> (DeckhandServer, FakeSpawner) {
        let spawner = FakeSpawner::new(move |_, worker| behaviour(worker));
        let supervisor = Supervisor::new(Box::new(spawner.clone()), Config::default());
        (DeckhandServer::new(supervisor, Audit::disabled(), settings), spawner)
    }

    fn text_of(response: &Value) -> &str {
        response["result"]["content"][0]["text"].as_str().unwrap()
    }

    fn tools() -> Vec<Tool> {
        DeckhandServer::tool_router().list_all()
    }

    fn tool(name: &str) -> Tool {
        tools().into_iter().find(|tool| tool.name == name).unwrap()
    }

    #[test]
    fn exactly_the_ten_tools_are_listed() {
        let names: Vec<String> = tools().iter().map(|tool| tool.name.to_string()).collect();
        assert_eq!(names, TOOLS);
    }

    #[test]
    fn every_schema_is_an_object() {
        for tool in tools() {
            assert_eq!(tool.input_schema.get("type"), Some(&json!("object")), "{}", tool.name);
            assert!(tool.output_schema.is_none(), "{}", tool.name);
        }
        let act = tool("act");
        assert!(act.input_schema["properties"].get("steps").is_some());
        let wait = tool("wait_for");
        assert!(wait.input_schema["properties"].get("until").is_some());
    }

    #[test]
    fn descriptions_are_short() {
        for tool in tools() {
            let description = tool.description.unwrap_or_default();
            assert!(!description.is_empty(), "{}", tool.name);
            assert!(description.chars().count() <= 350, "{} has {} chars", tool.name, description.chars().count());
        }
    }

    #[test]
    fn annotations_match_the_spec() {
        for tool in tools() {
            let name = tool.name.as_ref();
            let annotations = tool.annotations.clone().unwrap();
            let read_only = READ_ONLY.contains(&name).then_some(true);
            assert_eq!(annotations.read_only_hint, read_only, "{name}");
            assert_eq!(annotations.destructive_hint, None, "{name} must keep the destructive default");
            assert_eq!(annotations.open_world_hint, Some(false), "{name}");
        }
    }

    #[test]
    fn core_tools_are_always_loaded() {
        for tool in tools() {
            let name = tool.name.as_ref();
            let flag = tool.meta.as_ref().and_then(|meta| meta.get(ALWAYS_LOAD)).cloned();
            let expected = ALWAYS_LOADED.contains(&name).then_some(Value::Bool(true));
            assert_eq!(flag, expected, "{name}");
        }
    }

    #[test]
    fn instructions_are_short_and_name_the_hotkey() {
        let (server, _) = server();
        let info = server.get_info();
        let instructions = info.instructions.unwrap();
        assert!(instructions.lines().count() <= 12);
        assert!(instructions.contains("ctrl+alt+shift+backspace"));
        assert_eq!(info.server_info.name, "deckhand");
        assert_eq!(info.server_info.version, crate::VERSION);
        assert!(info.capabilities.tools.is_some());
    }

    struct RawClient {
        lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
        writer: WriteHalf<DuplexStream>,
    }

    impl RawClient {
        fn connect(server: DeckhandServer) -> Self {
            let (client, transport) = tokio::io::duplex(1 << 20);
            tokio::spawn(async move {
                if let Ok(running) = server.serve(transport).await {
                    let _ = running.waiting().await;
                }
            });
            let (reader, writer) = tokio::io::split(client);
            Self { lines: BufReader::new(reader).lines(), writer }
        }

        async fn send(&mut self, message: &Value) {
            let mut line = message.to_string();
            line.push('\n');
            self.writer.write_all(line.as_bytes()).await.unwrap();
            self.writer.flush().await.unwrap();
        }

        async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
            self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})).await;
            loop {
                let next = tokio::time::timeout(Duration::from_secs(10), self.lines.next_line()).await;
                let line = next.unwrap().unwrap().unwrap();
                let message: Value = serde_json::from_str(&line).unwrap();
                if message["id"] == json!(id) {
                    return message;
                }
            }
        }

        async fn initialize(&mut self) -> Value {
            let params = json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "deckhand-test", "version": "0"}
            });
            let response = self.request(1, "initialize", params).await;
            self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await;
            response
        }
    }

    #[tokio::test]
    async fn initialize_reports_name_version_and_instructions() {
        let (server, _) = server();
        let mut client = RawClient::connect(server);
        let response = client.initialize().await;
        let result = &response["result"];
        assert_eq!(result["serverInfo"]["name"], "deckhand");
        assert_eq!(result["serverInfo"]["version"], crate::VERSION);
        assert!(result["capabilities"]["tools"].is_object());
        assert!(result["instructions"].as_str().unwrap().contains("pick_file"));
    }

    #[tokio::test]
    async fn tools_list_over_the_wire_carries_annotations_and_meta() {
        let (server, _) = server();
        let mut client = RawClient::connect(server);
        client.initialize().await;
        let response = client.request(2, "tools/list", json!({})).await;
        let listed = response["result"]["tools"].as_array().unwrap();
        let names: BTreeSet<&str> = listed.iter().map(|tool| tool["name"].as_str().unwrap()).collect();
        assert_eq!(names, TOOLS.into_iter().collect());
        let observe = listed.iter().find(|tool| tool["name"] == "observe").unwrap();
        assert_eq!(observe["_meta"][ALWAYS_LOAD], true);
        assert_eq!(observe["annotations"]["readOnlyHint"], true);
        assert_eq!(observe["annotations"]["openWorldHint"], false);
    }

    #[tokio::test]
    async fn bad_arguments_fail_as_a_tool_error_without_starting_a_worker() {
        let (server, spawner) = server();
        let mut client = RawClient::connect(server);
        client.initialize().await;
        let params = json!({"name": "observe", "arguments": {"windw": "Notepad"}});
        let response = client.request(3, "tools/call", params).await;
        assert_eq!(response["result"]["isError"], true);
        assert_eq!(spawner.spawned(), 0);
    }

    #[tokio::test]
    async fn status_call_round_trips_through_the_worker() {
        let (server, spawner) = server();
        let mut client = RawClient::connect(server);
        client.initialize().await;
        let response = client.request(4, "tools/call", json!({"name": "status", "arguments": {}})).await;
        let result = &response["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(result["content"][0]["type"], "text");
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn status_still_answers_when_the_worker_cannot_start() {
        let (server, spawner) = server_with(settings(), drop);
        let mut client = RawClient::connect(server);
        client.initialize().await;
        for id in [5, 6] {
            let response = client.request(id, "tools/call", json!({"name": "status", "arguments": {}})).await;
            assert_eq!(response["result"]["isError"], false);
            let text = text_of(&response);
            assert!(text.starts_with(&format!("version: {}", crate::VERSION)), "{text}");
            let worker = text.lines().last().unwrap();
            assert!(worker.starts_with("worker: the desktop worker failed to start: it exited"), "{text}");
            assert!(worker.contains("retrying in"), "{text}");
        }
        assert_eq!(spawner.spawned(), 1);
        let response = client.request(7, "tools/call", json!({"name": "observe", "arguments": {}})).await;
        assert_eq!(response["result"]["isError"], true);
        assert!(text_of(&response).starts_with("error[target_not_responding]: the desktop worker failed to start"));
    }

    #[tokio::test]
    async fn invalid_config_refuses_every_tool_but_status() {
        let refusing = Settings { config_problem: Some(BAD_CONFIG.to_string()), ..settings() };
        let (server, spawner) = server_with(refusing, echo_worker);
        let mut client = RawClient::connect(server);
        client.initialize().await;
        let calls = [
            ("observe", json!({})),
            ("act", json!({"steps": [{"do": "key", "keys": "ctrl+s"}]})),
            ("windows", json!({"action": "list"})),
            ("clipboard", json!({"action": "get"})),
        ];
        for (id, (name, arguments)) in (10..).zip(calls) {
            let response = client.request(id, "tools/call", json!({"name": name, "arguments": arguments})).await;
            assert_eq!(response["result"]["isError"], true, "{name}");
            assert_eq!(
                text_of(&response),
                format!("error[invalid_argument]: {BAD_CONFIG}; fix it or delete it to use the defaults"),
                "{name}"
            );
        }
        let response = client.request(20, "tools/call", json!({"name": "status", "arguments": {}})).await;
        assert_eq!(response["result"]["isError"], false);
        let text = text_of(&response);
        assert!(text.lines().last().unwrap().starts_with(&format!("config: {BAD_CONFIG}; fix it")), "{text}");
        assert_eq!(spawner.spawned(), 0);
    }
}
