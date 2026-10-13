import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import readline from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";

const DEMO = dirname(fileURLToPath(import.meta.url));
const LOCAL = process.env.LOCALAPPDATA;
const WINPILOT = process.env.WINPILOT_EXE ?? join(LOCAL, "Programs", "winpilot", "winpilot.exe");
const BROWSERS = [
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  join(LOCAL, "Google/Chrome/Application/chrome.exe"),
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
];
const TITLE = "Acme Cloud";
const APP = `window:"${TITLE}"`;
const at = (selector) => `${APP} >> ${selector}`;
const STAGE_COLOR = "#0b0d1a";
const WINDOW_RECT = { x: 64, y: 60, width: 1920, height: 1200 };
const MARGIN = 48;
const RAW = join(DEMO, "raw.mp4");
const TIMELINE = join(DEMO, "timeline.json");
const SPEC = join(DEMO, "spec.pdf");
const COUNTDOWN_SECONDS = 3;
const CHECK_ONLY = process.argv.includes("--check");
const KEEP = process.argv.includes("--keep");
const SCRATCH = join(tmpdir(), `winpilot-promo-${Date.now()}`);

const EXPECTED = [
  ["edit", "Project name"],
  ["edit", "Owner email"],
  ["combobox", "Region"],
  ["checkbox", "Enable backups"],
  ["radio", "Pro"],
  ["button", "Attach spec"],
  ["button", "Create project"],
];

const CONFIG = `[overlay]
capturable = true
theme = "aurora"
label = "Claude"

[cursor]
glide_ms = 380

[record]
fps = 30
quality = "high"
`;

const BROWSER_FLAGS = [
  "--no-first-run",
  "--no-default-browser-check",
  "--disable-extensions",
  "--disable-sync",
  "--disable-default-apps",
  "--disable-background-networking",
  "--disable-component-update",
  "--hide-crash-restore-bubble",
  "--force-color-profile=srgb",
  "--disable-features=Translate,MediaRouter,OptimizationHints,AutofillServerCommunication",
];

const pause = (ms) => new Promise((done) => setTimeout(done, ms));
const say = (line) => console.log(line);

class WinPilot {
  constructor(env) {
    this.pending = new Map();
    this.nextId = 0;
    this.errors = [];
    this.process = spawn(WINPILOT, ["mcp"], { stdio: ["pipe", "pipe", "pipe"], env });
    this.process.stdin.on("error", () => {});
    readline.createInterface({ input: this.process.stdout }).on("line", (line) => this.receive(line));
    this.process.stderr.on("data", (chunk) => this.errors.push(String(chunk)));
    this.exited = new Promise((done) => this.process.on("exit", done));
    this.exited.then(() => this.failPending());
  }

  receive(line) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    const waiter = this.pending.get(message.id);
    if (!waiter) return;
    this.pending.delete(message.id);
    waiter.resolve(message);
  }

  failPending() {
    for (const waiter of this.pending.values()) waiter.reject(new Error(`winpilot exited\n${this.errors.join("")}`));
    this.pending.clear();
  }

  send(message) {
    this.process.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", ...message })}\n`);
  }

  request(method, params) {
    const id = ++this.nextId;
    const reply = new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
    this.send({ id, method, params });
    return reply;
  }

  async start() {
    const reply = await this.request("initialize", {
      protocolVersion: "2025-06-18",
      capabilities: {},
      clientInfo: { name: "claude-code", version: "promo" },
    });
    if (reply.error) throw new Error(`initialize failed: ${reply.error.message}`);
    this.send({ method: "notifications/initialized" });
    const tools = await this.request("tools/list", {});
    const names = (tools.result?.tools ?? []).map((tool) => tool.name);
    if (!names.includes("observe") || !names.includes("act")) {
      throw new Error(`this winpilot.exe exposes ${names.join(", ")}; the promo needs the v2 build with observe and act`);
    }
  }

  async call(name, args) {
    const started = Date.now();
    const reply = await this.request("tools/call", { name, arguments: args });
    const ended = Date.now();
    if (reply.error) throw new Error(`${name} failed: ${reply.error.message}`);
    const text = (reply.result?.content ?? []).filter((block) => block.type === "text").map((block) => block.text).join("\n");
    if (reply.result?.isError) throw new Error(`${name} ${JSON.stringify(args)} returned an error:\n${text}`);
    return { text, started, ended };
  }

  act(steps) {
    return this.call("act", { steps, return: "none" });
  }

  async close() {
    this.process.stdin.end();
    const timeout = pause(6000).then(() => "timeout");
    if ((await Promise.race([this.exited, timeout])) === "timeout") this.process.kill();
  }
}

