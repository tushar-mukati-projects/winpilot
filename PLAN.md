# Deckhand: fast Windows desktop control for Claude

**Status:** plan, 2026-10-11. Nothing is built yet except the benchmark seed in `bench/deskbench`.

Deckhand is a single Rust binary that Claude Code runs as an MCP server over stdio. It sees the screen as a compact element list (screenshots too, when asked), acts on elements by reference, waits on real UI events instead of fixed sleeps, and returns what changed after every action. An optional "Desktop" panel in Claude Deck shows a live feed of what the agent is doing, with pause and stop.

---

## 1. Why Windows-MCP felt slow: measured on this laptop

The figures come from the 139 Windows-MCP calls in your last two sessions, joined with Claude Code's MCP server log. The six tasks took 1,330 s of wall-clock in total:

| Where the time went | Seconds | Share |
|---|---|---|
| Claude thinking between calls | 601 | 45% |
| Windows-MCP executing | 481 | 36% |
| Blocked by its "user is touching the mouse" guard | 174 | 13% |
| Other tools | 74 | 6% |

What inside Windows-MCP caused it:

1. **Snapshot walked the entire accessibility tree every time.** It took a median of 15.2 s, and up to 56 s, on heavy windows. It made one cross-process call per element, asked for about 40 properties per element, descended into off-screen subtrees, and walked the taskbar and desktop windows too. App switching and WaitFor repeated the full walk on every call or poll.
2. **Fixed sleeps on every input.** Each Click slept 0.55 s and each Scroll 0.55 s per wheel tick. Type slept about 90 ms per character whenever the text contained a newline or brace: one 1,767-character Type took 164 s. In total, about 45% of the server's execution time was spent sleeping.
3. **The takeover guard.** Any key press or mouse move by you, including typing in the terminal, made it reject *every* tool for 10 s, even read-only ones. 12% of all calls failed this way.
4. **One action per model turn.** The pattern was always look → act → look. Results were large: 6 MB of images and 169k characters in total. Context at those turns had a median of about 875k tokens, which makes every model turn slower.
5. **Startup.** The server took 5–13 s to start, and its startup handshake timed out 5 times.

## 2. What a Rust core does on this laptop: measured

`bench/deskbench` ran read-only on this machine (2560×1600 screen, `windows` 0.62.2):

| Operation | Windows-MCP | Rust (measured) |
|---|---|---|
| Window list | part of the 1–15 s snapshot | **0.2 ms** |
| Element tree, small window (20 elements) | ~0.8 s on a quiet desktop | **15 ms** |
| Element tree, Claude Deck (102 elements) | 0.22–0.42 s | **35 ms** |
| Element tree, Chrome medium page (423) | — | **40 ms** |
| Element tree, Chrome heavy page (2,119) | 9–56 s in your sessions | **140 ms** |
| Element under a point (hit test) | — | **2 ms** |
| Screenshot (warm DXGI capture, resize to 1568 px, JPEG q75) | 0.13 s plus a 3.5 s flash | **21–23 ms**, 45–115 KB |
| OCR, Windows.Media.Ocr on the downscaled frame | — | **25–130 ms** |
| Process start → first UIA result | 5–13 s | **~40 ms** |

The key technique is a single `BuildUpdatedCache` call with a subtree cache request. It returns the whole tree with its hierarchy in one round trip: 3–4× faster than `FindAllBuildCache` and about 15× faster than walking the tree. Filtering to interactive elements should happen in Rust after the fetch. Asking UIA to filter saves almost no time, because the app walks its whole tree anyway.

## 3. Realistic expectation

On the same six tasks, the estimate is:

| Part | Today | With Deckhand |
|---|---|---|
| Tool execution | 481 s | 10–20 s |
| Guard blocks | 174 s | 0 s |
| Claude thinking | 601 s | 250–350 s, from fewer calls and smaller results |
| **Total** | **1,330 s** | **about 300–400 s, i.e. 3–4× faster end to end** |

Claude's thinking becomes the dominant cost, so the tool API is designed above all to cut the **number of turns** and the **tokens per turn**.

## 4. Decision: build it, borrowing ideas, not forking

