# Deckhand

Deckhand is a fast MCP server, written in Rust, that lets AI agents such as Claude Code, Codex, Cursor or any other MCP client see and operate a Windows desktop.

It reads a window as a compact list of elements in one UI Automation round trip, acts on those elements by ref or selector, waits on real UI events instead of sleeping, and reports what changed after every batch. A typical window read takes 15–125 ms and the server starts in about 60 ms, against a median of 15 s per read for the Python server it replaces ([numbers](#compared-with-windows-mcp)).

```text
observe {}
window w2032546 notepad.exe "Untitled - Notepad" foreground 1280x800@200,120 · focus e5
e1 window "Untitled - Notepad"
  e2 menubar "Application"
    e3 menuitem "File"
  e5 edit "Text editor" focused

act {"steps":[{"do":"type","target":"e5","text":"hello"},{"do":"key","keys":"ctrl+s"}]}
✓ 1 type "hello" into e5 (keys)
✓ 2 key ctrl+s (keys)
after: window w2033540 notepad.exe "Save as" foreground …
```

## Features at a glance

- **Two tools.** `observe` sees the screen in any form: elements, full text, screenshots, recordings, window, monitor and app lists, the clipboard and status. `act` runs a batch of up to 50 steps and reports a diff.
- **Background mode.** Steps can run through UI Automation patterns only, without touching your mouse, keyboard or focus, including on windows on other virtual desktops.
- **Visible agent.** An animated overlay rings the window being driven and names the agent ("Claude is controlling"), with themes, custom colours and a demo mode for screen recorders.
- **Recording.** Record a window, element, region or monitor to MP4 (H.264) or GIF. Files stay playable even if something crashes.
- **Notifications.** The agent can send you a Windows toast from "Deckhand" when a long task finishes.
- **Safety rails.** A kill-switch hotkey, yielding to your own input, deny lists with blackout in screenshots and recordings, no control of elevated windows or the agent's own terminal, and an audit log.

## Requirements

- Windows 10 version 2004 or later, or Windows 11.
- Rust 1.88 or later with the MSVC toolchain.
- Visual Studio Build Tools with the Windows SDK. The build embeds the icon and version info with `rc.exe`.
- An MCP client: Claude Code (the install script uses the `claude` CLI) or any client that runs stdio servers.

## Install, update and uninstall

```powershell
git clone <this repository> deckhand
cd deckhand
.\scripts\install.ps1 -Register
```

The script:

1. Builds the release binary (`-SkipBuild` installs an existing build).
2. Copies it to `%LOCALAPPDATA%\Programs\deckhand\deckhand.exe` and checks that the copy's SHA-256 matches the build. On a mismatch it restores the previous version.
3. Registers the notification identity `Deckhand.Agent` (name and icon) under `HKCU\Software\Classes\AppUserModelId`, so notifications show as "Deckhand".
4. With `-Register`, registers the server with Claude Code at user scope: `claude mcp add -s user deckhand -- <exe> mcp`.

Restart open Claude Code sessions to load it. A session still running the previous build says so: its next call fails with "a newer Deckhand was installed …; restart your MCP client to load it".

**Updating.** Pull and run the script again. It works even while sessions are using the old build: the running copy is renamed to `deckhand.old-<time>.exe` and deleted on a later run.

**Uninstalling.** `.\scripts\install.ps1 -Uninstall` removes the MCP registration, the notification identity and the install folder. Your config (`%APPDATA%\deckhand`) and logs (`%LOCALAPPDATA%\deckhand`) stay unless you add `-Purge`.

### Other MCP clients

Deckhand is a plain stdio server: run `deckhand.exe mcp`. Most clients take a command and arguments. Here is the common `mcpServers` JSON shape used by Claude Desktop and Cursor; use the full path, because most clients don't expand `%LOCALAPPDATA%`:

```json
{
  "mcpServers": {
    "deckhand": {
      "command": "C:\\Users\\<you>\\AppData\\Local\\Programs\\deckhand\\deckhand.exe",
      "args": ["mcp"]
    }
  }
}
```

Other clients, such as Codex, take the same command and `args` in their own config format. Deckhand reads the client's name from the MCP handshake and shows it on the overlay.

## The two tools

### `observe`

`mode` picks what to see. The default is `elements`.

| Mode | Returns | Fields |
|---|---|---|
| `elements` | One line per element of a window, with refs | `window`, `query`, `all_windows`, `all`, `coords`, `max` |
| `text` | The full text of a window, element or region | `window` or `target` or `region`, `ocr`, `coords`, `max_chars` |
| `screenshot` | A caption, then a JPEG or PNG image | `target` or `region` or `monitor`, `preset`, `quality`, `max_width`, `format`, `marks` |
| `record` | Starts, stops or reports a screen recording | `action`, `target` or `region` or `monitor`, `format`, `fps`, `video_quality`, `max_width`, `max_seconds`, `cursor`, `path` |
| `windows` | The open app windows | none |
| `monitors` | Displays with position, scale and work area | none |
| `apps` | Installed apps that `window action=launch` can start | `query`, `max` |
| `clipboard` | The clipboard text | `max_chars` |
| `status` | Version, uptimes, kill switch and hotkey, user idle time, elevation, secure desktop, restarts, the last five timings | none |

| Field | Meaning |
|---|---|
| `window` | A window ref, title text, or a `window:` / `process:` selector. Default: the [default window](#targets-and-selectors). |
| `target` | An element or window, as a ref or selector. Screenshots and recordings capture that window, cropped to the element when one is given. |
| `query` | `elements`: a selector filter; matches are listed with their ancestors. `apps`: a name filter. |
| `all_windows` | `elements`: search every app window with `query`. Returns match lines with window and path. Needs `query`; can't be combined with `window`, `all` or `coords`. |
| `all` | Also list non-interactive elements such as static text and panes. |
| `coords` | `elements`: add `@x,y,wxh` screen rectangles. `text`: read with OCR and end each line with its `@x,y,wxh` screen rectangle (so `ocr: "never"` with `coords` is an error). |
| `max` | Cap on element lines (300), `all_windows` matches (20) or apps (30). |
| `region` | A screen rectangle `{"x","y","width","height"}` in physical pixels. A region can only be read as text with OCR, so `ocr: "never"` with a region is an error. |
| `monitor` | A monitor index: primary first, then by x, then by y, as `mode=monitors` lists them. |
| `ocr` | `auto` (default): UI Automation text, replaced by OCR when a window has under 40 readable characters and OCR finds more, or when an element has none. `never` or `always`. |
| `max_chars` | Text and clipboard limit, default 20000, clamped to 1–200000. A cut reply ends with `(truncated)`. |
| `preset` | Screenshot size and detail, see below. Default `medium`. |
| `quality` | Screenshot JPEG quality 1–100; overrides the preset. PNG is lossless, so it takes no `quality`. |
| `video_quality` | Recording quality `low`, `medium`, `high` (default) or `max`. |
| `max_width` | Output width limit. Screenshots 64–2576; recordings 160–3840 (default 1920, GIF 960). |
| `format` | Screenshot `jpeg` (default) or `png`. Recording `mp4` (default) or `gif`. |
| `marks` | Draw element ref labels on the screenshot. |
| `action` | Recording `start`, `stop` or `status`; required for `mode=record`. `stop` and `status` take no other fields. |
| `fps`, `max_seconds`, `cursor`, `path` | Override the [`[record]` config](#configuration) for one recording. |

Arguments are checked per mode before anything runs. A field from another mode is an `invalid_argument` error that says where it belongs, such as `fps does not apply to mode=screenshot; it is for mode=record`. Text, screenshots and recordings take at most one source (`window`/`target`/`region` for text, `target`/`region`/`monitor` for the others); with none, they use the default window. When `apps` is cut short, a first line says so: `showing 30 of 212 installed apps; narrow with query or raise max`. `status` answers even when the worker is down or the config file is invalid.

| Preset | Max width | JPEG quality |
|---|---|---|
| `low` | 960 | 60 |
| `medium` | 1280 | 80 |
| `high` | 1600 | 88 |
| `max` | native, up to 8000 px per side | 95, or lossless with `format: "png"` |

### `act`

```json
{ "steps": [ ... ], "return": "diff", "stop_on_error": true, "background": false }
```

| Field | Meaning |
|---|---|
| `steps` | Up to 50 steps, run in order. Each has a `do` field; unknown fields are rejected. |
| `return` | What to report afterwards: `diff` (default), `observe`, `screenshot` or `none`. |
| `stop_on_error` | Stop at the first failing step (default `true`). The kill switch always stops the batch. |
| `background` | Run without real input, see [Background mode](#background-mode-and-virtual-desktops). Default: `[act] background`, which is `false`. |

| `do` | Fields | Notes |
|---|---|---|
| `click` | `target`, `button?`, `count?` (1–3), `modifiers?`, `how?` (`auto`, `invoke`, `mouse`), `glide_ms?` | `auto` uses a UI Automation pattern on background windows and the real mouse otherwise. |
| `type` | `target?`, `text`, `enter?`, `how?` (`auto`, `keys`, `paste`) | Without a target it types into the focused app, never the agent's terminal. Printable text is refused when the focused element is known not to take text (a button, list item, tab, menu item, slider, checkbox, link, or anything whose value is read-only): `the focused element is button "X", not a text field; click the field first or give type a target`. Unknown or generic focus (a lone pane, a custom editor, a `contenteditable` region) is allowed, and `key` steps are never checked. A point or `ocr:` target is clicked to put the caret there. `auto` pastes text longer than 200 characters when the clipboard holds only text and the field's value can be read back to confirm the paste, then restores the clipboard unless something else changed it meanwhile. |
| `key` | `keys`, `target?`, `repeat?` (1–100), `action?` (`press`, `down`, `up`) | `keys` holds at most 200 chords. `down` and `up` hold and release one chord, such as shift while clicking, and take no `repeat`. |
| `mouse` | `action` (`move`, `down`, `up`, `scroll`, `drag`), `target?`, `to?`, `button?`, `direction?`, `amount?`, `glide_ms?` | `move` needs `target`. `down` and `up` default to the cursor position. `scroll` needs `direction`, defaults to the window centre and takes `amount` 1–50 (default 3). `drag` needs `target` (from) and `to`. `button` applies to `down`, `up` and `drag`. |
| `set` | `target` plus exactly one of `value`, `option`, `checked`, `expanded`, `selected` | Sets a text value, picks a combobox or list item, or sets a checkbox, expander or selection state; `selected: false` deselects. To flip a checkbox without naming the state, use `click`. |
| `focus` | `target` | An element gets keyboard focus; a window comes to the front. |
| `window` | `action` (`launch`, `close`, `minimize`, `maximize`, `restore`, `move`), `target?`, `app?`, `args?`, `rect?` | `launch` needs `app` (a Start-menu name like `Notepad`, an app id or an exe path) and waits for a new window of the launched process, a process it started, or, failing those, a new window whose program or title matches the app, then brings it to the front; shortcut paths (`.lnk`, `.url`) are refused, and with `deny_paths` set, so are absolute paths in `args` inside them. A Start-menu desktop app given `args` runs the program its entry points to with the entry's own arguments first (a Chrome profile shortcut keeps its profile), under the same `deny_paths` checks; an entry that starts no program file, such as File Explorer, refuses `args`. `move` needs `target` and `rect`; the others need `target`, which must be an app window, never the desktop or taskbar. A text `target` is a window spec, so titles containing colons work. |
| `pick` | `paths`, `open_with?`, `overwrite?`, `timeout_ms?` | Fills a native Open or Save dialog, optionally clicking `open_with` (such as an Upload button) first and waiting up to `timeout_ms` (15000, at most 60000) for the dialog. Several paths must share one folder. |
| `wait` | `until?` (`time`, `appears`, `disappears`, `window`, `window_opens`, `window_closed`, `text`, `idle`), `ms?`, `target?`, `text?`, `window?`, `timeout_ms?` | `until` defaults to `time`, a plain pause of `ms` (at most 5000). `window` is met as soon as a window matching `window` (title text or a `window:`/`process:` selector) is open, at once if one already is; `window_opens` counts only a matching window that wasn't open when the call began, such as a second Notepad; `window_closed` waits until none is open. Conditions are event-driven, with `timeout_ms` 5000 by default and at most 60000. `idle` waits for `ms` of quiet (300 by default), which must be shorter than the timeout. While keys or buttons are held down, a wait that could last over 1 s is refused. |
| `clipboard` | `text` | Sets the clipboard text. |
| `notify` | `title` (1–63), `message` (1–255) | Shows you a Windows notification. Lengths are in UTF-16 units, so an emoji counts as two. At most 3 per call, and notifications from separate calls must be 5 s apart (`busy` otherwise). |

Mouse steps glide the real cursor to the target first (`[cursor] glide_ms`, 120 ms by default; `glide_ms: 0` jumps; at most 3000). A glide or drag stops at once on the kill switch or when you touch the mouse or keyboard (`user_active`), and the spot is hit-checked again right before each press, so a window that moved over it is never clicked. Keys and buttons still held when the call ends are released, buttons first, with a note such as `released Shift left held by key down`; after a failed step a held button is released back where it was pressed. A pattern click or toggle that pulls its window to the front hands the foreground straight back, with a note. Each step line reports the method that actually ran, such as `(invoke)`, `(mouse)`, `(keys)`, `(paste)` or `(value)`. A notification that Windows suppresses still counts as a successful step, with a note that starts `not shown:`.

After the steps, `return: "diff"` compares the default window with how it was before: added (`+`), removed (`-`) and changed (`~`) elements, the focus, and windows that opened or closed. If the default window itself changed, for example because a dialog or a newly launched app took over, you get its full element list instead.

### Examples

Read the default window:

```text
observe {}
window w2032546 notepad.exe "Untitled - Notepad" foreground 1280x800@200,120 · focus e5
e1 window "Untitled - Notepad"
  e2 menubar "Application"
    e3 menuitem "File"
    e4 menuitem "Edit"
  e5 edit "Text editor" value="Shopping list" focused
  e9 statusbar "Status"
    e10 text "Ln 1, Col 14"
other windows: w1180342 chrome.exe "Inbox", w66320 explorer.exe "Downloads" desktop 2
```

Search every window without listing them:

```text
observe {"query":"role:button name~:save","all_windows":true}
e212 button "Save" · w2032546 notepad.exe · Untitled - Notepad > Save changes?
e1045 button "Save draft" · w1180342 chrome.exe · Inbox > New message
```

Take a detailed screenshot with ref labels:

```text
observe {"mode":"screenshot","target":"window:\"Paint\"","preset":"high","marks":true}
i4 1600x1000 jpeg · q88 · scale 1.60 · origin 0,0 · w329876 mspaint.exe "Untitled - Paint" maximized foreground (screen pixels) · click points as {"x":..,"y":..,"img":"i4"}
[image]
```

Record a window, then stop:

```text
observe {"mode":"record","action":"start","target":"window:\"Notepad\"","fps":15}
recording to …\Videos\Deckhand\deckhand-20261012-101500.mp4 · 1280x800 @ 15 fps · stop with observe mode=record action=stop

observe {"mode":"record","action":"stop"}
saved …\Videos\Deckhand\deckhand-20261012-101500.mp4 · 42.3 s · 635 frames · 3.8 MB
```

Launch Notepad, type, and save through the Save dialog in one call:

```json
{"steps":[
  {"do":"window","action":"launch","app":"Notepad"},
  {"do":"type","text":"Meeting notes\n- ship v2\n- update docs"},
  {"do":"key","keys":"ctrl+shift+s"},
  {"do":"wait","until":"window","window":"Save as"},
  {"do":"pick","paths":["D:\\Notes\\meeting.txt"]}
]}
```

```text
✓ 1 launch Notepad: launched w2033012 notepad.exe "Untitled - Notepad" foreground
✓ 2 type "Meeting notes\n- ship v2\n…" (keys)
✓ 3 key ctrl+shift+s (keys)
✓ 4 wait for window Save as: w2033540 notepad.exe "Save as" foreground is open
✓ 5 pick 1 file: picked 1 file in the Save dialog: D:\Notes\meeting.txt
after: window w2033012 notepad.exe "meeting.txt - Notepad" foreground 1280x800@240,160 · focus e5
e1 window "meeting.txt - Notepad"
  …
took 1840 ms
```

The default window changed (Notepad replaced whatever was in front), so the report is a full observation rather than a diff.

Work on a window in the background, without moving your mouse:

```json
{"background":true,"stop_on_error":false,"steps":[
  {"do":"click","target":"window:\"Build Settings\" >> role:button name:\"Refresh\""},
  {"do":"set","target":"window:\"Build Settings\" >> id:chkVerbose","checked":true},
  {"do":"type","target":"window:\"Build Settings\" >> name:\"Notes\"","text":" (nightly)"},
  {"do":"key","keys":"ctrl+s"}
]}
```

```text
✓ 1 click window:"Build Settings" >> role:button name:"Refresh" (invoke)
✓ 2 check window:"Build Settings" >> id:chkVerbose (toggle): now on
✓ 3 type " (nightly)" into window:"Build Settings" >> name:"Notes" (value): appended to the existing text; background mode can't place a caret
✗ 4 key ctrl+s — error[needs_user]: key needs the real keyboard; background mode never touches it (drop background or use set)
…
```

Drag one element onto another with a slower, visible glide:

```json
{"steps":[{"do":"mouse","action":"drag","target":"e41","to":"e57","glide_ms":300}]}
```

```text
✓ 1 drag e41 to e57 (mouse)
after: window w1180342 chrome.exe "Board" foreground · focus …
~ e57 listitem "Done" …
took 690 ms
```

Point targets from a screenshot work too: `"target":{"x":220,"y":340,"img":"i4"}`.

Hold shift while clicking to select a range. If the batch stops early, the held key is still released and the last step line says so:

```json
{"steps":[
  {"do":"key","keys":"shift","action":"down"},
  {"do":"click","target":"e12"},
  {"do":"click","target":"e19"},
  {"do":"key","keys":"shift","action":"up"}
]}
```

```text
✓ 1 key down shift (keys)
✓ 2 click e12 (mouse)
✓ 3 click e19 (mouse)
✓ 4 key up shift (keys)
after: …
```

## Targets and selectors

Every `target`, `to` and `open_with` takes one of four forms:

| Form | Example | Notes |
|---|---|---|
| Element ref | `e12` | From `observe`. Stays valid while the element exists. A stale ref is re-found by its fingerprint (window, role, name, AutomationId, class, tree path); if that's ambiguous you get the candidates, never a guess. |
| Window ref | `w2032546` | From window lines. |
| Point | `{"x":640,"y":360,"img":"i3"}` | Pixels in screenshot `i3`, mapped back through its scale and origin. Omit `img` for physical screen pixels. `set` acts on the element under the point; `type` clicks the point first. A point in a screenshot of one window may land on that app's own menus, flyouts, dropdowns or dialogs (same process, or owned by the window); any other window there, and always the agent's terminal or the taskbar, counts as covering it. |
| Selector | `role:button name:"Save"` | Lets the agent act without observing first when it already knows the app. |

Selector terms:

| Term | Matches |
|---|---|
| `role:button` | Control type. Also accepts aliases such as `link`/`a`, `textbox`/`input`, `dropdown`/`select`, `checkbox`/`check`, `dialog`. |
| `name:"Save as"` | The exact name, ignoring case and runs of whitespace. |
| `name~:save` | Name contains. |
| `id:1001` | AutomationId, exactly. |
| `class:Edit` | Class name, ignoring case. |
| `text:report` | Name or value contains. Bare words are the same: `Sign in` means `text:"Sign in"`. |
| `ocr:"Export"` | Text on screen, read by OCR from one window (the one `window:` or `process:` picks, else the default window). Matches a run of words on one line that contains the text, ignoring case and runs of whitespace; exact matches beat contains matches. The target is the matched words' rectangle, acted on at its centre like a point. Combines only with `window:`, `process:` and `nth:` (`nth:2` takes the second match); several matches without `nth:` are an `ambiguous` error listing up to five with their positions. Only `act` step targets take it: `observe`, the `window` step and `wait`'s `window` field read `ocr:…` as window title text. |
| `window:"Notepad"` | Window title contains. |
| `process:chrome` | Process name, with or without `.exe`. |
| `nth:2` | The second match, counting visible matches first. |
| `focused` | The focused element. |

- Quote values that contain spaces or colons: `name:"File name:"`. Inside quotes, escape `"` and `\` with a backslash.
- `>>` chains steps: each step searches inside the matches of the one before, for example `window:"Notepad" >> role:menuitem name:File`.
- When several elements match, an exact name beats a prefix, which beats a contains match; then visible beats hidden, enabled beats disabled, and actionable beats inert. A tie is an `ambiguous` error that lists up to five candidates; add `nth:`, a `role:`, or use a ref.
- A selector with `window:` or `process:` terms that matches in several windows prefers the one on the current virtual desktop, then the foreground one; if that still leaves several, the step fails with `ambiguous` and lists them, topmost first.
- A selector that matches nothing yet is waited for briefly (`[act] auto_wait_ms`, 2 s), so a step can target a button that is still appearing.

**Apps without UI Automation.** Some apps (CapCut, games, custom-drawn editors) expose only their window to UI Automation. For those, `observe` elements adds the window's OCR lines with their screen rectangles under the note `this app exposes no UI Automation; showing OCR text — target it with ocr:"…"`, and `observe` text with `coords: true` does the same for any window, element or region. Act on that text with `ocr:"…"` targets: `click`, `type`, `key`, `mouse` (`move`, `down`, `up`, `scroll`, `drag` from and to), `set` and `focus` (on the element under the text), `pick`'s `open_with`, and `wait` `appears`/`disappears` all take them. The capture hides the overlay and blacks out denied windows, as screenshots do. Within one `act` call the latest OCR reading is reused while its window keeps its place and raises no change events, until a step other than `wait` or `notify` runs; a target that is still being waited for is read again. `wait` `text`, and `appears`/`disappears` with plain text, fall back to OCR, at most every 400 ms, when the window has no UI Automation content; `ocr:` targets in a `wait` are read at most every 400 ms too.

**The default window.** When a call names no window, Deckhand uses the foreground app window. If that's the terminal hosting the agent, it uses the frontmost other app window that isn't minimized. The default window is always on the current virtual desktop.

**Keys.** `keys` is a chord such as `ctrl+s`, `alt+f4` or `ctrl+shift+esc`, or a sequence separated by spaces or commas, such as `ctrl+k ctrl+s`. A chord is any modifiers plus at most one key, so `shift` alone is valid for holding.

- Modifiers: `ctrl` (`control`), `alt`, `shift`, `win` (`windows`, `meta`).
- Named keys: `enter`, `esc`, `tab`, `space`, `backspace`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, `f1`–`f24`, `printscreen`, `pause`, `capslock`, `numlock`, `scrolllock`, `menu`, `volumeup`, `volumedown`, `mute`, `playpause`, `nexttrack`, `prevtrack`, `browserback`, `browserforward`.
- Any single character is a key. Use `plus`, `minus` and `comma` where the symbol would be ambiguous.

## Background mode and virtual desktops

Normally Deckhand acts like a careful person at the keyboard: it brings the window forward, glides the real mouse and types with real keystrokes. With `background: true` (per call) or `[act] background = true` (config), steps use only UI Automation patterns and never touch the real mouse, keyboard or focus.

| Step | In background mode |
|---|---|
| `click` | A plain left single click only (no `modifiers`, no `how: "mouse"`). Tries Invoke, then Toggle, SelectionItem, ExpandCollapse and the legacy default action. |
| `type` | Needs a `target`, no `enter`, and `how: "auto"`. The text is appended to the field's current value through `SetValue`, with the note "appended to the existing text; background mode can't place a caret". |
| `set` | UI Automation patterns only, with no typing or mouse fallback. |
| `mouse action=scroll` | Needs an element `target` and uses its scroll pattern. |
| `window` | Only `close`. `launch`, `minimize`, `maximize`, `restore` and `move` are refused. |
| `key`, `focus`, `pick`, `mouse` move/down/up/drag | Always refused with `needs_user`. |
| `wait`, `clipboard`, `notify` | Unchanged. |

Background steps don't wait for you to stop using the mouse and keyboard, because they never touch them. They still take the actor lock and still respect the kill switch, the deny lists and the elevation check, and they refuse a window that has a modal dialog open, just like real clicks do.

**Virtual desktops.** Window lines show `desktop N`, or `another desktop` when Windows doesn't say which, for windows on other virtual desktops; a hidden window whose desktop can't be looked up counts as on another desktop. The default window is always on the current desktop, and Deckhand never switches your desktop. Real input only reaches the current desktop, so steps aimed at a window on another desktop always get background behaviour; a step that can't be done that way fails with an error that names the desktop. A screenshot of such a window is printed from the window itself and fails, suggesting `mode=text`, when Windows hasn't drawn it. Recording a window on another desktop is refused, and the overlay draws nothing for those windows.

**Honest limits.**

- Web apps that ignore `SetValue`, such as some React-controlled inputs, need the real keyboard, so they can't be typed into in background mode.
- Full parallel control, where the agent has its own mouse and keyboard while you keep working, needs a separate Windows session: RDP loopback, a VM, or Microsoft's Agent Workspace preview. Windows 11 Home doesn't offer this out of the box.

## The overlay

While an agent drives a window you see:

- a flowing multi-colour glow around the window;
- a glass pill at the top naming the agent and the stop hotkey, such as "Claude is controlling · Ctrl+Alt+Esc to stop";
- a ripple at each click, and a name badge and short trail on the cursor as it glides;
- keycaps for named keys and shortcuts such as `Ctrl+S` or `Enter`, and a gradient outline on the field being typed into (typed text shows only as "typing…", never its characters or length);
- a brief viewfinder over the area a screenshot captured, and a faint sweep over windows the agent reads.

When you press the kill switch everything turns crimson and the pill says "Stopped · press Ctrl+Alt+Esc to resume", and it stays that way until you press the hotkey again to resume. While a recording runs, a crimson pill at the top right shows "Claude is recording" and the elapsed time.

The agent name comes from the MCP client: Claude, Codex, Cursor, Copilot, Windsurf and Gemini are recognised, other clients are shown by their own name, and `[overlay] label` overrides it.

The overlay never takes focus or receives clicks. It is hidden from Deckhand's own screenshots and OCR (in demo mode, if it can't be confirmed hidden in time, the screenshot caption or OCR text says it may appear), and it uses no CPU when idle (under 2% of one core while animating). It draws nothing for minimized windows or windows on other virtual desktops, and it doesn't replace the crimson "Stopped" state while the kill switch is on.

| Setting | Effect |
|---|---|
| `theme = "aurora"` | `aurora` (violet, indigo, sky, cyan, pink), `sunset` (orange, red, pink, purple), `ocean` (sky, cyan, teal, blue) or `mono` (white and greys). |
| `colors = ["#7C3AED", "#0EA5E9"]` | Your own looping palette instead of the theme. |
| `label = "Assistant"` | The name on the pill and cursor badge. |
| `animations = false` | Static versions of everything. Turning off Windows' animation effects does the same. |
| `show_reads = false` | No sweep over windows that are only read. |
| `capturable = true` | **Demo mode.** Your own screen recorders (OBS, Snipping Tool, Game Bar) record the overlay. Deckhand still hides it for the instant of its own captures. |
| `enabled = false` | No overlay at all. |

## Recording

`observe mode=record` records the default window, a window or element (cropped to it), a region or a monitor. One recording runs at a time.

- **Formats.** MP4 (H.264 through Media Foundation, on the hardware encoder when there is one, tagged BT.709) or GIF.
- **Defaults.** 30 fps (GIF at most 20), quality `high`, up to 300 s, cursor shown, scaled to at most 1920 px wide (GIF 960), saved as `deckhand-<date>-<time>.mp4` in `Videos\Deckhand`, wherever OneDrive or a policy moved your Videos folder.
- **Quality.** For MP4, `video_quality` sets the bitrate at 0.05, 0.08, 0.12 or 0.2 bits per pixel per frame, within 1–60 Mbps. For GIF it sets how hard the palette is fitted.
- **Constant frame rate.** When nothing changes, or a frame can't be captured in time, the previous frame is held. A resized window is scaled to the size chosen at the start, with black bars where the shape differs. A monitor that changes resolution or position is followed.
- **It stops by itself** at `max_seconds`, when you press the kill switch, when the recorded window closes or when the recorded monitor is disconnected. The next `status` or `stop` reports the saved file, and so does the next `start` if the report was never read.
- **Files are never overwritten.** A default name that is taken gets `-2`, `-3` and so on; an explicit `path` that exists is refused, as is a path under `deny_paths`.
- **Visible.** Windows draws a yellow border around whatever is being recorded, and the overlay shows the recording pill.
- **Crash-safe.** MP4s are written as fragmented MP4, so a file always plays up to its last fragment, under a second before the end. When the client ends the session, Deckhand gets a few seconds to finalize the file. Even if the worker has to be killed, the MP4 plays up to its last fragment; a GIF usually keeps the frames written so far.

## Notifications

The `notify` step shows a Windows toast from "Deckhand", with its icon. It stays in Notification Center. The step's note says when notifications are turned off for Deckhand, for you or by group policy (`not shown: …`, though the step still succeeds), and when Windows may hold the banner back: during a full-screen app, presentation mode, the lock screen, or the first hour after sign-in. Do Not Disturb can't be detected, so the note never claims you saw it.

Toasts need the app identity `Deckhand.Agent` that the install script registers. Without it, for example when running a dev build straight from `target`, Deckhand falls back to a tray balloon, which Windows may hold back.

## Safety

- **Kill switch.** `Ctrl+Alt+Esc` (configurable) stops all input from every Deckhand session at once; press it again to resume. It works even while Deckhand is holding a key such as Shift, because Deckhand also watches the physical keyboard for the chord. A running batch stops between steps and while waiting, and every `act` call fails with `stopped_by_user` until you resume, in background mode too. The agent cannot undo it and is told to report it rather than retry. Reading the screen keeps working. It also stops and saves any recording, and no new recording starts until you resume.
- **You keep priority.** If you used the mouse or keyboard in the last 600 ms, input steps wait until you stop, for up to 5 s, then fail with `user_active`. Reads and background steps never wait. Deckhand marks its own input so it never mistakes it for yours.
- **One actor at a time.** A lock shared by every Deckhand session stops two of them from driving the mouse at once; the second gets `busy`.
- **The agent's own terminal is off limits.** Deckhand finds the terminal window that hosts it and refuses to act on it; typing without a target never lands there.
- **Deny lists.** The only built-in rules are password managers: KeePass, KeePassXC, 1Password, Bitwarden, Dashlane and Enpass. Add your own by process, window title or both; they extend the built-ins. Denied windows are never read, captured or acted on, and `observe mode=clipboard` refuses clipboard content that a denied app put there or that its app marked private, as password managers do. Process rules are reliable. Title rules are partial protection: they match only what the title bar says, and titles change.
- **Blackout.** Screenshots and recordings paint denied windows black. Recordings check on every frame, and if the window list can't be read the whole frame is blacked out. A denied window can't be recorded, and a recorded window that becomes denied is blacked out while it stays denied.
- **Paths.** `pick` paths, recording paths, `launch` exe paths and absolute paths in `launch` arguments are refused under any folder in `deny_paths`, after resolving junctions and symbolic links.
- **Elevation and the secure desktop.** Windows running as administrator are refused with `elevated_target`. A UAC prompt or the lock screen returns `needs_user`.
- **It restores what it borrows.** Pasting restores your clipboard unless you copied something new in the meantime, and pasted text is kept out of clipboard history and cloud clipboard. Held keys and buttons are released at the end of every call; if a Deckhand process dies while holding them, the next worker to start, or the next call that acts, releases them.
- **Audit log.** `%LOCALAPPDATA%\deckhand\logs\YYYY-MM-DD.jsonl` keeps one line per call: time, tool (`observe` or `act`), a short summary (for `observe` it starts with `mode=…`; for `act` it lists the steps), result and duration. Typed and set text, key sequences, chosen list options, waited-for text, clipboard text and launch arguments are logged by length, and file and app paths by name only, unless you set `[log] log_text = true`; notification text is always logged by length, and a window, target or query that names a denied window is logged as `<denied window>`. Logs older than 14 days are deleted. No screenshots are saved.

**Recommended Claude Code permissions.** Allow `observe` and keep asking for `act` until you trust it, in `~/.claude/settings.json`:

```json
{
  "permissions": {
    "allow": ["mcp__deckhand__observe"],
    "ask": ["mcp__deckhand__act"]
  }
}
```

Don't combine desktop control with bypass-permissions mode. Note that `observe` can also start recordings, which write video files.

## Configuration

`%APPDATA%\deckhand\config.toml` is optional, and every key in it is optional. It is read when the server starts, so restart the session after editing it. Unknown keys are errors: while the file is invalid, every call except `observe mode=status` is refused with a message that names the line.

| Key | Default | Meaning |
|---|---|---|
| **`[guard]`** | | |
| `deny` | password managers | Windows never to read or touch: a list of `{ process = "...", title = "..." }`. A rule with both needs both. `process` ignores case and `.exe`; `title` is a case-insensitive contains match. Added to the built-ins. |
| `deny_paths` | `[]` | Absolute folders that `pick`, recording paths and `launch` (its exe path and the paths in its arguments) may not use. |
| `yield_ms` | `600` | How long you must be idle before an input step proceeds. |
| `yield_timeout_ms` | `5000` | How long an input step waits for you before failing with `user_active`. |
| `kill_hotkey` | `"ctrl+alt+escape"` | The kill-switch chord, in `keys` syntax. |
| **`[act]`** | | |
| `background` | `false` | Run `act` steps in background mode unless a call says otherwise. |
| `settle_quiet_ms` | `120` | After an input step, wait until the window has been quiet this long... |
| `settle_max_ms` | `1000` | ...but no longer than this. |
| `auto_wait_ms` | `2000` | How long a selector target that doesn't exist yet is waited for. |
| `max_elements` | `3000` | Element cap for one window read. |
| `snapshot_budget_ms` | `2500` | Time cap for one window read; the listing says when it was cut short. |
| **`[screenshot]`** | | |
| `preset` | `"medium"` | `low`, `medium`, `high` or `max`. |
| `max_width` | unset | Overrides the preset's width (64–2576). |
| `quality` | unset | Overrides the preset's JPEG quality (1–100). |
| **`[record]`** | | |
| `fps` | `30` | 1–60; GIF is capped at 20. |
| `quality` | `"high"` | `low`, `medium`, `high` or `max`. |
| `max_seconds` | `300` | Stop and save after this long, 1–1800. |
| `cursor` | `true` | Show the mouse pointer. |
| `format` | `"mp4"` | `mp4` or `gif`. |
| `max_width` | 1920 (GIF 960) | Larger sources are scaled down, keeping their shape. |
| `folder` | `Videos\Deckhand` | Where default-named recordings go; created if missing. |
| **`[overlay]`** | | |
| `enabled` | `true` | Show the overlay. |
| `animations` | `true` | Animate it; `false` or Windows' animation setting gives static versions. |
| `capturable` | `false` | Demo mode: let other screen recorders capture it. |
| `show_reads` | `true` | Sweep over windows the agent reads. |
| `theme` | `"aurora"` | `aurora`, `sunset`, `ocean` or `mono`. |
| `colors` | unset | A list of `#RRGGBB` colours that replaces the theme. |
| `label` | the client's name | The agent name on the pill and cursor badge. |
| **`[cursor]`** | | |
| `glide_ms` | `120` | Default cursor glide before mouse steps; `0` jumps. Steps can override it, up to 3000. |
| **`[log]`** | | |
| `log_text` | `false` | Log typed text, clipboard text and full paths in the audit log. |
| `retention_days` | `14` | Delete audit logs older than this. |

```toml
[guard]
deny = [
  { process = "MyVault.exe" },
  { process = "explorer.exe", title = "Private" },
]
deny_paths = ['D:\Private']

[act]
background = false

[screenshot]
preset = "medium"

[record]
fps = 30
format = "mp4"
folder = 'D:\Clips'

[overlay]
theme = "ocean"
# colors = ["#7C3AED", "#0EA5E9", "#F472B6"]
# label = "Assistant"
# capturable = true

[cursor]
glide_ms = 120

[log]
retention_days = 14
```

## Compared with Windows-MCP

Deckhand replaces [Windows-MCP](https://github.com/CursorTouch/Windows-MCP). Its tools map like this:

| Windows-MCP | Deckhand |
|---|---|
| Snapshot | `observe` |
| Screenshot | `observe mode=screenshot` |
| Click, Type, Shortcut | `act` steps `click`, `type`, `key` |
| Scroll, Move | `act` step `mouse` (`scroll`, `move`, `drag`) |
| Wait, WaitFor | `act` step `wait` |
| MultiSelect, MultiEdit | several steps in one `act` call (`click` with `modifiers`, `set`, `type`) |
| App | `act` step `window`, plus `observe` modes `windows` and `apps` |
| Clipboard | `observe mode=clipboard`, plus the `act` step `clipboard` |
| ControlStatus | `observe mode=status` |
| DisplayInventory | `observe mode=monitors` |
| Notification | `act` step `notify` |
| Scrape | `observe mode=text` on the browser window, plus the client's own web fetch |
| PowerShell, Registry, FileSystem, Process | Not included: coding agents have their own tools for these, governed by their own permissions |

Measured on the author's laptop at 2560×1600:

| Measure | Deckhand | Windows-MCP |
|---|---|---|
| Window read (`observe elements`) | 70–125 ms on Chrome; 15–40 ms on small windows | median 15 s |
| Screenshot | ~30 ms warm GDI capture, plus ~13 ms JPEG encode | — |
| Server start | ~60 ms | 5–13 s |
| Snapshot of a 2,100-element page | ~140 ms | — |
| Overlay CPU | under 2% of one core while animating; 0 idle | — |
| MP4 encode at 1920×1200@30 | 3.4 ms/frame on the hardware encoder | — |

The bigger win is fewer and smaller model turns: batches, diffs instead of full trees, selectors that skip the initial read, and no sleeps or blanket input guard. [docs/DESIGN.md](docs/DESIGN.md) has the analysis.

## Development

```powershell
.\scripts\check.ps1                    # fmt check, clippy -D warnings, tests
cargo test --workspace                 # safe while you work
cargo build -p deckhand-fixture        # the test app the e2e and fixture tests drive
cargo test -p deckhand --test e2e -- --ignored --test-threads=1
```

| Crate | Role |
|---|---|
| `deckhand-core` | Pure logic and the shared protocol: tool arguments and validation, selectors, keys, diff, output formatting, config, themes, agent names, OCR text matching, IPC framing. No Windows APIs. |
| `deckhand-uia` | UI Automation on one dedicated thread: snapshots, refs and fingerprints, pattern actions, text. |
| `deckhand-capture` | Screen and window capture (DXGI, GDI, PrintWindow), resize, JPEG/PNG encoding, marks, OCR, and recording to MP4 and GIF. |
| `deckhand-input` | SendInput mouse, keyboard and text, cursor glide, the user-activity monitor, the kill-switch hotkey. |
| `deckhand-desktop` | Windows, focus, launch, clipboard, monitors, virtual desktops, change events, the overlay, notifications. |
| `deckhand-worker` | The engine behind every observe mode and act step. |
| `deckhand` | The binary: MCP server, worker supervisor, audit log. |
| `deckhand-fixture` | A test-only Win32 app with known controls and file dialogs, plus a small library (`deckhand_fixture::FixtureProcess`) that the tests use to launch and close it. |

**Tests.** Plain `cargo test --workspace` is safe while you use the PC: it may read the desktop, but never injects input, moves the mouse, changes focus, writes the clipboard, opens windows or leaves files behind. Everything else is `#[ignore]`d with a reason, such as "moves the mouse and keyboard", "opens dialogs / changes focus", "records the screen", "shows the overlay", "writes the clipboard" or "shows a notification". Run those with `-- --ignored` only when you can leave the PC alone, and filter by test name to run one group. For example, the recording tests:

```powershell
cargo test --release -p deckhand-capture -p deckhand-worker record -- --ignored --test-threads=1
```

Each one writes into its own `%TEMP%\deckhand-record-test-<name>-<pid>-<n>` folder (under `DECKHAND_RECORD_TEST_DIR` when that is set) and removes the whole folder afterwards.

**The e2e test** starts `deckhand.exe mcp` with a throwaway config folder, speaks MCP to it over stdio and drives the fixture app with the real mouse and keyboard. Build the fixture first.

**The icon.** `assets/deckhand.png` and `assets/deckhand.ico` are drawn by `node assets/generate-icon.mjs`, which rewrites both files. `build.rs` embeds the icon, version info and manifest into `deckhand.exe`.

**Benchmark.** `bench/deskbench` is an optional, standalone, read-only benchmark of the raw techniques: window listing, UIA tree-fetch variants, capture, encoding, OCR and process start-up. It is outside the workspace:

```powershell
cargo run --release --manifest-path bench/deskbench/Cargo.toml
```

`--skip-uia`, `--skip-capture` and `--skip-startup` leave out sections, `--webview-process <exe>` adds a WebView2 app (by process name) to the UI Automation targets, and `--probe` is the child mode the start-up section times. Its privacy guard is on by default: it never prints window titles, it skips windows that match your Deckhand deny list (the built-in rules plus your config's) or a `--deny-title <text>` argument, and it skips the capture sections while such a window is visible.

**Demo video.** `demo/` scripts the promo recording. `node demo/choreography.mjs` drives `demo/demo.html`, a fictional page in Chrome or Edge, through an installed Deckhand (or the `deckhand.exe` that `DECKHAND_EXE` names) with a throwaway config and browser profile. It records a screen region with `observe mode=record`, uploads `demo/spec.pdf` through the `pick` step, and writes `demo/raw.mp4` and `demo/timeline.json`; `--check` rehearses without recording and `--keep` leaves the windows and scratch folder in place. `node demo/build.mjs` then needs `ffmpeg` on the `PATH`: it adds the title cards and captions and writes `demo/deckhand-demo.mp4`, `deckhand-demo.gif` and `poster.png` (`--raw`, `--timeline` and `--edit` pick other inputs, `--placeholder` builds from a stand-in clip, and `--skip-cards`, `--skip-gif` and `--keep-work` save time). Everything it writes is git-ignored.

The code rules (no comments, small functions, no `unwrap`, RAII handles, actor threads for COM) are in [CLAUDE.md](CLAUDE.md). The design is in [docs/DESIGN.md](docs/DESIGN.md).

## License

MIT. See [LICENSE](LICENSE).