function findBrowser() {
  const found = BROWSERS.find((path) => existsSync(path));
  if (!found) throw new Error("Chrome or Edge was not found");
  return found;
}

function writeScratch() {
  mkdirSync(join(SCRATCH, "appdata", "winpilot"), { recursive: true });
  mkdirSync(join(SCRATCH, "localappdata"), { recursive: true });
  writeFileSync(join(SCRATCH, "appdata", "winpilot", "config.toml"), CONFIG);
  const preferences = { selectfile_last_directory: DEMO, browser: { has_seen_welcome_page: true }, profile: { exit_type: "Normal" } };
  mkdirSync(join(SCRATCH, "chrome-app", "Default"), { recursive: true });
  writeFileSync(join(SCRATCH, "chrome-app", "Default", "Preferences"), JSON.stringify(preferences));
  mkdirSync(join(SCRATCH, "chrome-stage"), { recursive: true });
}

function launchBrowser(browser, profile, extra) {
  const child = spawn(browser, [`--user-data-dir=${join(SCRATCH, profile)}`, ...BROWSER_FLAGS, ...extra], { stdio: "ignore" });
  child.on("error", (error) => say(`browser failed to start: ${error.message}`));
  return child;
}

function launchStage(browser) {
  const page = `<title>Stage</title><body style="margin:0;background:${STAGE_COLOR};cursor:default">`;
  return launchBrowser(browser, "chrome-stage", ["--kiosk", `data:text/html,${encodeURIComponent(page)}`]);
}

function launchApp(browser) {
  const dip = (value) => Math.round(value / 1.25);
  return launchBrowser(browser, "chrome-app", [
    `--app=${pathToFileURL(join(DEMO, "demo.html")).href}`,
    `--window-size=${dip(WINDOW_RECT.width)},${dip(WINDOW_RECT.height)}`,
    `--window-position=${dip(WINDOW_RECT.x)},${dip(WINDOW_RECT.y)}`,
  ]);
}

function rectIn(text) {
  const found = text.match(/(\d+)x(\d+)@(-?\d+),(-?\d+)/);
  if (!found) throw new Error(`no rectangle in: ${text.split("\n")[0]}`);
  const [width, height, x, y] = found.slice(1).map(Number);
  return { x, y, width, height };
}

async function waitForApp(winpilot) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const { text } = await winpilot.call("observe", { mode: "windows" });
    if (text.includes(`"${TITLE}`)) return;
    await pause(400);
  }
  throw new Error(`the "${TITLE}" window did not open`);
}

async function primaryMonitor(winpilot) {
  const { text } = await winpilot.call("observe", { mode: "monitors" });
  const line = text.split("\n").find((entry) => entry.includes("primary")) ?? text.split("\n")[0];
  return rectIn(line);
}

function regionAround(rect, monitor) {
  const left = Math.max(monitor.x, rect.x - MARGIN);
  const top = Math.max(monitor.y, rect.y - MARGIN);
  const right = Math.min(monitor.x + monitor.width, rect.x + rect.width + MARGIN);
  const bottom = Math.min(monitor.y + monitor.height, rect.y + rect.height + MARGIN);
  const width = Math.floor((right - left) / 2) * 2;
  const height = Math.floor((bottom - top) / 2) * 2;
  return { x: left, y: top, width, height };
}

function missingTargets(text) {
  return EXPECTED.filter(([role, name]) => !new RegExp(`\\b${role} "${name}`).test(text));
}

async function readUntilReady(winpilot) {
  let read;
  for (let attempt = 1; attempt <= 10; attempt++) {
    read = await winpilot.call("observe", { window: APP });
    if (missingTargets(read.text).length === 0) {
      const attach = read.text.split("\n").find((line) => line.includes('"Attach spec'));
      say(`  all ${EXPECTED.length} targets found on read ${attempt} (Attach spec reads as: ${attach?.trim().split(" ")[1] ?? "?"})`);
      return read;
    }
    await pause(800);
  }
  const lines = read.text.split("\n").filter((line) => /^\s*e\d+ /.test(line) || line.startsWith("window "));
  say(lines.slice(0, 60).join("\n"));
  throw new Error(`the page is missing ${missingTargets(read.text).map(([role, name]) => `${role} "${name}"`).join(", ")}`);
}

