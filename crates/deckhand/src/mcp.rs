use std::time::Instant;

use deckhand_core::{
    Context, ErrorCode, Result,
    format::ObserveFormat,
    protocol::{ActArgs, Call, ObserveArgs, Reply},
};
use rmcp::{
    ErrorData, RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolResult, Implementation, InitializeRequestParams, InitializeResult, MetaObject, ServerCapabilities,
        ServerConfig,
    },
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

const OBSERVE: &str = "See the desktop. mode: elements (default: a window as element lines with refs like e12; \
query filters; all_windows with query searches every window), text (full text, UI Automation first, OCR fallback), \
screenshot (an image with an img id for point targets), record (start, stop or status an MP4 or GIF), windows, \
monitors, apps (installed; query filters), clipboard or status. window picks the window for elements and text; \
target picks an element or window for text, screenshot and record. A field that belongs to another mode is an error.";
const ACT: &str = "Do things: up to 50 steps per call, each {\"do\":...}, then a diff of what changed (return: diff, \
observe, screenshot or none). Steps: click, type, key (press/down/up), mouse (move/down/up/scroll/drag), set \
(value/option/checked/expanded/selected), focus, window (launch/close/minimize/maximize/restore/move), pick (file \
dialogs), wait (ms, or until appears/disappears/window/window_closed/text/idle), clipboard, notify. Targets: refs \
(e12, w123), selectors like role:button name:\"Save\", or {\"x\":..,\"y\":..,\"img\":\"i3\"}. background:true uses UI \
Automation only, never the real mouse, keyboard or focus.";

