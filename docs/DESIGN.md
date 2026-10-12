# Deckhand design

This document describes Deckhand as built: why it exists, how it is put together, and the rules it follows. The user-facing reference is the [README](../README.md); the rules for working in the code are in [CLAUDE.md](../CLAUDE.md).

## 1. The problem

Deckhand started as a replacement for [Windows-MCP](https://github.com/CursorTouch/Windows-MCP), a Python MCP server for Windows desktop control. Desktop tasks driven through it felt slow, so 139 of its calls from two real sessions on the author's laptop were joined with Claude Code's MCP server log. The six tasks in those sessions took 1,330 s of wall-clock time:

| Where the time went | Seconds | Share |
|---|---|---|
| The model thinking between calls | 601 | 45% |
| Windows-MCP executing | 481 | 36% |
| Blocked by its "user is touching the mouse" guard | 174 | 13% |
| Other tools | 74 | 6% |

The causes inside Windows-MCP:

1. **Every snapshot walked the whole accessibility tree.** A median of 15.2 s, and up to 56 s on heavy windows. It made one cross-process call per element, asked for about 40 properties each, descended into off-screen subtrees, and walked the taskbar and desktop as well. App switching and waits repeated the full walk on every call or poll.
2. **Fixed sleeps on every input.** Each click slept 0.55 s, each scroll 0.55 s per wheel tick, and typing slept about 90 ms per character whenever the text contained a newline or brace: one 1,767-character Type took 164 s. About 45% of the server's execution time was sleeping.
3. **A blanket takeover guard.** Any key press or mouse move by the user, including typing in the terminal, made it reject every tool for 10 s, read-only ones included. 12% of all calls failed this way.
4. **One action per model turn.** The pattern was always look, act, look. Results were large (6 MB of images and 169k characters in total) and context at those turns had a median of about 875k tokens, which slows every model turn.
5. **Startup.** The server took 5–13 s to start, and its handshake timed out five times.

A read-only Rust benchmark (`bench/deskbench`) then measured what direct Windows APIs cost on the same machine (2560×1600):

| Operation | Windows-MCP | Rust benchmark |
|---|---|---|
| Window list | part of the 1–15 s snapshot | 0.2 ms |
| Element tree, small window (20 elements) | ~0.8 s | 15 ms |
| Element tree, 102-element WebView2 app | 0.22–0.42 s | 35 ms |
| Element tree, Chrome, 423 elements | — | 40 ms |
| Element tree, Chrome, 2,119 elements | 9–56 s | 140 ms |
| Element under a point | — | 2 ms |
| Screenshot, resized to 1568 px, JPEG | 0.13 s plus a 3.5 s flash | 21–23 ms |
| OCR on the downscaled frame | — | 25–130 ms |
| Process start to first UIA result | 5–13 s | ~40 ms |

The key technique is one `BuildUpdatedCache` call with a subtree cache request: it returns the whole tree with its hierarchy in a single cross-process round trip, 3–4× faster than `FindAllBuildCache` and about 15× faster than walking the tree. Filtering to interactive elements happens in Rust afterwards; asking UIA to filter saves almost nothing, because the provider walks its whole tree anyway.

Model thinking time can't be removed, so the design goal was to make tool execution negligible and, above all, to cut the **number of turns** and the **tokens per turn**. The estimate at design time was 300–400 s for the same six tasks instead of 1,330 s.

## 2. Architecture

```
MCP client ──stdio (JSON-RPC)──▶ deckhand.exe mcp        tokio + rmcp; thin, never blocks on the desktop
                                   │  argument checks, deadlines, routing, output formatting, audit log
                                   │  length-prefixed JSON frames over the child's stdin/stdout
                                   ▼
                                 deckhand.exe worker      child process in a Job Object; killed and restarted if it hangs
                                   ├─ call threads        one per request, MTA
                                   ├─ UIA actor           CUIAutomation8, element registry and refs
                                   ├─ capture actor       DXGI duplication / GDI / PrintWindow, OCR
                                   ├─ change monitor      WinEvent hooks for waits and settling
                                   ├─ activity monitor    Raw Input: is the user using the mouse or keyboard?
                                   ├─ hotkey thread       the kill switch
                                   ├─ overlay thread      layered click-through windows
                                   └─ recorder threads    WGC capture and encoding, while a recording runs
```

### One binary, two roles

`deckhand mcp` (also the default with no arguments) is the MCP server. It spawns `deckhand worker` from its own executable. All Windows work happens in the worker; the server only validates, routes, formats and logs. A hung app can freeze a COM call indefinitely (Windows-MCP issue #383 is an Electron example), and with this split that freeze can never block the MCP transport.

The transport is stdio only. There is no network listener at all, which removes the class of bug behind Windows-MCP's CVE in its HTTP transport.

### Worker process

- **Spawn.** The worker is created suspended with `CREATE_NO_WINDOW`, inherits only its two pipes and stderr (`PROC_THREAD_ATTRIBUTE_HANDLE_LIST`), is assigned to a Job Object, then resumed. Its stderr is the server's stderr, so worker diagnostics land in the client's MCP log.
- **Job Object.** `KILL_ON_JOB_CLOSE` ends the worker if the server dies, `DIE_ON_UNHANDLED_EXCEPTION` stops a crashed worker from sitting in an error dialog, and `SILENT_BREAKAWAY_OK` lets apps the worker launches outlive it.
- **IPC.** Each message is a little-endian `u32` length followed by JSON, at most 64 MB. The server sends `init` (config, restart epoch, client name), then `call` messages carrying the time left on their deadline, `cancel` messages, and `agent` when the client re-initializes under another name (the overlay label follows); the worker answers `ready` with its version and build id, then `reply` messages keyed by request id. A reply too large for a frame is answered with an error instead. Requests run concurrently on their own threads, so a long `act` batch doesn't hold up a read.
- **Build check.** `build.rs` stamps every build with an id made of the version, the build time and a hash of the workspace sources. Server and worker are the same executable, so they share the id unless the file was replaced after the server started; a worker that reports a different version or build id is refused with "a newer Deckhand was installed …; restart your MCP client to load it". This catches a server still running an old build after an install, even when the version number didn't change.
- **Ref epochs.** Each worker generation hands out element and image refs from its own numeric range, so a ref from before a restart is reported as stale instead of silently naming a different element.

### Deadlines, restarts and backoff

Every call carries a deadline sized to the request: a few seconds for a read, more for launches and long waits, and for `act` a sum of per-step budgets that grows with typed text, key presses and glides, all capped at a few minutes. The deadline starts when the server receives the call, and the server sends the time left with it, so time spent starting the worker counts. Inside the worker, the call works to that minus a second of slack, so it can stop between steps and answer with a `timeout` itself, and it skips the after-state report, with a note, when less than a window read's budget is left.

If the worker still misses the deadline, the server assumes an app froze a COM call: it terminates the job, fails the call with `target_not_responding` naming the cause, and starts a fresh worker for the next call. A worker that exits or sends an unreadable frame is handled the same way. A worker that fails to *start* puts the supervisor into backoff: the next attempt waits 0.5 s, doubling up to 10 s, and calls in between get the failure and the retry time. `observe mode=status` never fails because of the worker: if the worker can't answer, the server builds the report itself and appends the worker problem.

### Graceful shutdown

When the client closes stdin, the server marks itself shut down and closes the worker's input. The worker sees end-of-file, refuses any new recording, cancels the calls still running and waits up to a second for them to finish their own cleanup (restoring the clipboard, releasing held keys), then stops and finalizes any recording, dismisses its notification balloons, and exits. The server waits up to 3 s for that before terminating the job, and then gives the async runtime 2 s to finish.

### Configuration

The server reads `%APPDATA%\deckhand\config.toml` once at startup and sends it to the worker in `init`. Every section and key is optional, and unknown keys are rejected so typos don't silently do nothing. If the file can't be parsed, the server stays up and refuses every call except status, with the file, line and message, rather than running with half a config. User deny rules and deny paths are added to the built-ins, never replacing them.

## 3. Crates

| Crate | Responsibility |
|---|---|
| `deckhand-core` | Pure logic and the shared protocol: tool arguments and their validation, selectors, keys, roles, refs, geometry, diff, output formatting, config and themes, the agent-name mapping, OCR text matching, IPC framing. No Windows APIs, so it is tested exhaustively. |
| `deckhand-uia` | UI Automation on one dedicated thread: automation setup, cache requests, snapshots, the element registry with refs and fingerprints, re-finding stale refs, pattern actions, text. |
| `deckhand-capture` | Screen capture (DXGI duplication, GDI, PrintWindow), resize, JPEG and PNG encoding, ref marks, OCR, and the recorder (Windows.Graphics.Capture to Media Foundation H.264 or GIF). |
| `deckhand-input` | SendInput for mouse, keyboard and Unicode text, cursor glides, releasing stuck inputs, the Raw Input activity monitor, the kill-switch hotkey. |
| `deckhand-desktop` | Window enumeration, focus and placement, launching apps, the clipboard, monitors and DPI, virtual desktops, WinEvent change hooks, integrity levels and the secure desktop, named sync objects, the overlay, notifications. |
| `deckhand-worker` | The engine: every observe mode and act step, built from the platform crates, plus the guard (kill switch, yield, deny lists, actor lock). |
| `deckhand` | The binary: MCP server (rmcp), worker supervisor, audit log. |
| `deckhand-fixture` | A test-only Win32 app with known controls, delayed elements and file dialogs, plus a library (`FixtureProcess`) that tests use to launch it without stealing focus and to close it. |

Each crate's `lib.rs` only declares modules and re-exports; those signatures are the contract between crates.

### Threading

COM objects are bound to threads, so the platform crates use the owning-thread actor pattern: a long-lived thread owns the COM objects, and callers send it closures or requests over a channel and wait for the reply. Nothing implements `Send` by hand to move a COM object across threads.

- **UIA actor** (`deckhand-uia`). One MTA thread owns the `IUIAutomation` instance, the cache requests and the element registry. UIA objects never leave it.
- **Capture actor** (`deckhand-capture`). Owns the GDI surfaces and the DXGI duplication, and runs OCR.
- **Message threads** (`deckhand-desktop`, `deckhand-input`). The change monitor, activity monitor, hotkey listener, overlay and tray balloons each run a Win32 message loop on their own thread and stop by posting `WM_QUIT` when their owner is dropped.
- **Recorder.** A sampling thread pulls frames and keeps time; an encoding thread writes the file, so a slow encoder drops frames instead of stalling capture.
- **Server.** A two-thread tokio runtime for MCP, plus one reader and one writer thread per worker pipe.

Every thread that touches COM initializes it as MTA through a guard.

### Coordinates and DPI

The worker is Per-Monitor-V2 DPI aware, and every coordinate in the protocol is a physical pixel in virtual-screen space, including negative coordinates on monitors left of or above the primary. Screenshots carry a `scale` and an `origin`; a point target with an `img` ref is mapped back through that image's mapping, so clicking where the model sees something in a downscaled image lands on the right physical pixel. Absolute mouse input is normalized against the virtual screen, not the primary monitor.

## 4. The tool surface

Deckhand exposes two tools:

- **`observe`** sees the screen in any form, picked by `mode`: a window's elements (the default), full text, a screenshot, a recording, the windows, monitors or apps lists, the clipboard, or status.
- **`act`** runs a batch of up to 50 steps (`click`, `type`, `key`, `mouse`, `set`, `focus`, `window`, `pick`, `wait`, `clipboard`, `notify`) and reports what changed.

### Why two tools

- **Schema cost.** Every tool's schema is sent with every model turn. Two tools with short descriptions cost far less than a dozen, and a lot less than servers with dozens of tools. Both tools set Claude Code's `anthropic/alwaysLoad` so they are never deferred behind a tool search, which would cost an extra round trip per session.
- **Fewer decisions.** The model chooses between seeing and doing, then fills in a mode or a step. The earlier twelve-tool surface asked it to pick between `find`, `observe` and `read_text`, or between `windows` and `act`, for closely related jobs.
- **Batching by construction.** Anything that changes the desktop is a step, so a whole flow (launch an app, type, open the Save dialog, fill it) fits in one call. Waits are steps too, so the model never has to poll.
- **A clean permission split.** Clients can auto-allow `observe` and ask for `act`. `observe` is not marked read-only, because a recording writes a file, but it never sends input.

Arguments are strict, so a mistake fails loudly instead of being ignored:

- Unknown fields are rejected.
- `observe` arguments are validated per mode into a typed request before anything runs. A field from another mode is an `invalid_argument` that says where it belongs, such as "fps does not apply to mode=screenshot; it is for mode=record". Sources are exclusive: text takes at most one of `window`, `target` and `region`, and screenshots and recordings at most one of `target`, `region` and `monitor`. `all_windows` needs `query` and excludes `window`, `all` and `coords`.
- `act` steps check their own field combinations the same way: `mouse action=move` needs `target` and takes no `button`; `key action=down` takes exactly one chord and no `repeat`; `set` takes exactly one of `value`, `option`, `checked`, `expanded` and `selected`; `window action=launch` takes only `app` and `args`; `wait until=time` takes only `ms`.

### Output

Output is text designed to be cheap in tokens. A window is one line per element with a ref, indented by structure:

```
window w2032546 notepad.exe "Untitled - Notepad" foreground 1280x800@200,120 · focus e5
e1 window "Untitled - Notepad"
  e2 menubar "Application"
    e3 menuitem "File"
  e5 edit "Text editor" value="hello wor…" focused
other windows: w1180342 chrome.exe "Inbox", w66320 explorer.exe "Downloads" desktop 2
```

- By default only visible elements that can be acted on, plus readable text, are listed; named containers that hold listed elements stay for structure. `all` adds the rest, `coords` adds rectangles.
- Names are cut at 80 characters and values at 60. Password values are masked. The U+FFFC object-replacement marks that Electron and web apps scatter through UI Automation text are dropped from names, values and `mode=text` output, along with lines they leave empty.
- A typical window costs 1–3k tokens. A diff after an action is usually a few hundred.
- `act` reports one line per step with the method that actually ran and any note (a value that read back differently, a launched window, a notification Windows held back), then the after-state: a diff against the default window as it was before the batch, or, if the default window itself changed (a dialog opened, an app launched), its full element list.

### Targets

Every step accepts four target forms: an element ref (`e12`), a window ref (`w2032546`), a screenshot point (`{"x":640,"y":360,"img":"i3"}`), or a selector. Selectors borrow Terminator's syntax: `role:`, `name:`, `name~:`, `id:`, `class:`, `text:`, `window:`, `process:`, `nth:`, `focused`, and `>>` to search inside earlier matches; `ocr:` adds text read by OCR for apps without UI Automation (rule 14). They let the model act without observing first when it already knows the app. A selector whose window terms match several windows is resolved in all of them: a match on the current virtual desktop wins, then one in the foreground window, and anything still tied is an `ambiguous` error listing the windows, never a guess by z-order.

Element refs come from the registry in the UIA actor. Each ref is tied to the element's UIA RuntimeId while it lives, and carries a fingerprint: window, role, name, AutomationId, class and tree path. When the element is gone, a stale ref is re-found by fingerprint; if more than one element could be it, the call fails with the candidates. Entries for elements that haven't been seen for a while are evicted, and the registry has a hard size cap.

## 5. Speed techniques

| Goal | Technique |
|---|---|
| Fast window reads | One `BuildUpdatedCache(TreeScope_Subtree)` from the window root, with a minimal property set: name, control type, AutomationId, class, rectangle, enabled, off-screen, focusable, value, toggle/expand/selection state, and pattern availability. Filtering happens in Rust. Reads stop at an element cap and a time budget and say when they were cut short. When UIA can't build the subtree in one go, the read goes on one level at a time (see rule 12). |
| A tuned UIA client | `CUIAutomation8` created directly and configured through `IUIAutomation2` and `IUIAutomation6`: 2 s connection and 3 s transaction timeouts, event coalescing, connection recovery, and **auto-set-focus off**, so pattern calls never steal focus. |
| No fixed sleeps | After an input step, Deckhand waits until the target window has been quiet (no structure or property events) for `settle_quiet_ms`, capped at `settle_max_ms`. Waits are event-driven through WinEvent hooks, with a short poll as a fallback. |
| Waiting for targets | A selector that matches nothing yet is retried on change events for up to `auto_wait_ms`, so "click the button that is about to appear" is one step. |
| Fast clicks | `how: auto` uses an Invoke, Toggle, SelectionItem or ExpandCollapse pattern when the window is in the background, so nothing moves; otherwise a real SendInput click after a hit test confirms the element (or, for a window or point target, the window) is really under the point, before the glide and again right before the press. The hit reaches the target when it is the target, inside it, or encloses it: UIA's parent links lead from one to the other, or the hit lists, as one of its own children, an element on the target's parent chain (a host such as a WebView2 control whose hosted tree names another parent). Anything else covers the target: another window's element, or a same-window overlay or popup, and the error says which. |
| Fast typing | Text is one batch of `KEYEVENTF_UNICODE` events; newlines become Enter. Above 200 characters it is pasted, but only into a field whose value can be read back to confirm the paste, with the clipboard saved, restored only if its sequence number shows nobody else wrote it meanwhile, and the pasted text kept out of clipboard history. `set value` uses `ValuePattern.SetValue` and verifies the value, falling back to select-all and typing when the value didn't change at all; a value the app merely reformatted is kept and noted. |
| Fast screenshots | The foreground window and screen regions come from a DXGI duplication when it has a fresh frame, else a GDI copy; background windows use `PrintWindow`, falling back to screen pixels when the window can't be printed. Resizing uses SIMD `fast_image_resize` and encoding `jpeg-encoder`, which takes BGRA directly. Presets keep images at 960–1600 px unless `max` is asked for. |
| Fast launches | App names are resolved from `shell:AppsFolder` and Start-menu shortcuts, cached; packaged apps start through `IApplicationActivationManager` (which passes `args`), desktop apps through `ShellExecuteExW` on their `shell:AppsFolder` item. That drops `args`, so a desktop app given `args` runs the program its Start-menu entry points to (`System.Link.TargetParsingPath`), after the entry's own arguments (`System.Link.Arguments`, such as a Chrome profile), with the same `deny_paths` checks as a path launch; an entry that starts no program file refuses `args`. Deckhand waits for the new window on WindowOpened events, then returns its elements. |
| Fewer turns | Batches of steps, diffs with every action, selectors without a prior read, `pick` and `launch` doing whole flows, waits on the server, and all-window search without listing every window. |
| Fewer tokens | Text first and screenshots only when asked, compact element lines, diffs instead of trees, two tools with short descriptions. |
| Reads never blocked | The user-yield guard and the actor lock apply only to steps that send input. Reads run concurrently with acts. |

The numbers as built, on the author's laptop at 2560×1600, are in the README's [comparison with Windows-MCP](../README.md#compared-with-windows-mcp).

## 6. Reliability rules

1. **Never report success without checking.** Each step verifies its effect where it can: the value was set, the dialog closed, the window came forward, the element appeared. Otherwise the step fails with the reason. Notes report partial outcomes, such as a value that reads back differently.
2. **Report the method.** Every step line says how it was done (`invoke`, `mouse`, `keys`, `paste`, `value`, …), so the model can tell a pattern call from a real click.
3. **Sparse Chromium trees.** Chromium and Electron build their web accessibility tree lazily. If a Chromium window returns only a handful of elements on first touch, Deckhand retries once after a short delay.
4. **SetValue that doesn't stick.** Some web apps, such as React-controlled inputs, ignore `SetValue`. When the value is unchanged afterwards, `set` falls back to focusing and typing.
5. **Hung apps.** UIA timeouts first, then the call deadline, then terminating and restarting the worker. A hang never freezes the MCP server.
6. **Elevated targets.** The target window's integrity level is compared with Deckhand's; a higher one fails with `elevated_target` instead of failing silently, because Windows drops input to it.
7. **The secure desktop.** When a UAC prompt or the lock screen holds the input desktop, input steps fail with `needs_user`.
8. **File dialogs.** The Common Item Dialog and legacy dialogs are matched by control type and class, using control IDs (1148, 1001, 1152, Open 1, Cancel 2) only to confirm, so localization doesn't break them. Message boxes that a dialog raises are read and reported.
9. **Held inputs.** Keys and buttons held by `key action=down` or `mouse action=down` are released at the end of the call, however it ends, buttons before keys so a Ctrl-drag stays a copy, and the last step line says what was released. If the call ended on a failed step, a held button is released back at the point where it was pressed, so a half-finished drag drops where it started; an aborted `drag` step does the same. Held modifiers are lifted around Deckhand's own key presses (select-all, paste, Enter, Escape), and a `wait` that could last over 1 s is refused while anything is held, so the user's own keys are never modified for long. If a Deckhand process died while holding the actor lock, the next worker to start, or the next actor, finds the lock abandoned and releases any stuck keys and buttons first.
10. **Capture loss.** The DXGI duplication is reopened after access loss (UAC, resolution changes) and demoted to GDI when it stops delivering fresh frames.
11. **Pattern actions keep the user's focus.** Some Win32 controls perform Invoke or Toggle as a simulated click that activates their window. Deckhand records the foreground window before every pattern action and, if the target took it, hands it straight back (with the same foreground unlock it uses to bring windows forward) and notes it. Pattern actions also refuse a window that has a modal dialog open, as real clicks do.
12. **Trees that loop.** On Windows 11 build 26300, an open Win32 combobox reports its dropdown list as its only child, but the list's UIA parent is the desktop and its next siblings are every other top-level window, the combobox's own window included. WinUI 3 apps that host a WebView2 (Clipchamp) list the web content's root, a top-level `Chrome_WidgetWin_1` window of `msedgewebview2.exe`, twice: under the `WebView2` control and under the `DesktopChildSiteBridge`, which is its UIA parent; while the page loads, its siblings can run on into other apps' windows. Subtree caches through either fail with "Malformed Cacheresponse" (`E_FAIL`). When a subtree read fails with `E_FAIL`, Deckhand reads that node one level at a time, rebuilds each child's subtree, and splits further only where it fails again. Every read, one level or whole, keeps a top-level window child only when it is the combobox's own list (`GetComboBoxInfo`) or UIA places it under an element of a window already in the read (the read's own window, or one adopted that way), and lists each element (by runtime id) once, the first time it is reached. Snapshots, text reads and option searches all use this read, so they never reach other apps or list a subtree twice.
13. **Win32 combobox picks.** A Win32 combobox only shows its items while its list is dropped, `SelectionItem.Select` there sets the list's selection without telling the app, and collapsing the list cancels it back. `Invoke` on an item is a real mouse double click, so it is never used. After picking, Deckhand puts the chosen index back with `CB_SETCURSEL` if the collapse undid it, then sends the combobox the `LBN_SELCHANGE` its list would send. That is what Microsoft's own client-side UIA proxy for list items in a combobox does. The combobox then updates and notifies its parent with `CBN_SELCHANGE`, with no input and no focus change.
14. **Apps without UI Automation.** A window whose tree is only the window, unnamed containers and its title bar is read with OCR: `observe` elements appends the lines with their rectangles, `wait` text and plain-text `appears`/`disappears` poll OCR at most every 400 ms, and `ocr:"…"` selectors resolve to the rectangle of the matched words (a span of OCR word boxes on one line, exact matches before contains matches, `nth:` to pick one) and act on its centre as a point. An `ocr:` term combines only with `window:`, `process:` and `nth:`, so the OCR'd window is chosen like any other selector's. OCR runs on the same capture path as screenshots, so the overlay is hidden and denied windows are blacked out. The latest OCR reading is kept for the rest of an `act` call and reused while its window keeps its handle and rectangle and raises no change events, until a step other than `wait` or `notify` runs; a selector that is still being waited for reads again. Separately, `observe` text with `ocr: auto` switches to OCR when a window's UI Automation text has under 40 readable characters and OCR finds more, or when an element's has none; a region is always read with OCR.
15. **Launch matching.** A launched window is one that is new since the launch and belongs to the process Deckhand started or to a process it started (the Toolhelp parent chain, with every link created after the launch, so reused pids never match), or to the expected exe or package; an existing window that took the foreground counts too. Failing those, a new window whose process is the Start-menu entry's program or is named after the app, or whose title holds every word of the app's name, is taken. The window is then brought to the front with the usual foreground handover, unless it is on another virtual desktop.
16. **Covers.** At a point, a window of the same process as the target window (except shared hosts such as `ApplicationFrameHost.exe` and `explorer.exe`, where only menus count), or one owned by it, is part of the target; any other window covers it, and the agent's terminal, the taskbar and the desktop always do.
17. **Typing guard.** Before printable text is typed, the focused element is classified. Roles that never take text (buttons, list and tree items, tabs, menus, sliders, links and the like) and elements whose Value pattern is read-only are refused with an error naming them; edits and writable values pass, and anything generic or unknown passes too, since custom editors and `contenteditable` regions often look generic. `key` steps are not checked.

## 7. Safety model

The user must always be in charge, be able to see what the agent is doing, and be able to stop it instantly.

- **Kill switch.** A global hotkey, `Ctrl+Alt+Esc` by default: easy to hit in a hurry, present on every keyboard (many laptops have no Pause key), and not taken by a standard Windows shortcut. It is registered with `RegisterHotKey`, and the Raw Input monitor also watches the physical keyboard for the same chord, ignoring injected input: `RegisterHotKey` stops matching while extra modifiers are down, so without the second path a Shift or Win key that Deckhand is holding would block the stop. Deckhand's own keystrokes can never trigger it, and when both paths see the same press it counts once. The hotkey toggles a named event, `Local\deckhand-stopped`, that every Deckhand worker in the session checks, so one press stops every agent. Running batches stop between steps and inside waits; every `act` step, background steps included, fails with `stopped_by_user`; recordings stop and save; the overlay turns crimson until the user resumes. The model has no way to reset it, and the server's instructions tell it to report the stop rather than retry. Reads keep working.
- **User yield instead of refusal.** If the user touched the mouse or keyboard in the last `yield_ms`, an input step waits for that much quiet, up to `yield_timeout_ms`, then fails with `user_active`. Reads are never blocked, and neither are background steps, which never touch the user's input. Deckhand's own input carries a marker in `dwExtraInfo`, and the Raw Input monitor ignores injected events, so its own typing never counts as the user's. A glide or drag in progress stops at once when the kill switch is pressed or physical input arrives (`user_active`), and both are checked again right before every click, button press and drag start.
- **One actor at a time.** Each client session runs its own server and worker, so a named mutex (`Local\deckhand-actor`) serializes calls that act across sessions, background steps included. A call that can't get it within its wait limit fails with `busy`.
- **The host terminal.** At startup the worker walks its parent-process chain to the first ancestor with a window (stopping at the shell), which is the terminal hosting the agent. That window is never acted on, never the default window, and untargeted typing never lands in it. A host listed in `[guard] partial_hosts` that exposes the opt-in id `deckhand-protects-terminals` is opened partially: element and point targets outside any `deckhand-protected*` ancestor are allowed, while whole-window targets, `key` steps, window actions and anything inside a protected element are refused. The allowlist stops a web page shown inside some other host (say an editor's webview) from opting that host in; marking every session terminal, not just the agent's own, stops one agent from driving another session.
- **Deny lists.** Windows matching a deny rule (process, title, or both) are never read, captured or acted on, and paths under `deny_paths` are refused by `pick`, as recording paths, as `launch` exe paths and anywhere in `launch` arguments (shortcut paths are refused outright, since their target can't be checked). Paths are compared after resolving junctions and symbolic links. `observe mode=clipboard` refuses, with `denied`, content whose clipboard owner matches a rule and content marked private by the app that copied it (`ExcludeClipboardContentFromMonitorProcessing`, `Clipboard Viewer Ignore`, or `CanIncludeInClipboardHistory` set to 0), which is how password managers mark a copied secret. The only built-in rules are common password managers. Process rules are robust; title rules only see the title bar.
- **Shell windows.** `window` actions only control enumerated app windows, never the desktop or taskbar (`Progman`, `WorkerW`, `Shell_TrayWnd` and the like), so a stray `close` can't raise the Shut Down dialog.
- **Notifications.** At most three `notify` steps per call, and notifications from separate calls at least 5 s apart, so a looping agent can't flood the user.
- **Blackout.** Screen captures paint every denied window that overlaps the capture black, checking the window list before and after the capture. Background-window captures use `PrintWindow`, which contains only that window. Recordings re-check the window list continuously and paint each frame; if the list can't be read, the whole frame is painted black.
- **Elevation and the secure desktop.** Refused and reported, never driven. A signed `uiAccess` build would be needed to drive elevated windows (see §15).
- **Audit log.** One JSON line per call in `%LOCALAPPDATA%\deckhand\logs\YYYY-MM-DD.jsonl`: time, server pid, tool (`observe` or `act`), a summary of the arguments (starting with `mode=…` for `observe`, one entry per step for `act`), result, error code and duration. Typed text, key sequences, `set option` values, clipboard text and launch arguments are logged as a character count, and file and app paths as base names, unless `log_text` is on; notification text is always a count. A window, target or query that names a deny rule's title or process is replaced by `<denied window>`, whatever `log_text` says. No screen contents are ever written. Logs past `retention_days` are pruned at startup.
- **No side effects beyond the request.** Deckhand never writes to stdout outside the protocol, never changes system settings, writes files only to documented paths, and restores what it borrows, such as the clipboard.

## 8. Overlay

The overlay makes the agent visible without getting in the way. It runs on its own thread in the worker and draws with layered windows that are topmost, never activated, transparent to input, and excluded from capture with `WDA_EXCLUDEFROMCAPTURE`.

- **What it draws.** A conic gradient glow (a crisp inner line plus a soft bloom) that ignites from the top centre and flows around the driven window; a glass pill naming the agent and the stop hotkey; ripples with particles at click points; a name badge and trail on the cursor during glides; keycaps for named keys and modifier chords, with typed text, and printable keys sent by a `key` step, shown only as "typing…" (their characters and length never reach the overlay thread); a gradient outline on the field being typed into; a viewfinder over each screenshot's area; a faint sweep over windows the agent reads; a crimson pill with the elapsed time while recording; and a crimson "Stopped" state when the kill switch is pressed, which stays until the user resumes with the hotkey.
- **Only the current desktop.** Nothing is drawn for windows on other virtual desktops, since the user can't see them.
- **Cursor glide.** Mouse steps move the real cursor along an ease-in-out cubic path at about 120 Hz for `glide_ms` (120 ms by default, up to 3 s, 0 to jump). The motion is deliberate: it lets the user follow what the agent is doing.
- **The agent's name.** The MCP client's `clientInfo.name` is mapped to a product name: anything containing "claude" becomes Claude, and Codex, Cursor, Copilot (including VS Code clients), Windsurf and Gemini are recognised the same way. Other names are title-cased and cut to 24 characters; an empty name becomes "AI agent". `[overlay] label` overrides it.
- **Themes.** `aurora`, `sunset`, `ocean` and `mono` are looping palettes of four or five colours; `colors` replaces them with the user's own.
- **Cost.** A 15 ms frame timer runs only while something animates; when nothing does, the timer is killed and the thread sleeps in its message loop. Each frame repaints only what changed, from precomputed falloff tables, and large steady glows repaint at a lower rate. The result stays under 2% of one core while animating and zero when idle.
- **Reduced motion.** When Windows' animation effects are off, or `animations = false`, every element has a static version.
- **Demo mode.** With `capturable = true`, the overlay windows are not excluded from capture, so the user's screen recorders see them. Deckhand then hides the overlay for the instant of its own captures, with a short timeout so a failed capture can't leave it hidden; when the hide can't be confirmed within that timeout, the screenshot caption or OCR text carries a note that the overlay may appear in it.

## 9. Recording pipeline

```
Windows.Graphics.Capture (window or monitor) ─▶ D3D11 staging ─▶ crop / scale / letterbox ─▶ redaction ─▶ encoder thread
                                                                                                     ├─ Media Foundation sink writer: H.264 in fragmented MP4
                                                                                                     └─ GIF writer: changed area only, palette per frame
```

- **Source.** Windows.Graphics.Capture records a window (cropped to an element when asked) or a monitor; a region is recorded from the monitor it lies on, clipped to that monitor. Windows draws its yellow capture border around the source, which keeps recordings visible to the user. The cursor is captured or not as asked. A window on another virtual desktop is refused, because Windows doesn't draw it there.
- **Clock.** The sampler emits frames at a constant rate. When the source has no new frame, or capture falls behind, the previous frame is held, so nothing is ever shown early and the timeline matches wall time.
- **Size.** The output size is chosen at the start: the source scaled down to `max_width`, keeping its shape and even dimensions. A window resized mid-recording is scaled to fit, with black bars where the shape differs. A monitor that changes resolution or position is followed.
- **MP4.** Media Foundation's sink writer with hardware transforms enabled, writing H.264 into a fragmented MP4 container, tagged BT.709. The bitrate is 0.05, 0.08, 0.12 or 0.2 bits per pixel per frame for `low` to `max`, clamped to 1–60 Mbps. Outputs under 720 lines are converted to NV12 by Deckhand; larger ones are offered to the encoder as RGB32 first, with NV12 as the fallback. Fragmented MP4 means a file plays up to its last fragment, under a second from the end, even if the process is killed.
- **GIF.** At most 20 fps. Only the changed rectangle of each frame is written, and unchanged frames extend the previous frame's delay. `video_quality` sets how hard the palette is fitted.
- **Ending.** A recording ends at `max_seconds`, on the kill switch, when the recorded window closes, when the recorded monitor disappears, or on `stop`. The outcome (file, duration, frames, size, dropped frames, why it ended) is kept until the next `status`, `stop` or `start` reports it.
- **Files.** The output file is reserved with create-new semantics before recording starts, so nothing is ever overwritten: default names get `-2`, `-3` and so on, and an explicit path that exists is refused. Paths under `deny_paths` are refused.

## 10. Notifications

The `notify` step shows a WinRT toast under the AppUserModelID `Deckhand.Agent`, which the install script registers under `HKCU\Software\Classes\AppUserModelId` with a display name and icon. A toast from a registered identity shows as "Deckhand" and stays in Notification Center. Titles are limited to 63 UTF-16 units and messages to 255.

Before showing it, Deckhand checks `ToastNotifier.Setting`, so it can say when notifications are turned off for Deckhand, for the user or by policy, and `SHQueryUserNotificationState`, so it can say when Windows may hold the banner back (full-screen app, presentation mode, lock screen, the first hour after sign-in). Do Not Disturb is invisible to both, so the reply never claims the user saw the notification. A notification that Windows suppresses still counts as a successful step, because the request itself was valid; its note starts with "not shown:" and gives the reason.

Without the registered identity, such as a dev build run from `target`, Deckhand falls back to a tray-icon balloon on its own message thread, which Windows may also hold back. Balloons are dismissed when the worker shuts down.

## 11. Background mode and virtual desktops

Normal mode behaves like a careful person: bring the window forward, glide the mouse, type real keys. Background mode (`act { background: true }`, falling back to `[act] background`, which is off by default) uses only UI Automation patterns and never touches the real mouse, keyboard or focus. The per-step rules are in the README's [background mode table](../README.md#background-mode-and-virtual-desktops). In short: a click is a plain left single click through Invoke, Toggle, SelectionItem, ExpandCollapse and finally the legacy default action; typing is appended to the current value with `SetValue`, since patterns can't place a caret, and the step says so in a note; `window` allows only `close`, because the other actions change focus or the user's layout; and `key`, `focus`, `pick` and mouse move/down/up/drag need real input, so they are refused with `needs_user` and a hint at an alternative where one exists.

A step that needs real input is refused before it runs, rather than silently falling back to the mouse. Background steps skip the user-yield wait, since they can't collide with the user's input, but keep every other check: the actor lock, the kill switch, the deny lists and the elevation check.

**Virtual desktops.** A window is on another virtual desktop when it is shell-cloaked and, per `IVirtualDesktopManager`, not on this desktop; suspended UWP windows are shell-cloaked too, so the cloak alone is not enough. When Explorer's registry lists at most one desktop, shell-cloaked windows are simply hidden and no COM call is made, which keeps the window list near a third of a millisecond; otherwise one `GetWindowDesktopId` call per shell-cloaked window is compared with the current desktop's id from the registry. The desktop's number comes from the desktop order Explorer keeps in the registry; window lines show `desktop N`, or `another desktop` when the number can't be found. A shell-cloaked window whose desktop can't be looked up at all counts as on another desktop too, never as current, so it is never brought forward. The default window is always on the current desktop, and Deckhand never switches desktops: the APIs to do so are undocumented and change between Windows builds, and switching would yank the user's own workspace. Because real input only reaches the current desktop, steps aimed at a window on another desktop always get background behaviour, and a step that can't be done that way fails with an error naming the desktop. A screenshot of such a window comes from `PrintWindow` and fails, suggesting `mode=text`, when the result is blank because Windows hasn't drawn it. Recording such a window is refused, and the overlay draws nothing for it.

Two limits are inherent. Web apps that ignore `SetValue` need the real keyboard. And true parallel control, with the agent's own mouse and keyboard while the user works, needs a separate Windows session (RDP loopback, a VM, or Microsoft's Agent Workspace preview), which Windows 11 Home doesn't offer out of the box.

## 12. Testing strategy

- **Unit tests** sit next to the code. `deckhand-core` is pure and tested exhaustively: selector parsing and ranking, keys, diff, formatting and token budgets, validation, config, deadlines, quality presets, glide paths and the agent-name mapping.
- **Safe by default.** Plain `cargo test --workspace` may read the desktop (enumerate windows, snapshot, capture to memory) but must never inject input, move the mouse, change focus, write the clipboard, open or close windows, or leave files behind. It is safe to run while the user works.
- **Opt-in desktop tests.** Anything more is `#[ignore = "<reason>"]`: moving the mouse and keyboard, opening dialogs or changing focus, opening the fixture app, recording the screen or encoding video, showing the overlay, writing the clipboard, showing a notification, launching Calculator, moving or closing real windows, registering a system-wide hotkey or raw input. These run with `-- --ignored` only when the user can leave the PC alone. Tests never print or save screen contents or window titles.
- **The fixture app.** `deckhand-fixture` is a small Win32 app with known controls, delayed elements, Open and Save dialogs and a configurable title and position, so input tests have a stable target and never touch the user's own windows. Its library half, `FixtureProcess`, is the one launcher every test uses: it starts the app without activating it, on or off screen, waits for its window and closes it afterwards.
- **End to end.** `crates/deckhand/tests/e2e.rs` starts the real binary with a throwaway `APPDATA` and `LOCALAPPDATA`, speaks MCP over stdio, checks that exactly the two tools are listed, and drives the fixture through batches, waits, file dialogs, screenshots, all-window queries and window actions. It also checks that closing the server's input finalizes a running recording into a valid MP4.
- **Golden tasks** for comparing with Windows-MCP: Notepad (type 30 lines and save), Explorer (create, rename and move files in a scratch folder), Chrome (fill a web form including a native upload dialog), Calculator, reading Settings, and copying a paragraph from Chrome into Notepad.

## 13. Known limits

- Model thinking time remains. Deckhand cuts turns and context but can't remove model latency.
- The UAC prompt, Ctrl+Alt+Del and the lock screen live on the secure desktop and are unreachable. Elevated apps can't be driven by an unelevated process.
- Canvas apps and games expose no accessibility tree, so they can only be driven by OCR text (`ocr:` targets) or by screenshot and coordinates. Anti-cheat software may block injected input.
- Background mode only works where apps implement UI Automation patterns, and some web inputs ignore `SetValue`.
- Real input only reaches the current virtual desktop, and only one agent can hold the real mouse and keyboard at a time.
- Title deny rules only see the title bar.
- An unsigned build that injects input may be flagged by Defender or Smart App Control; signing matters for distribution.

## 14. Alternatives considered

| Option | Verdict |
|---|---|
| **Fix Windows-MCP** | No. Its slowness is structural: Python on a single-threaded apartment, sequential walks of every window, sleeps baked into the vendored UIA library, and acting by coordinates only. Fixing it meant a rewrite. |
| **Fork Cua Driver** (trycua/cua, Rust, MIT core) | The closest design: background clicks via UIA, refusals instead of fake success, element tokens, batching, CDP. But it has 65 tools (a heavy schema on every turn), is built around macOS with high churn, publishes no Windows latency numbers and doesn't handle DPI. Ideas borrowed, code not. |
| **Fork Terminator** (mediar-ai, Rust) | The best selector language and a good verify-after-action idea, but no release since April 2026 and an open shell-injection bug. The selector syntax and verification were borrowed. |
| **An HTTP transport** | Rejected. Every MCP client that matters can spawn a stdio server, and a network listener on a machine that injects input is attack surface for no gain. |
| **Screenshot-first control** | Rejected as the default. Images cost far more tokens than element lines, coordinates break on DPI and layout changes, and acting by element is more reliable. Screenshots remain available, with marks and point targets, for canvas apps and visual checks. Vision parsers such as OmniParser were also too slow (0.6 s or more per frame on a GPU). |
| **TypeScript or Python** | Rejected. The costs are COM calls, image work and input hooks; Rust reaches them directly through `windows-rs` with no interpreter lock or native-addon layer, starts in milliseconds and ships as one small executable. |

## 15. Future ideas

- **A signed `uiAccess` build** to drive elevated windows. Whether a self-signed certificate is enough for `uiAccess` is unverified.
- **A CDP fast path** for a dedicated automation Chrome profile, for pages where the accessibility tree is too coarse.
- **Macros:** record a successful `act` batch and replay it by name.
- **UIA Remote Operations**, to batch several pattern calls into one cross-process round trip.

## 16. Sources

- Measurements: `bench/deskbench`, plus transcript and MCP-log analysis of Windows-MCP 0.8.8.
- Windows-MCP: https://github.com/CursorTouch/Windows-MCP (issues #35, #87, #383, #416, #453)
- Cua Driver: https://github.com/trycua/cua and https://cua.ai/blog/inside-windows-computer-use
- Terminator: https://github.com/mediar-ai/terminator
- OSWorld-Human (LLM time is 75–94% of agent latency): https://arxiv.org/html/2506.16042v1
- UFO²: https://arxiv.org/html/2504.14603
- UIA caching: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-cachingforclients
- IUIAutomation6: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationclient/nn-uiautomationclient-iuiautomation6
- UIA security and uiAccess: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-securityoverview
- Chrome UIA provider: https://developer.chrome.com/blog/windows-uia-support-update
- Chrome 136 remote-debugging change: https://developer.chrome.com/blog/remote-debugging-port
- Claude vision sizing: https://platform.claude.com/docs/en/build-with-claude/vision
- Claude Code MCP: https://code.claude.com/docs/en/mcp
- rmcp: https://github.com/modelcontextprotocol/rust-sdk
