# WinPilot

A fast Windows desktop-control MCP server for AI agents, in Rust, with two tools: `observe` and `act`. `README.md` is the user reference and `docs/DESIGN.md` explains the why and the design. This file holds the rules for working in the code.

## Commands

```powershell
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`scripts/check.ps1` runs fmt check, clippy and tests in one go. All three must be clean before work is called done.

```powershell
cargo build -p winpilot-fixture
cargo test -p winpilot --test e2e -- --ignored --test-threads=1
.\scripts\install.ps1 -Register
```

```powershell
.\scripts\install.ps1 -SpeechModel base
cargo test -p winpilot-speech -- --ignored --test-threads=1
```

The first line downloads the Whisper model once into `%LOCALAPPDATA%\winpilot\models` and checks its pinned SHA-256; it builds and installs nothing. The ignored speech tests need that model (they skip themselves without it), write and delete a temporary WAV, and touch no audio devices. Set `WINPILOT_SPEECH_SAMPLE` to a media file (and optionally `WINPILOT_SPEECH_LANGUAGE`) and add `--nocapture` to print its transcript and the real-time factor; use `--release` for representative speed.

The e2e test drives the real mouse and keyboard against the fixture app, so build the fixture first and run it only when the user agrees. The optional benchmark sits outside the workspace: `cargo run --release --manifest-path bench/deskbench/Cargo.toml` (`--skip-uia`, `--skip-capture`, `--skip-startup`, `--webview-process <exe>`, `--deny-title <text>`). `scripts/install.ps1` builds, installs to `%LOCALAPPDATA%\Programs\winpilot`, registers the `WinPilot.Agent` notification identity and, with `-Register`, the Claude Code MCP server. `-Uninstall` undoes it; `-Purge` also removes config and logs.

## Crates

| Crate | Role | Depends on |
|---|---|---|
| `winpilot-core` | Pure logic and the shared protocol: tool arguments and validation, errors, geometry, selectors, keys, diff, output formatting, config, themes, agent names, OCR text matching, IPC framing. No Windows APIs. | serde, serde_json, schemars |
| `winpilot-uia` | UI Automation on one dedicated MTA thread: snapshots, element refs and fingerprints, pattern actions, text | core, windows |
| `winpilot-capture` | Screen capture (DXGI, GDI, PrintWindow), resize, JPEG/PNG encode, marks, OCR, recording (WGC to Media Foundation MP4 and GIF) | core, windows |
| `winpilot-input` | SendInput mouse/keyboard/text, window-message input for background mode, cursor glide, physical-input monitor, kill-switch hotkey | core, windows |
| `winpilot-desktop` | Windows, focus, launch, clipboard, monitors, virtual desktops, integrity, change monitor, overlay, notifications, named sync objects | core, windows |
| `winpilot-worker` | The engine: runs every observe mode and act step using the platform crates | all of the above |
| `winpilot` | The binary: `winpilot mcp` (rmcp stdio server, worker supervisor, audit log) and `winpilot worker` | core, worker, rmcp |
| `winpilot-speech` | Local speech-to-text: whisper.cpp ggml model reader, Whisper on candle with a cached decoder, resampling, Media Foundation media decoding. Takes plain samples; does not depend on capture. | core, windows, candle |
| `winpilot-fixture` | A test-only Win32 app with known controls and file dialogs, and a library (`FixtureProcess`) that tests use to launch it; a dev-dependency only | windows |

Public APIs live in each crate's `lib.rs`, which only declares modules and re-exports. Those signatures are the contract between crates; change them only when every caller is updated in the same change.

## Where things live

| What | Where |
|---|---|
| Tool definitions, descriptions, server instructions | `crates/winpilot/src/mcp.rs` |
| Worker supervision: spawn, Job Object, deadlines, backoff, shutdown | `crates/winpilot/src/` (`supervisor`, `lifecycle`, `link`, `spawn`, `job`) |
| Audit log and its redaction | `crates/winpilot/src/audit.rs`, `summary.rs` |
| `observe` arguments and per-mode field rules | `crates/core/src/observe.rs`, `observe_fields.rs`, `observe_request.rs` |
| `act` steps and `wait` arguments | `crates/core/src/steps.rs`, `wait.rs` |
| Argument limits and cross-field checks | `crates/core/src/validation.rs` |
| Output text for every reply | `crates/core/src/format.rs` |
| Config keys and defaults | `crates/core/src/config.rs` |
| Selectors, keys, themes, agent names | `crates/core/src/selector.rs`, `keys.rs`, `theme.rs`, `agent.rs` |
| `observe` modes | `crates/worker/src/observe/`: `elements`, `search` (`all_windows`), `text`, `screenshot`, `record` (with `record/`), `windows`, `monitors`, `apps`, `clipboard`, `status` |
| `act` steps | `crates/worker/src/act/`: one module per step (`click`, `typing`, `keys`, `mouse`, `set`, `focus_step`, `window`, `launch`, `pick` with `pick/`, `wait_step`, `clipboard`, `notify`) plus shared helpers |
| Background mode: refusals, the per-window mode and the window-message path | `crates/worker/src/act/refusal.rs`, `mode.rs`, `append.rs`, `message.rs`, `message_keys.rs`; message plans and delivery in `crates/input/src/message_*.rs` |
| Kill switch, user yield, deny lists, actor lock | `crates/worker/src/guard.rs`, `kill_switch.rs` |
| OCR text matching and `ocr:` targets | `crates/core/src/ocr.rs`, `crates/worker/src/ocr_read.rs`, `ocr_target.rs`, `wait/ocr_look.rs` |
| The agent's own terminal | `crates/worker/src/host.rs`, partial hosts in `host_terminal.rs`, ids in `crates/core/src/host_marks.rs` |
| Recording encoder | `crates/capture/src/recorder/` |
| Virtual desktops | `crates/desktop/src/virtual_desktops.rs`, `desktop_ids.rs` |
| Overlay | `crates/desktop/src/overlay/` |
| Notifications | `crates/desktop/src/notify.rs`, `toast.rs`, `balloon*.rs` |
| Speech-to-text | `crates/speech/src/`: `transcriber`, `windowing`, `decoder` and `text_decoder` (model), `logits` (timestamp rules), `ggml` and `tensor_names` (model file), `audio` (resample), `media` (file decoding), `paths` (model location) |
| Fixture launcher for tests | `crates/fixture/src/launch.rs` (`FixtureProcess`, re-exported from `lib.rs`) |

## Code rules

- **No comments of any kind**: no `//`, `///`, `//!` or `/* */`. Names carry the meaning. If something needs explaining, rename it or split it; longer reasoning goes in commit messages or `docs/DESIGN.md`. Schema descriptions use `#[schemars(description = "...")]`, never doc comments.
- **Readable first.** Small functions with one job (aim under 40 lines), descriptive names, early returns, `let … else`, no nesting deeper than three levels. One concept per file.
- **No side effects beyond the request.**
  - Never write to stdout outside the MCP transport: it is the protocol stream (`print_stdout` is denied).
  - Restore whatever you borrow, such as the clipboard.
  - Write files only to documented paths.
  - Never change system settings.
  - Leave the user's windows, focus and mouse alone unless the tool call asks for it.