| Option | Verdict |
|---|---|
| Fix Windows-MCP | No. Its slowness is structural: Python on a single-threaded apartment (STA), sequential walks of every window, sleeps baked into the vendored UIA library, and acting by coordinates only. Fixing it means a rewrite. |
| Fork **Cua Driver** (trycua/cua, Rust, MIT core) | The closest design: background clicks via UIA, refusals instead of fake success, element tokens, batching, CDP. But it has 65 tools (a heavy schema on every turn), it's built around macOS with high code churn, publishes no Windows latency numbers, and doesn't handle DPI. **Borrow ideas; optionally measure it for an hour in Phase 0.** |
| Fork **Terminator** (mediar-ai, Rust) | It has the best selector language and diff/verify ideas, but no release since April 2026 and an open shell-injection bug. **Borrow the selector syntax and the verify-after-action idea.** |
| **Build Deckhand** | The core is about 1,500 lines away: the benchmark already compiles every hard part (UIA, DXGI/WGC capture, OCR, encoding). It also matches Claude Deck's Rust backend, so code can move between them (Job Objects, the MCP client for testing). |

**Why Rust and not TS/Python:**
- The costs are COM calls, image work and input hooks. Rust gets direct `windows-rs` access, with no Python global interpreter lock (GIL) and no Node native-addon layer.
- It starts in ~40 ms and ships as one ~5 MB exe.

**Why stdio and not HTTP:** no network listener at all. Windows-MCP's CVE was in its HTTP transport.

## 5. Architecture

```
Claude Code ──stdio (MCP)──▶ deckhand.exe mcp            tokio + rmcp; thin, never blocks
                               │  per-call deadlines, request routing, output formatting
                               │  length-prefixed messages over the child's stdin/stdout
                               ▼
                             deckhand.exe worker          child process in a Job Object; killed and respawned if it hangs
                               ├─ UIA thread (MTA)        CUIAutomation8, timeouts, event handlers, element cache + refs
                               ├─ capture thread          warm DXGI duplication → WGC → GDI fallback; resize + JPEG
                               ├─ input thread            SendInput batches, Raw Input user-activity monitor
                               ├─ hotkey thread           RegisterHotKey kill switch (message loop)
                               └─ overlay thread          click-through border + click ripple, hidden from captures
                               │
                               └──▶ %LOCALAPPDATA%\deckhand\logs\YYYY-MM-DD.jsonl   audit lines any UI can tail; the named event Local\deckhand-stopped is the shared kill switch
                                         ▼
                                   Claude Deck "Desktop" panel: live feed, pause/stop, deny-list editor
```