async function prepare(winpilot) {
  await waitForApp(winpilot);
  await winpilot.act([{ do: "focus", target: 'window:"Stage"' }]).catch(() => say("  (could not raise the stage window)"));
  await winpilot.act([{ do: "focus", target: APP }]);
  const monitor = await primaryMonitor(winpilot);
  const wanted = WINDOW_RECT.x + WINDOW_RECT.width + MARGIN <= monitor.x + monitor.width ? WINDOW_RECT : { ...WINDOW_RECT, x: monitor.x + Math.round((monitor.width - WINDOW_RECT.width) / 2) };
  await winpilot.act([{ do: "window", action: "move", target: APP, rect: wanted }, { do: "focus", target: APP }]);
  await pause(600);
  const read = await readUntilReady(winpilot);
  const rect = rectIn(read.text);
  const region = regionAround(rect, monitor);
  say(`  window ${rect.width}x${rect.height}@${rect.x},${rect.y} · recording region ${region.width}x${region.height}@${region.x},${region.y}`);
  const park = { x: Math.min(monitor.x + monitor.width - 40, region.x + region.width + 140), y: region.y + Math.round(region.height * 0.62) };
  await winpilot.act([{ do: "mouse", action: "move", target: park, glide_ms: 0 }]);
  await winpilot.act([{ do: "focus", target: APP }]);
  await requireAppInFront(winpilot);
  return { rect, region, monitor };
}

async function requireAppInFront(winpilot) {
  const { text } = await winpilot.call("observe", { mode: "windows" });
  const line = text.split("\n").find((entry) => entry.includes(`"${TITLE}`)) ?? "";
  if (!/\bforeground\b/.test(line)) throw new Error(`the "${TITLE}" window is not in the foreground; not recording`);
  say(`  "${TITLE}" is in the foreground`);
}

function event(call, zero) {
  return { start: (call.started - zero) / 1000, end: (call.ended - zero) / 1000 };
}

function typingSteps() {
  const name = at('role:edit name:"Project name"');
  return [
    { do: "click", target: name },
    { do: "type", target: name, text: "Apollo" },
    { do: "wait", ms: 350 },
    { do: "key", keys: "tab", target: name },
    { do: "wait", ms: 300 },
    { do: "type", target: at('role:edit name:"Owner email"'), text: "ada@example.com" },
    { do: "wait", ms: 500 },
  ];
}

function optionSteps() {
  const region = at('role:combobox name:"Region"');
  if (process.argv.includes("--region-keys")) return keyedOptionSteps(region);
  return [{ do: "set", target: region, option: "Frankfurt" }, { do: "wait", ms: 450 }];
}

function keyedOptionSteps(region) {
  return [
    { do: "click", target: region },
    { do: "wait", ms: 350 },
    { do: "key", keys: "f" },
    { do: "wait", ms: 250 },
    { do: "key", keys: "enter" },
    { do: "wait", ms: 450 },
  ];
}

async function chooseRegion(winpilot) {
  try {
    return { call: await winpilot.act(optionSteps()), method: process.argv.includes("--region-keys") ? "keys" : "set option" };
  } catch (error) {
    say(`  set option failed, falling back to click + keys: ${error.message.split("\n").slice(-1)[0]}`);
    return { call: await winpilot.act(keyedOptionSteps(at('role:combobox name:"Region"'))), method: "keys (fallback)" };
  }
}

function choiceSteps() {
  return [
    { do: "set", target: at('role:checkbox name:"Enable backups"'), checked: true },
    { do: "wait", ms: 400 },
    { do: "click", target: at('role:radio name:"Pro"') },
    { do: "wait", ms: 500 },
  ];
}

async function fillForm(winpilot) {
  const typed = await winpilot.act(typingSteps());
  const region = await chooseRegion(winpilot);
  const chosen = await winpilot.act(choiceSteps());
  return { started: typed.started, ended: chosen.ended, text: [typed.text, region.call.text, chosen.text].join("\n"), region: region.method };
}

function createSteps() {
  return [
    { do: "click", target: at('role:button name:"Create project"') },
    { do: "wait", until: "text", text: "Project created", window: TITLE, timeout_ms: 8000 },
    { do: "wait", ms: 900 },
  ];
}

async function countdown() {
  say("Hands off the mouse and keyboard.");
  for (let left = COUNTDOWN_SECONDS; left > 0; left--) {
    say(`  recording in ${left}…`);
    await pause(1000);
  }
}

