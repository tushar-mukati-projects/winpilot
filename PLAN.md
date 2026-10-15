# WinPilot plan

## Goal

Give AI agents fast, safe control of a Windows PC through MCP. Read apps the way a screen reader does, act like a careful person, and never get in the user's way.

## Flow

```
MCP client ──stdio──> winpilot mcp ──pipe──> winpilot worker ──> Windows
 (Claude, Codex)       server: tools,          engine: observe,      UIA, SendInput,
                       deadlines, audit        act, guard, overlay   DXGI/WGC, WASAPI
```

- The **server** holds no Windows state. It validates arguments, gives each call a deadline, logs a redacted audit line and restarts the worker if it hangs or crashes.
- **WSL:** Claude Code inside WSL runs the same Windows exe through interop. The server translates Linux paths in `path`, `pick` and `launch` (`/mnt/c/...` to `C:\...`, `/home/...` to `\\wsl.localhost\<distro>\...`, with the distro from `WSLENV=WSL_DISTRO_NAME`).
- The **worker** is a child process inside a Job Object, so a crash or hang never takes the server down with it. It is Per-Monitor-V2 DPI aware, and every coordinate is a physical pixel.

## Crates

| Crate | Owns |
|---|---|
| `core` | Protocol, arguments and limits, selectors, keys, config, output text, OCR matching. Pure Rust, no Windows. |
| `uia` | UI Automation on one MTA thread: snapshots, stable element refs, patterns. |
| `capture` | Screenshots (DXGI, GDI, PrintWindow), OCR, recording (WGC to H.264 + AAC MP4, or GIF), WASAPI audio. |
| `input` | SendInput mouse and keyboard, window-message input for background mode, user-activity monitor, kill hotkey. |
| `desktop` | Windows, focus, launch, stacking, clipboard, monitors, virtual desktops, overlay, notifications. |
| `speech` | Offline Whisper on candle: ggml model reader, cached decoder, resampling, media decoding. |
| `worker` | The engine that runs every observe mode and act step. |
| `winpilot` | The binary: `winpilot mcp` (server) and `winpilot worker`. |
| `fixture` | A plain Win32 test app with known controls and dialogs. |

## observe

| Mode | How |
|---|---|
| `elements` | A UIA snapshot as lines with refs (`e12`). Apps without UIA fall back to OCR lines with positions. |
| `text` | UIA text, or OCR. |
| `screenshot` | DXGI, or GDI as a fallback. Denied windows are painted black. The `img` id maps points back to the screen. |
| `record` | WGC frames plus loopback and/or mic audio, muxed by Media Foundation. Silence is filled so audio stays in sync. |
| `listen` | Captures up to 90 s of audio, or decodes a file, and transcribes it locally. The model loads once per worker. |
| `windows`, `monitors`, `apps`, `clipboard`, `status` | Lists and state. |

## act

Each step: resolve the target (ref, selector, `ocr:` text or point), pick the mode, check the guard, perform, settle, then diff.

| | Foreground (default) | Background (`background: true`) |
|---|---|---|
| Input | Real mouse glide and keystrokes | UIA patterns, then window messages to the innermost child window |
| Focus | Brings the window forward | Never. Keeps the target below the user's window and hands back any focus an app grabs |
| Refused | Nothing extra | Ctrl/Alt/Shift chords, mouse move/down/up, focus, and the user's own window |

Windows on other virtual desktops always use the background path.

## Safety

- **Kill switch:** Ctrl+Alt+Esc, also detected on the physical keyboard, stops every session. It also stops recordings.
- **User first:** foreground steps wait while the user is typing or moving the mouse.
- **One actor:** a machine-wide lock means only one agent drives the mouse at a time.
- **Off limits:** the agent's own terminal, deny-listed apps and paths, elevated windows, UAC.
- **Partial hosts:** an app that holds many sessions (Agent Deck) can opt in with `winpilot-protects-terminals`. Its buttons become usable, but every `winpilot-protected-*` terminal stays refused.
- **Privacy:** typed text never shows in the overlay or the log, and denied windows are blacked out in captures.

## Overlay

| Step | Shown |
|---|---|
| Foreground | Glow on the window, cursor glide, click ripples, key chips |
| Background, target visible | Glow, ripples and chips on the window, no cursor glide |
| Background, target covered | One bottom pill: "Claude · working in CapCut behind your windows · click" |

The overlay is hidden from screenshots and recordings unless `[overlay] capturable = true`.

## Build and release

- `scripts/check.ps1` runs fmt, clippy (warnings as errors) and the tests. The default tests never touch input, focus, the clipboard or the screen; live tests are `#[ignore]`.
- `scripts/install.ps1 [-Register] [-SpeechModel base]` builds, installs to `%LOCALAPPDATA%\Programs\winpilot`, checks hashes and registers the MCP server.
- Releases attach `winpilot-<version>-windows-x64.zip` with `winpilot.exe` and its icon.

## Next

- `listen`: the `small` model for Hindi and noisy audio, and a longer live mode that streams.
- Background: fewer focus flashes from classic Win32 buttons and newly opened dialogs.
- Live test runs in CapCut for message input and the overlay pill.
