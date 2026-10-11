# Deckhand

Deckhand gives Claude Code fast, safe control of a Windows 11 desktop. It's an MCP server written in Rust that reads windows as compact element lists, acts on elements by ref or selector, waits on real UI events instead of sleeping, and reports what changed after every action.

`PLAN.md` explains why it exists and how it is designed. `CLAUDE.md` holds the rules for working on the code.

## Install

```powershell
.\scripts\install.ps1 -Register
```

This does four things:
1. Builds a release binary.
2. Copies it to `%LOCALAPPDATA%\Programs\deckhand\deckhand.exe`.
3. Checks that the copy's SHA-256 matches the build.
4. Registers it with Claude Code as the user-scope MCP server `deckhand`.

Restart open Claude Code sessions to load it. Re-running the script swaps in a new build safely, even while sessions are using the old one.

## Tools

| Tool | What it does |
|---|---|
| `observe` | Reads a window as one line per element with a ref (`e12`). It reads the foreground app by default, skipping Claude's own terminal. |
| `act` | Runs up to 50 steps in one call, then reports a diff of what changed. If a new dialog took over, it returns the dialog's elements instead. |
| `find` | Searches by selector or text in one window or in all of them. |
| `screenshot` | Captures a window, element, region or monitor as a JPEG or PNG, optionally with ref labels drawn on. `preset` picks the size and detail: `low`, `medium` (default), `high` or `max` (native pixels). |
| `record` | Records the screen, a window, an element or a region to MP4 (H.264) or GIF. `start`, `stop` and `status`; one recording at a time. |
| `wait_for` | Waits until an element appears or disappears, a window opens or closes, some text shows up, or the UI goes idle. |
| `windows` | Lists, focuses, launches, closes, minimizes, maximizes, restores or moves windows. `apps` lists installed apps (optionally filtered by name), and `monitors` lists the displays with their work area and scale, numbered as `screenshot` numbers them. |
| `pick_file` | Fills a native Open or Save dialog, optionally clicking an Upload button first. |
| `read_text` | Gets the full text of an element or window through UI Automation, falling back to OCR. On a browser window it returns the page text. |
| `clipboard` | Gets or sets clipboard text. |
| `notify` | Shows you a Windows notification, for example when a long task finishes. It reports when Focus Assist holds it back. |
| `status` | Shows health, the kill switch, user activity and recent timings. |

**Targets** take one of these forms:
- an element ref like `e12`
- a window ref like `w2032546`
- a point from a screenshot, `{"x":640,"y":360,"img":"i3"}`
- a selector:

```
role:button name:"Save"
window:"Notepad" >> role:menuitem name:File
name~:report nth:2
id:1001
process:chrome text:"Sign in"
```

**Act steps** look like `{"do":"click","target":"e12"}` or `{"do":"type","target":"e3","text":"hello","enter":true}`. The step types are:

`click` · `hover` · `type` · `set_value` · `key` · `scroll` · `drag` · `select` · `toggle` · `expand` · `collapse` · `focus` · `wait_for` · `wait`

## Compared with Windows-MCP

Deckhand replaces Windows-MCP. Each Windows-MCP tool maps to a Deckhand tool like this:

| Windows-MCP | Deckhand |
|---|---|
| Click / Type / Scroll / Move / Shortcut / Wait / MultiSelect / MultiEdit | `act` steps |
| WaitFor | `wait_for` |
| Snapshot | `observe` |
| Screenshot | `screenshot` |
| App | `windows` (`launch`, `apps` and the window actions) |
| Clipboard | `clipboard` |
| ControlStatus | `status` |
| DisplayInventory | `windows` with `action: "monitors"` |
| Notification | `notify` |
| Scrape | `read_text` on the browser window, plus Claude Code's WebFetch |
| PowerShell / Registry / FileSystem / Process | Not included: Claude Code has its own tools for these, governed by its permission rules |

**Screenshot quality.** Presets set the maximum width and the JPEG quality:

| Preset | Max width | JPEG quality |
|---|---|---|
| `low` | 960 | 60 |
| `medium` (default) | 1280 | 80 |
| `high` | 1600 | 88 |
| `max` | native resolution | 95, or lossless with `format: png` |

`max_width` (64–2576), `quality` (1–100) and `format` override the preset. The caption shows the quality used, e.g. `i3 1280x800 jpeg · q80 · …`.

**Recording** looks like this:

```json
{"action":"start"}
{"action":"start","target":"window:\"Notepad\"","fps":15}
{"action":"start","monitor":0,"format":"gif","max_seconds":20}
{"action":"start","region":{"x":0,"y":0,"width":1280,"height":720},"quality":"max","cursor":false}
{"action":"start","target":"e12","path":"C:\\clips\\button.mp4"}
{"action":"status"}
{"action":"stop"}
```

- With no `target`, `region` or `monitor`, it records the same window `observe` reads by default. An element records its window, cropped to the element.
- `start` replies `recording to C:\…\deckhand-20261011-101500.mp4 · 1920x1200 @ 30 fps · stop with record action stop`. `stop` replies `saved C:\…\x.mp4 · 12.4 s · 372 frames · 4.1 MB`, plus a note when frames were dropped or the recording ended on its own.
- The video has a constant frame rate. When nothing changes on screen, the previous frame repeats. If the window is resized while recording, the picture is scaled to fit the size chosen at the start, with black bars where the shape differs.
- It stops by itself at `max_seconds`, when the kill hotkey is pressed, or when the recorded window closes. The next `status` or `stop` reports the saved file.
- Files are never overwritten. A default name that is taken gets `-2`, `-3` and so on; an explicit `path` that exists is refused.