- **Errors:** return `winpilot_core::Result`. Use `Error::new(ErrorCode::…, message)` for meaningful failures and `.context("what was being done")` for OS errors.
  - No `unwrap`/`expect` outside tests.
  - No panics on any input.
  - Every Win32/COM return value is checked.
- **Unsafe:** keep each `unsafe` block as small as possible inside a safe function with a precise name.
  - Wrap OS handles in RAII types that close on drop, and never leak a handle.
  - Pair every `CoInitializeEx` with `CoUninitialize` through a guard.
  - Never `impl Send`/`Sync` by hand to smuggle COM objects across threads. Use the owning-thread actor pattern instead (see `winpilot-uia`).
- **Threads:**
  - Every thread that touches COM initialises it as MTA through a guard.
  - UIA objects never leave the UIA thread.
  - Long-lived threads must stop cleanly when their owner is dropped (post `WM_QUIT`, close channels).
- **Coordinates:** physical pixels in virtual-screen space everywhere. Worker processes are Per-Monitor-V2 DPI aware.
- **Dependencies:** declare them in the root `[workspace.dependencies]` only, and reuse what's there before adding anything.
- **Formatting:** `cargo fmt --all` with the repo `rustfmt.toml` (width 120).

## Tests

- Unit tests sit next to the code in `#[cfg(test)] mod tests`. Pure logic in `winpilot-core` is tested exhaustively.
- Default `cargo test` must be safe to run while the user works. It may read the desktop (enumerate windows, snapshot, capture to memory). It must never:
  - inject input, move the mouse, or change focus;
  - write the clipboard;
  - open or close windows;
  - show the overlay or a notification;
  - record the screen or encode video;
  - leave files behind.
- Tests that do any of that carry `#[ignore = "<reason>"]` with a reason that names the effect, such as `"moves the mouse and keyboard"`, `"opens dialogs / changes focus"`, `"opens the fixture app off-screen"`, `"records the screen"`, `"encodes video files"`, `"shows the overlay"`, `"writes the clipboard"` or `"shows a notification"`. Run them explicitly with `cargo test -- --ignored` (filtered by name, `--test-threads=1`), only when the user has agreed.
- Recording tests each write into their own `winpilot-record-test-<name>-<pid>-<n>` folder under `%TEMP%` (or `WINPILOT_RECORD_TEST_DIR`) and delete it afterwards.
- Never print or save screen contents or window titles in test output.

## Safety rails for anyone working here

- Never read, open, capture or pass paths that the user's WinPilot config denies (`[guard] deny_paths`).
- Don't use the `mcp__windows-mcp__*` or `mcp__claude-in-chrome__*` tools while developing. The user's own windows are live.
- Don't run git commands that change history or the working tree (commit, stash, checkout, reset) unless asked.