- **One binary, two roles.** `deckhand mcp` spawns `deckhand worker`. If the worker misses a deadline because a hung app froze a COM call (Electron issue #383 in Windows-MCP), the server kills it and respawns it in ~50 ms. It returns `target_not_responding (process X)` and skips that process for a minute.
- **One actor at a time.** Every Claude session spawns its own server, so a named mutex (`Local\deckhand-act`) stops two sessions driving the mouse at once. Read-only calls never take the lock.
- **Physical pixels everywhere.** The worker is Per-Monitor-V2 DPI aware. Screenshots carry `scale` and `origin` so image coordinates map back to the screen exactly.
- **The UIA instance is created by hand** as `CUIAutomation8`, then cast to `IUIAutomation6`, with these settings:
  - ConnectionTimeout 2 s and TransactionTimeout 3 s;
  - CoalesceEvents and ConnectionRecoveryBehavior on;
  - **AutoSetFocus off**, so pattern actions don't steal focus.

  The `uiautomation` crate's constructor gives the old `CUIAutomation`, which has none of these.

## 6. The MCP tools (10, short descriptions)

The core four tools set `_meta["anthropic/alwaysLoad"]`, so Claude Code doesn't defer them behind ToolSearch (that would cost an extra round trip per session). Read-only tools set `readOnlyHint` so they can be auto-allowed.

| Tool | Purpose | Default output |
|---|---|---|
| `observe` | Read a window as text: foreground window by default, or `window`, `scope:"desktop"`, `query`, `max` | Header (foreground window, other top windows one line each, focused element), then one line per visible interactive/text element with a ref |
| `act` | Run 1–N steps in one call, then report what changed | Per-step ✓/✗ with the method used, plus a diff: new windows/dialogs, focus move, elements added/removed/changed. If a dialog opened, its full element list. |
| `find` | Server-side search by selector or text, foreground or all windows | Matching refs with role, name and path; never the whole tree |
| `screenshot` | Image of the foreground window, a window, a region or a monitor; optional ref labels (`marks`) | JPEG ≤1280 px wide (2560×1600 → 1280×800 ≈ 1,330 tokens) plus `img=i7 scale=2.0 origin=0,0` |
| `wait_for` | Wait until an element appears or disappears, a window opens, text shows up, or the UI goes idle | Event-driven, with a 250 ms poll fallback; returns state when met; 5 s default, 60 s max |
| `windows` | list / focus / launch / close / minimize / maximize / restore / move | `launch` resolves Start-menu names (no PowerShell), waits for the window and returns its snapshot |
| `pick_file` | Fill any native Open/Save dialog: `paths:[…]`, optional `open_with` (a target to click first, e.g. an Upload button), `overwrite:false` | One call does the whole upload: click Upload, wait for the dialog, set the path, press Open, verify it closed |
| `read_text` | Full text of an element, window or region: TextPattern / Value first, OCR as fallback | Plain text, optional line boxes |
| `clipboard` | Get or set text | — |
| `status` | Health: paused/stopped, user active, lock holder, worker restarts, recent timings and errors | — |

### Element lines (token-cheap)

```
fg w3 "Untitled - Notepad" notepad.exe · also: w1 chrome.exe "Inbox", w2 claude-deck.exe
focus e2
e1 menuitem "File"
e2 edit "Text editor" value="hello wor…" focused
e5 button "Close"
```

- Coordinates are left out by default, because Claude acts by ref. `coords:true` adds `@x,y,w,h`.
- A typical window costs 1–3k tokens, and a diff after an action ≤300.

### Targets: three forms, accepted by every step

- **A ref:** `"e12"`. Refs come from UIA RuntimeIds, stay stable across snapshots while the element lives, and carry a fingerprint (window, role, name, AutomationId, index path). Stale refs are re-found by fingerprint. If the match is ambiguous, Deckhand returns the candidates instead of guessing.
- **A selector**, a Terminator-style subset: `role:button name:"Save"`, `name~:sav` (contains), `id:1148`, `class:Edit`, `text:…`, `window:"Notepad"`, `process:chrome`, `nth:2`, `>>` to chain. This lets Claude act **without a prior observe** when it already knows the app.
- **A point:** `{"x":640,"y":360,"img":"i7"}` in screenshot coordinates. It's mapped back with that image's scale and origin.

### `act` steps

Each step is an object with a `do` field: `click` (`button`, `count` 1–3, `modifiers`, `how: auto|invoke|mouse`), `hover`, `type` (`text`, `enter`, `how: auto|keys|paste`), `set_value` (`value`), `key` (`keys`, `repeat`), `scroll` (`direction`, `amount`), `drag` (`from`, `to`), `select` (`option`), `toggle` (`on`), `expand`, `collapse`, `focus`, `wait_for` (`until` plus its fields) and `wait` (`ms`, capped at 5 s). Unknown fields are rejected. The result always reports which method actually ran.

```json
{"steps":[
  {"do":"click","target":"window:\"Notepad\" >> role:menuitem name:File"},
  {"do":"click","target":"name:\"Save as\""},
  {"do":"wait_for","until":"window","window":"Save As"},
  {"do":"set_value","target":"id:1001","value":"C:\\temp\\a.txt"},
  {"do":"key","keys":"enter"}
], "return":"diff"}
```

## 7. How each speed goal is met

| Goal | Technique |
|---|---|
| Snapshot p50 ≤60 ms, p95 ≤150 ms | One `BuildUpdatedCache(TreeScope_Subtree)` from the foreground root. Minimal cached properties: Name, ControlType, AutomationId, BoundingRectangle, IsEnabled, IsOffscreen, IsKeyboardFocusable, ClassName, Value, plus pattern availability. Filter in Rust. Cap at 3,000 nodes and a time budget; skip off-screen subtrees. |
| No repeated walks | Keep the tree per window. FocusChanged, StructureChanged and WindowOpened events (with cache requests attached) invalidate only what changed. |
| Click ≤15 ms plus app reaction | **Auto mode:** if the target window is in the foreground, a real SendInput click at the element's centre, after a 2 ms hit test confirms the element is really there. If the window is in the background and supports Invoke/Toggle/Select, use the pattern, which doesn't move the mouse. Otherwise focus the window, then click. **No sleeps.** |
| Type 2,000 chars ≤100 ms | Plain edit fields: `ValuePattern.SetValue`, then verify the value. Otherwise one `KEYEVENTF_UNICODE` SendInput batch: newlines become Enter, emoji become surrogate pairs. Very long text: paste, saving and restoring the clipboard. |
| Scroll ≤20 ms | One wheel event with the full delta, or `ScrollPattern` / `ScrollItemPattern.ScrollIntoView`. |
| No fixed waits after actions | **Settle:** wait until the target window has had no structure/property events for 120 ms, or 1 s max. Inside a batch, settle only before a step that needs a fresh target. |
| Screenshot ≤30 ms | Keep the DXGI duplication open for the server's lifetime; skip its black first frame; reopen on `ACCESS_LOST` (UAC, resolution change). WGC session as fallback, GDI as last resort. `fast_image_resize` plus `jpeg-encoder` (SIMD, takes BGRA directly). |
| App switch ≤100 ms | `EnumWindows` (0.2 ms) plus `SetForegroundWindow`. If Windows refuses, tap Alt with SendInput and retry. Confirm with `GetForegroundWindow`. |
| Launch | Name→AUMID index from `shell:AppsFolder` and Start-menu `.lnk` files, cached. `IApplicationActivationManager` for packaged apps, `ShellExecuteExW` for Win32. Wait on WindowOpened. |
| Fewer turns | Act by selector without observing; batch steps; diff returned with every action; `pick_file` and `launch` do whole flows; waits run on the server. |
| Fewer tokens | Text first; screenshots only when asked; ≤1280 px JPEG; diffs instead of trees; 10 tools with short descriptions. A `desktop` subagent keeps screen data out of the main session's context. |
| Reads never blocked | The user-activity guard applies to input steps only. Reads can run concurrently: capture and OCR have their own threads. |

## 8. Reliability rules

1. **Never report success without checking.** After each step, Deckhand verifies: the foreground window is the one targeted, the value was set, the dialog closed, the element appeared. Otherwise the step returns ✗ with the reason.
2. **Chromium/Electron first touch can be sparse,** because the web-content accessibility tree switches on lazily. If a Chrome-class window returns very few elements, retry once after 150 ms.
3. **Some web apps ignore SetValue,** for example React-controlled inputs. When the value doesn't stick, fall back to focus + typing automatically.
4. **Hung apps:** UIA timeouts, then the worker deadline, then kill and respawn, then a temporary skip list. Hangs never freeze the MCP server.
5. **Elevated targets:** compare the target's token integrity level with ours and return `elevated_target` instead of failing silently.
6. **UAC and secure desktop:** detect `consent.exe` or the input desktop not being reachable, and return `needs_user: UAC prompt is up`.
7. **Capture loss:** reopen DXGI on `ACCESS_LOST`; fall back to WGC on `DXGI_ERROR_UNSUPPORTED` (hybrid GPU / MUX mode).
8. **Dialog variants:** the Common Item Dialog uses File name 1148, Open 1 and Cancel 2; Save reportedly uses 1001 (verify on build 26300). Legacy dialogs use edt1 1152. Match control type and class first and use the IDs to confirm, so localisation doesn't break it.

## 9. Safety

- **Kill switch:** a global hotkey (default `Ctrl+Alt+Shift+Backspace`, the same chord Windows-MCP used, since many laptop keyboards have no Pause key; configurable). It stops all input at once and cancels in-flight steps. Until you press it again or resume from the panel, `act` returns `stopped_by_user`. The model can't re-arm it.
- **User yield, not refusal:** if you touched the mouse or keyboard in the last 600 ms, an input step waits for up to 5 s of quiet, then proceeds or returns `user_active`. Your own input is told apart from Deckhand's by `LLMHF_INJECTED` plus a marker in `dwExtraInfo` (Raw Input monitor on its own thread).
- **Deny lists in `%APPDATA%\deckhand\config.toml`:**
  - processes and window-title patterns never to read or touch (defaults include password managers and any title containing `Downloads\recovery`);
  - `pick_file` refuses paths under the folders in `[guard] deny_paths`.
- **Visible agent:** a thin border around the window being driven and a ripple at each click point. It's excluded from screenshots with `WDA_EXCLUDEFROMCAPTURE`, and it's static with no animation loop.
- **Audit log:** rolling JSONL in `%LOCALAPPDATA%\deckhand\logs` with tool, target (role and short name), process, method, result and duration. Typed text is logged by length only unless `log_text = true`. No screenshots are saved.
- **Claude Code permissions:** auto-allow `observe`, `find`, `screenshot`, `read_text` and `status`; ask for `act`, `pick_file`, `windows` and `clipboard` until you trust it. Don't combine desktop control with bypass-permissions mode.

## 10. Claude Code integration

- **Install** the built exe to `%LOCALAPPDATA%\Programs\deckhand\deckhand.exe` rather than registering `target\release` directly. A running MCP server locks its exe, which would break `cargo build`. An `install` script copies the exe and checks its SHA-256 (this laptop has a known bit-flip fault on large writes).
- **Register:** `claude mcp add -s user deckhand -- "%LOCALAPPDATA%\Programs\deckhand\deckhand.exe" mcp`.
- **`desktop` subagent** (`~/.claude/agents/desktop.md`): tools limited to `mcp__deckhand__*`, with instructions to act by selector, batch steps, avoid screenshots unless needed, and use `pick_file` for uploads. The main session hands it a goal and gets back a short result, so screen data never bloats the main context (your sessions ran at ~875k tokens).
- **Replace Windows-MCP** once Deckhand passes the Phase 4 gate: `claude mcp remove windows-mcp -s user`, then `uv tool uninstall windows-mcp`.

## 11. Repo layout and crates

```
dev/deckhand/
  Cargo.toml                 workspace; release profile like Claude Deck (lto, codegen-units=1, panic=abort, strip)
  crates/deckhand-core/src/
    uia/        automation.rs (CUIAutomation8 setup), snapshot.rs, refs.rs, selector.rs, events.rs, patterns.rs
    capture/    dxgi.rs, wgc.rs, gdi.rs, encode.rs
    input/      sendinput.rs, keys.rs, activity.rs (Raw Input monitor)
    act/        resolve.rs, click.rs, typing.rs, settle.rs, diff.rs
    apps/       windows.rs (enum/focus), launch.rs (AppsFolder/AUMID), integrity.rs
    dialogs.rs  ocr.rs  clipboard.rs  overlay.rs  guard.rs (kill switch, deny lists, lock)  audit.rs  ipc.rs
  crates/deckhand/src/
    main.rs     `deckhand mcp` | `deckhand worker` | `deckhand bench`
    mcp.rs      rmcp tool router, formatting, deadlines, worker supervision
  bench/deskbench/            the measured seed (already here)
  tests/fixtures/form.html    local page: inputs, a file input, delayed elements, a modal
  scripts/install.ps1         build, copy to %LOCALAPPDATA%\Programs\deckhand, verify hash
```

| Crate | Version | Use |
|---|---|---|
| `windows` / `windows-core` | 0.62.2 | UIA, SendInput, DXGI, WGC, OCR, shell (proven in the benchmark) |
| `rmcp` | 3.5.1 | MCP server: `server`, `macros`, `transport-io`, `schemars` |
| `tokio` | 1.x | Server runtime |
| `fast_image_resize` | 6.1.0 | SIMD resize (5–6 ms at 1568 px) |
| `jpeg-encoder` (`simd`) | 0.7.1 | JPEG from BGRA (8–10 ms) |
| `png` | 0.18 | Lossless crops for tiny text |
| `serde`, `serde_json`, `toml_edit` | — | IPC, config |
| `tracing`, `tracing-appender` | 0.2.5 | Audit log, never to stdout |

Skipped: `enigo` (own SendInput wrapper, ~300 lines), `xcap` and `windows-capture` (slower than the raw paths in the benchmark), OmniParser (≥0.6 s per frame on a GPU, AGPL detector), Windows AI TextRecognizer (needs an NPU and package identity).

## 12. Roadmap with gates

Each phase ends with a measured gate. Effort assumes building it together with Claude.

| Phase | Work | Gate |
|---|---|---|
| **0. Setup** (½ day) | Workspace; move the deskbench modules into `deckhand-core`; install script; optionally an hour measuring Cua Driver to steal techniques | `cargo test` green; `deckhand bench` reproduces the numbers in §2 |
| **1. See** (2 days) | Worker process plus IPC and deadlines; CUIAutomation8 setup; subtree snapshot; refs and fingerprints; formatter; window list; warm capture; `observe`, `find`, `screenshot`, `status` | observe p50 ≤60 ms, p95 ≤150 ms on Chrome and Deck; screenshot ≤30 ms warm; start ≤150 ms; Claude describes the screen in 1 call |
| **2. Act** (3 days) | SendInput library; pattern actions; target resolution and hit-test check; foregrounding; settle and diff; `act` batches; event-driven `wait_for`; actor lock; user yield; kill switch; audit log | Notepad task (open, type 2,000 chars with newlines, save) in ≤4 calls and ≤1 s of tool time; zero read failures while you use the mouse |
| **3. Apps, dialogs, text** (2 days) | `windows` launch/focus; `pick_file` (open, save, multi-file, `open_with`); `read_text` (TextPattern, OCR); `clipboard`; elevated/UAC detection; Chromium sparse retry; VS Code hang test | Upload through Chrome's native dialog in one `pick_file` call; a VS Code snapshot never hangs the server |
| **4. Feel and switch over** (2 days) | Overlay; audit-log feed; Claude Deck "Desktop" panel (live feed, pause/stop, timings, deny-list editor); `desktop` subagent and skill; remove Windows-MCP | Head-to-head on the golden tasks (§13): ≥3× faster wall-clock and ≥40% fewer calls than the Windows-MCP baseline |
| **5. Later** | Record/replay macros; CDP fast path for a dedicated automation Chrome profile; signed `uiAccess` build for elevated windows; UIA Remote Operations; OCR "set-of-marks" for canvas apps and games | — |

Total: about 10 focused days to Phase 4.

## 13. Testing

- **Unit tests** (no desktop): selector parser, diff engine, ref fingerprint matching, coordinate and DPI math, dialog-ID tables, formatter token budgets.
- **Read-only integration tests:** snapshots and captures of whatever is open, like deskbench. Safe while you're working.
- **Input integration tests:** these move the real mouse. They run only when you start them, with a 3-second on-screen countdown, and abort on any physical input (the user-yield code doing double duty). All files go to a scratch folder.
- **Golden tasks**, timed for both Windows-MCP and Deckhand (calls, wall-clock, tokens):
  1. Notepad: open, type 30 lines, save to the scratch folder.
  2. Explorer: open the scratch folder, make a subfolder, rename a file, move it in.
  3. Chrome: open `tests/fixtures/form.html`, fill the form, upload a file through the native dialog, read the confirmation.
  4. Calculator: compute 12345 × 6789 with buttons, read the result.
  5. Settings: read the device name and Windows version (read-only).
  6. Copy a paragraph from a Chrome page into Notepad.

## 14. Known limits

- Claude's own thinking time remains. Deckhand cuts turns and context but can't remove model latency.
- The UAC prompt, Ctrl+Alt+Del and the lock screen are on the secure desktop and unreachable. Elevated apps need the Phase 5 signed `uiAccess` build. Whether self-signing works for uiAccess is unverified.
- Canvas apps and games have no accessibility tree, so they're driven by screenshot and coordinates only. Anti-cheat games may block injected input.
- An unsigned build that injects input may draw a Defender or Smart App Control flag. That's fine for a personal build; signing is needed only for distribution.
- Virtual-desktop switching needs undocumented APIs that break across builds, so it's not planned.

## 15. Sources

- Measurements: `bench/deskbench/results-run*.txt`, plus 2026-10-11 transcript and MCP-log analysis of Windows-MCP 0.8.8.
- Windows-MCP: https://github.com/CursorTouch/Windows-MCP (issues #35, #87, #383, #416, #453)
- Cua Driver: https://github.com/trycua/cua and https://cua.ai/blog/inside-windows-computer-use
- Terminator: https://github.com/mediar-ai/terminator
- OSWorld-Human (LLM time is 75–94% of agent latency): https://arxiv.org/html/2506.16042v1
- UFO²: https://arxiv.org/html/2504.14603
- UIA caching: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients
- IUIAutomation6: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nn-uiautomationclient-iuiautomation6
- UIA security / uiAccess: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- Chrome UIA provider: https://developer.chrome.com/blog/windows-uia-support-update
- Chrome 136 remote-debugging change: https://developer.chrome.com/blog/remote-debugging-port
- Claude vision sizing: https://platform.claude.com/docs/en/build-with-claude/vision
- Claude Code MCP limits: https://code.claude.com/docs/en/mcp
- rmcp: https://github.com/modelcontextprotocol/rust-sdk
