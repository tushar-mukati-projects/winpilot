# Deckhand

A fast Windows desktop-control MCP server for Claude Code, in Rust. `PLAN.md` explains the why and the design. This file holds the rules for working in the code.

## Commands

```powershell
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`scripts/check.ps1` runs fmt check, clippy and tests in one go. All three must be clean before work is called done.

## Crates

| Crate | Role | Depends on |
|---|---|---|
| `deckhand-core` | Pure logic and the shared protocol: types, errors, geometry, selectors, keys, diff, text formatting, config, IPC framing. No Windows APIs. | serde, schemars |
| `deckhand-uia` | UI Automation on one dedicated MTA thread: snapshots, element refs, pattern actions | core, windows |
| `deckhand-capture` | Screen capture (warm DXGI, GDI, PrintWindow), resize, JPEG/PNG encode, marks, OCR | core, windows |
| `deckhand-input` | SendInput mouse/keyboard/text, physical-input monitor, kill-switch hotkey | core, windows |
| `deckhand-desktop` | Windows, focus, launch, clipboard, monitors, integrity, change monitor, overlay, named sync objects | core, input, windows |
| `deckhand-worker` | The engine: runs each tool call using the platform crates | all of the above |
| `deckhand` | The binary: `deckhand mcp` (rmcp stdio server and worker supervisor) and `deckhand worker` | core, worker, rmcp |
| `deckhand-fixture` | A test-only Win32 app with known controls and file dialogs | windows |

Public APIs live in each crate's `lib.rs`, which only declares modules and re-exports. Those signatures are the contract between crates; change them only when every caller is updated in the same change.

## Code rules

- **No comments of any kind**: no `//`, `///`, `//!` or `/* */`. Names carry the meaning. If something needs explaining, rename it or split it; longer reasoning goes in commit messages or `PLAN.md`. Schema descriptions use `#[schemars(description = "...")]`, never doc comments.
- **Readable first.** Small functions with one job (aim under 40 lines), descriptive names, early returns, `let … else`, no nesting deeper than three levels. One concept per file.
- **No side effects beyond the request.**
  - Never write to stdout outside the MCP transport: it is the protocol stream (`print_stdout` is denied).
  - Restore whatever you borrow, such as the clipboard.
  - Write files only to documented paths.
  - Never change system settings.
  - Leave the user's windows, focus and mouse alone unless the tool call asks for it.
- **Errors:** return `deckhand_core::Result`. Use `Error::new(ErrorCode::…, message)` for meaningful failures and `.context("what was being done")` for OS errors.
  - No `unwrap`/`expect` outside tests.
  - No panics on any input.
  - Every Win32/COM return value is checked.
- **Unsafe:** keep each `unsafe` block as small as possible inside a safe function with a precise name.
  - Wrap OS handles in RAII types that close on drop, and never leak a handle.
  - Pair every `CoInitializeEx` with `CoUninitialize` through a guard.
  - Never `impl Send`/`Sync` by hand to smuggle COM objects across threads. Use the owning-thread actor pattern instead (see `deckhand-uia`).
- **Threads:**
  - Every thread that touches COM initialises it as MTA through a guard.
  - UIA objects never leave the UIA thread.
  - Long-lived threads must stop cleanly when their owner is dropped (post `WM_QUIT`, close channels).
- **Coordinates:** physical pixels in virtual-screen space everywhere. Worker processes are Per-Monitor-V2 DPI aware.
- **Dependencies:** declare them in the root `[workspace.dependencies]` only, and reuse what's there before adding anything.
- **Formatting:** `cargo fmt --all` with the repo `rustfmt.toml` (width 120).

## Tests

- Unit tests sit next to the code in `#[cfg(test)] mod tests`. Pure logic in `deckhand-core` is tested exhaustively.
- Default `cargo test` must be safe to run while the user works. It may read the desktop (enumerate windows, snapshot, capture to memory). It must never:
  - inject input, move the mouse, or change focus;
  - write the clipboard;
  - open or close windows;
  - leave files behind.
- Tests that touch the desktop beyond reading carry `#[ignore = "moves the mouse"]` (or a similar reason) and are run explicitly with `cargo test -- --ignored`, only when the user has agreed.
- Never print or save screen contents or window titles in test output.

## Safety rails for anyone working here

- Never read, open, capture or pass paths under the folders in `[guard] deny_paths`. It holds secrets.
- Don't use the `mcp__windows-mcp__*` or `mcp__claude-in-chrome__*` tools while developing. The user's own windows are live.
- Don't run git commands that change history or the working tree (commit, stash, checkout, reset) unless asked.