const INSTRUCTIONS: &str = "Deckhand controls the Windows desktop through UI Automation with two tools, observe and act.\n\
- observe mode=elements (the default) lists a window as lines with refs (e12); mode=text reads it whole. Other modes: screenshot, record, windows, monitors, apps, clipboard, status.\n\
- act runs several steps per call by ref (e12) or selector (role:button name:\"Save\") and returns a diff, so observe again only when you need the full list.\n\
- When you know the app, act by selector without observing first; observe all_windows with a query finds elements in every window.\n\
- Use the pick step for Open, Save or upload dialogs, window action=launch to open apps, and wait instead of sleeping.\n\
- Take a screenshot only when text is not enough. Point targets need its img id: {\"x\":640,\"y\":360,\"img\":\"i7\"}.\n\
- act background:true works through UI Automation alone, without the mouse, keyboard or focus; windows on other virtual desktops always get that.\n\
- The user can stop all input with the kill hotkey ({hotkey}); act then fails with stopped_by_user. Tell the user instead of retrying.";

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
        let tool = call.tool();
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
        let is_status = call.is_status();
        match self.supervisor.call(call, context.ct.cancelled()).await {
            Err(error) if is_status && error.code != ErrorCode::Cancelled => {
                Answer { outcome: Ok(self.server_status()), worker_problem: Some(error.message) }
            }
            outcome => Answer::plain(outcome),
        }
    }

    fn refuse(&self, call: &Call, problem: &str) -> Answer {
        if call.is_status() {
            return Answer::plain(Ok(self.server_status()));
        }
        Answer::plain(Err(config::refusal(problem)))
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
        annotations(read_only_hint = false, destructive_hint = false, open_world_hint = false),
        meta = always_load()
    )]
    async fn observe(
        &self,
        Parameters(args): Parameters<ObserveArgs>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let view = args.view();
        self.run(Call::Observe(args), view, &context).await
    }

    #[tool(description = ACT, annotations(open_world_hint = false), meta = always_load())]
    async fn act(&self, Parameters(args): Parameters<ActArgs>, context: RequestContext<RoleServer>) -> CallToolResult {
        self.run(Call::Act(args), ObserveFormat::default(), &context).await
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

    async fn initialize(
        &self,
        request: InitializeRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<InitializeResult, ErrorData> {
        self.supervisor.set_agent(&request.client_info.name);
        context.peer.set_peer_info(request.clone());
        self.negotiate_initialize(&request)
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

    const TOOLS: [&str; 2] = ["act", "observe"];
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
    fn exactly_the_two_tools_are_listed() {
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
        for field in ["steps", "return", "stop_on_error", "background"] {
            assert!(act.input_schema["properties"].get(field).is_some(), "act lacks {field}");
        }
        let observe = tool("observe");
        let observe_fields = [
            "mode",
            "window",
            "target",
            "query",
            "all_windows",
            "all",
            "coords",
            "max",
            "region",
            "monitor",
            "ocr",
            "max_chars",
            "preset",
            "quality",
            "video_quality",
            "max_width",
            "format",
            "marks",
            "action",
            "fps",
            "max_seconds",
            "cursor",
            "path",
        ];
        for field in observe_fields {
            assert!(observe.input_schema["properties"].get(field).is_some(), "observe lacks {field}");
        }
    }

    #[test]
    fn the_observe_schema_lists_every_mode() {
        let schema = tool("observe").input_schema;
        let modes = schema.get("$defs").and_then(|defs| defs.get("ObserveMode")).and_then(|mode| mode.get("enum"));
        assert_eq!(
            modes.cloned(),
            Some(json!([
                "elements",
                "text",
                "screenshot",
                "record",
                "windows",
                "monitors",
                "apps",
                "clipboard",
                "status"
            ]))
        );
    }

    #[test]
    fn descriptions_fit_in_600_characters() {
        for tool in tools() {
            let description = tool.description.unwrap_or_default();
            assert!(!description.is_empty(), "{}", tool.name);
            assert!(description.chars().count() <= 600, "{} has {} chars", tool.name, description.chars().count());
        }
    }

    #[test]
    fn annotations_match_the_spec() {
        let observe = tool("observe").annotations.unwrap();
        assert_eq!(observe.read_only_hint, Some(false), "recording writes a file");
        assert_eq!(observe.destructive_hint, Some(false));
        assert_eq!(observe.open_world_hint, Some(false));
        let act = tool("act").annotations.unwrap();
        assert_eq!(act.read_only_hint, None);
        assert_eq!(act.destructive_hint, None, "act must keep the destructive default");
        assert_eq!(act.open_world_hint, Some(false));
    }

    #[test]
    fn both_tools_are_always_loaded() {
        for tool in tools() {
            let flag = tool.meta.as_ref().and_then(|meta| meta.get(ALWAYS_LOAD)).cloned();
            assert_eq!(flag, Some(Value::Bool(true)), "{}", tool.name);
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
        let instructions = result["instructions"].as_str().unwrap();
        assert!(instructions.contains("observe") && instructions.contains("pick step"), "{instructions}");
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
        assert_eq!(observe["annotations"]["readOnlyHint"], false);
        assert_eq!(observe["annotations"]["destructiveHint"], false);
        assert_eq!(observe["annotations"]["openWorldHint"], false);
        let act = listed.iter().find(|tool| tool["name"] == "act").unwrap();
        assert_eq!(act["_meta"][ALWAYS_LOAD], true);
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
        let status = json!({"name": "observe", "arguments": {"mode": "status"}});
        let response = client.request(4, "tools/call", status).await;
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
            let status = json!({"name": "observe", "arguments": {"mode": "status"}});
            let response = client.request(id, "tools/call", status).await;
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
            ("observe", json!({"mode": "windows"})),
            ("observe", json!({"mode": "clipboard"})),
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
        let status = json!({"name": "observe", "arguments": {"mode": "status"}});
        let response = client.request(20, "tools/call", status).await;
        assert_eq!(response["result"]["isError"], false);
        let text = text_of(&response);
        assert!(text.lines().last().unwrap().starts_with(&format!("config: {BAD_CONFIG}; fix it")), "{text}");
        assert_eq!(spawner.spawned(), 0);
    }
}
