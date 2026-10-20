# WinPilot

WinPilot lets an AI agent use your Windows PC. It reads apps the way a screen reader does, falls back to OCR for apps that don't expose their controls, and clicks, types and fills in forms. It is an MCP server written in Rust, and everything runs on your machine.

![WinPilot reading, clicking, typing, uploading a file, dragging and working in the background](assets/demo.gif)

## Install

1. Download `winpilot-0.1.0-windows-x64.zip` from [Releases](../../releases) and unzip it to `%LOCALAPPDATA%\Programs\winpilot`.
2. Register it with Claude Code, then restart Claude Code or run `/mcp`:

   ```powershell
   claude mcp add -s user winpilot -- "$env:LOCALAPPDATA\Programs\winpilot\winpilot.exe" mcp
   ```

Other MCP clients work the same way: the command is `winpilot.exe mcp`.

From WSL, register the same Windows exe by its `/mnt/c/...` path. Paths under `/mnt/<drive>/` are translated; files that live only inside WSL have to be copied under `/mnt/c` first.

From source (Rust and the VS Build Tools): `.\scripts\install.ps1 -Register`. For `listen`, download the speech model once with `.\scripts\install.ps1 -SpeechModel base`.

## Tools

`observe` looks at the screen: `elements`, `text`, `screenshot`, `record`, `listen`, `windows`, `monitors`, `apps`, `clipboard`, `status`.

```json
{ "mode": "elements", "window": "Calculator" }
```

`act` runs up to 50 steps in one call and returns what changed: `click`, `type`, `key`, `mouse`, `set`, `focus`, `window`, `pick`, `wait`, `clipboard`, `notify`.

```json
{ "steps": [{ "do": "click", "target": "window:\"Calculator\" >> name:\"Seven\"" }, { "do": "key", "keys": "enter" }] }
```

Add `"background": true` to work behind your windows without using your mouse, keyboard or focus.

## Safety

- **Ctrl+Alt+Esc** stops all input at once. Press it again to resume.
- It waits while you are using the mouse or keyboard, and an overlay shows what it is doing.
- It never touches its own terminal, password managers, admin windows or UAC prompts, or anything on your deny list in `%APPDATA%\winpilot\config.toml`:

  ```toml
  [guard]
  deny = [{ process = "keepass.exe" }, { title = "Bank" }]
  deny_paths = ['D:\Private']
  ```

- Apps that host several agent terminals, like Agent Deck, can mark them protected: the agent can use the app's buttons but never another session's terminal.

## License

MIT