function clearRawSlot() {
  if (!existsSync(RAW)) return;
  mkdirSync(join(DEMO, "work"), { recursive: true });
  renameSync(RAW, join(DEMO, "work", `raw-previous-${Date.now()}.mp4`));
}

async function perform(winpilot, stage) {
  clearRawSlot();
  const recording = await winpilot.call("observe", {
    mode: "record",
    action: "start",
    region: stage.region,
    fps: 30,
    video_quality: "max",
    max_width: Math.min(3840, stage.region.width),
    max_seconds: 90,
    path: RAW,
  });
  const zero = recording.ended - 30;
  say(`  ${recording.text.split("\n")[0]}`);
  const events = {};
  const replies = {};
  try {
    await pause(700);
    const read = await winpilot.call("observe", { window: APP });
    events.read = { ...event(read, zero), ms: read.ended - read.started, elements: read.text.split("\n").filter((line) => /^\s*e\d+ /.test(line)).length };
    await pause(1500);
    const fill = await fillForm(winpilot);
    events.fill = event(fill, zero);
    replies.fill = fill.text;
    replies.region = fill.region;
    const pick = await winpilot.act([{ do: "pick", paths: [SPEC], open_with: at('role:button name~:"Attach spec"') }]);
    events.pick = event(pick, zero);
    replies.pick = pick.text;
    const create = await winpilot.act(createSteps());
    events.create = event(create, zero);
    replies.create = create.text;
    const shot = await winpilot.call("observe", { mode: "screenshot", target: APP });
    events.shot = event(shot, zero);
    replies.shot = shot.text.split("\n")[0].replace(/ · w\d+ .*$/, "");
    await pause(1700);
  } finally {
    const stopped = await winpilot.call("observe", { mode: "record", action: "stop" });
    replies.stop = stopped.text.split("\n")[0];
    say(`  ${replies.stop}`);
  }
  return { events, replies };
}

function writeTimeline(stage, performed) {
  const timeline = { version: 1, recordedAt: new Date().toISOString(), region: stage.region, window: stage.rect, events: performed.events, replies: performed.replies };
  writeFileSync(TIMELINE, `${JSON.stringify(timeline, null, 2)}\n`);
  say(`timeline written to ${TIMELINE}`);
  for (const [name, value] of Object.entries(performed.events)) say(`  ${name.padEnd(6)} ${value.start.toFixed(2)}–${value.end.toFixed(2)} s`);
}

function killTree(child) {
  if (!child?.pid) return;
  spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
}

async function cleanUp(winpilot, browsers) {
  if (winpilot) await winpilot.close().catch(() => {});
  if (KEEP) {
    say(`--keep: windows and ${SCRATCH} were left in place`);
    return;
  }
  browsers.forEach(killTree);
  await pause(1500);
  rmSync(SCRATCH, { recursive: true, force: true, maxRetries: 10, retryDelay: 500 });
  say(existsSync(SCRATCH) ? `could not delete ${SCRATCH}; remove it by hand` : "scratch profiles and config deleted");
}

async function main() {
  if (!existsSync(WINPILOT)) throw new Error(`winpilot.exe not found at ${WINPILOT}`);
  if (!existsSync(SPEC)) throw new Error(`missing ${SPEC}`);
  const browser = findBrowser();
  writeScratch();
  const browsers = [];
  let winpilot;
  const interrupted = () => {
    say("interrupted; cleaning up");
    winpilot?.process.kill();
  };
  process.once("SIGINT", interrupted);
  try {
    say(`scratch ${SCRATCH}`);
    browsers.push(launchStage(browser));
    await pause(2500);
    browsers.push(launchApp(browser));
    winpilot = new WinPilot({ ...process.env, APPDATA: join(SCRATCH, "appdata"), LOCALAPPDATA: join(SCRATCH, "localappdata") });
    await winpilot.start();
    say("preparing the stage");
    const stage = await prepare(winpilot);
    if (CHECK_ONLY) {
      const fill = await fillForm(winpilot);
      say(`  region chosen by ${fill.region}\n${fill.text}`);
      say("--check: everything is in place; nothing was recorded");
      return;
    }
    await countdown();
    const performed = await perform(winpilot, stage);
    writeTimeline(stage, performed);
    say("next: node demo/build.mjs");
  } finally {
    process.removeListener("SIGINT", interrupted);
    await cleanUp(winpilot, browsers);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