## Safety

- **Kill switch.** `Ctrl+Alt+Shift+Backspace` stops all input from every Deckhand session; press it again to resume. Claude cannot undo it. It also stops and saves any recording in progress, and no new recording starts until you resume.
- **Recordings are visible.** While a recording runs, Windows draws a yellow border around the recorded window or monitor. Monitor and region recordings black out denied windows (checked about twice a second), and a denied window can't be recorded at all. Windows that hide themselves from capture come out black.
- **You keep priority.** While you're using the mouse or keyboard, input steps wait for you to stop. Read-only tools are never blocked.
- **Claude's own terminal is off-limits.** Deckhand won't type into or act on the terminal that hosts Claude.
- **Deny list.** Password managers, and any windows or folders you add in the config, are never read or acted on. Screenshots black out denied windows. `pick_file` refuses paths under the folders in `deny_paths`.
- **Elevation and the secure desktop.** Elevated windows, UAC prompts and the lock screen are reported, never driven.
- **Audit log.** `%LOCALAPPDATA%\deckhand\logs\YYYY-MM-DD.jsonl` keeps one line per call. Typed text and file names are redacted.
- **Visible agent.** A thin border marks the window being driven, and a ripple marks each click. Both are hidden from screenshots.

## Configuration

`%APPDATA%\deckhand\config.toml` is optional. If the file is invalid, every tool except `status` refuses to run until it is fixed.

```toml
[guard]
deny = [{ process = "MyVault.exe" }, { process = "explorer.exe", title = "Private" }]
deny_paths = ['D:\Secrets']
yield_ms = 600
kill_hotkey = "ctrl+alt+shift+backspace"

[act]
settle_quiet_ms = 120
settle_max_ms = 1000
auto_wait_ms = 2000

[screenshot]
preset = "medium"

[record]
fps = 30
quality = "high"
max_seconds = 300
cursor = true
format = "mp4"
max_width = 1920
folder = 'D:\Clips'

[log]
log_text = false
retention_days = 14
```

User `deny` and `deny_paths` entries are added to the built-in ones; they don't replace them.

**`[screenshot]`.** `preset` is `low`, `medium` (default), `high` or `max`. The older `max_width` and `quality` keys still work: when set, they override the preset, so existing configs behave as before. Arguments in a call override everything here.

**`[record]`.** Every key is optional, and every `record` argument overrides its key:

| Key | Default | Meaning |
|---|---|---|
| `fps` | 30 | Frames per second, 1–60. GIFs are capped at 20. |
| `quality` | `"high"` | `low`, `medium`, `high` or `max`. For MP4 this sets the bitrate (0.05 / 0.08 / 0.12 / 0.2 bits per pixel per frame, within 1–60 Mbps); for GIF, how hard the palette is fitted. |
| `max_seconds` | 300 | Stop and save after this long, 1–1800. |
| `cursor` | true | Show the mouse pointer. |
| `format` | `"mp4"` | `mp4` or `gif`. |
| `max_width` | 1920 (GIF 960) | Larger sources are scaled down, keeping their shape and even dimensions. |
| `folder` | `%USERPROFILE%\Videos\Deckhand` | Where default-named recordings go; created if missing. |

**Unfinished recordings.** An MP4 becomes playable only when it is finalized, on `stop`, at `max_seconds`, on the kill hotkey or when the window closes. If the worker is restarted mid-recording (a hung call, or the Claude Code session ending), the MP4 is left unplayable. A GIF usually still shows the frames written before the restart.

## Development

```powershell
.\scripts\check.ps1
cargo test --workspace
cargo build -p deckhand-fixture
cargo test -p deckhand --test e2e -- --ignored --test-threads=1
```

- **`check.ps1`** runs fmt, clippy (`-D warnings`) and the tests.
- **`cargo test --workspace`** runs only tests that are safe while you work.
- **The `e2e` test** drives the real mouse and keyboard against the fixture app, so run it only when you can leave the PC alone.
- **Recording tests** are ignored by default because they record the screen (with the yellow border) and write video files. Run them with `cargo test --release -p deckhand-capture -p deckhand-worker record -- --ignored --test-threads=1`. They write under `%TEMP%\deckhand-record-tests`, or `DECKHAND_RECORD_TEST_DIR` if set, and delete what they wrote.

| Crate | Role |
|---|---|
| `deckhand-core` | Protocol, selectors, keys, diff, text formatting, config, IPC framing |
| `deckhand-uia` | UI Automation on one thread: snapshots, stable refs, pattern actions |
| `deckhand-capture` | GDI and DXGI capture, PrintWindow, resize, JPEG/PNG encoding, marks, OCR, Windows.Graphics.Capture recording to MP4 and GIF |
| `deckhand-input` | SendInput batches, Raw Input activity monitor, kill-switch hotkey |
| `deckhand-desktop` | Windows, focus, launch, clipboard, monitors, change monitor, overlay |
| `deckhand-worker` | The engine behind every tool |
| `deckhand` | The binary: MCP server, worker supervisor, audit log |
| `deckhand-fixture` | A test-only Win32 app |

## License

MIT. See [LICENSE](LICENSE).
