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
| `screenshot` | Captures a window, element, region or monitor as a JPEG or PNG, optionally with ref labels drawn on. |
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

## Safety

- **Kill switch.** `Ctrl+Alt+Shift+Backspace` stops all input from every Deckhand session; press it again to resume. Claude cannot undo it.
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
max_width = 1280
quality = 80

[log]
log_text = false
retention_days = 14
```

User `deny` and `deny_paths` entries are added to the built-in ones; they don't replace them.

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

| Crate | Role |
|---|---|
| `deckhand-core` | Protocol, selectors, keys, diff, text formatting, config, IPC framing |
| `deckhand-uia` | UI Automation on one thread: snapshots, stable refs, pattern actions |
| `deckhand-capture` | GDI and DXGI capture, PrintWindow, resize, JPEG/PNG encoding, marks, OCR |
| `deckhand-input` | SendInput batches, Raw Input activity monitor, kill-switch hotkey |
| `deckhand-desktop` | Windows, focus, launch, clipboard, monitors, change monitor, overlay |
| `deckhand-worker` | The engine behind every tool |
| `deckhand` | The binary: MCP server, worker supervisor, audit log |
| `deckhand-fixture` | A test-only Win32 app |

## License

MIT. See [LICENSE](LICENSE).
