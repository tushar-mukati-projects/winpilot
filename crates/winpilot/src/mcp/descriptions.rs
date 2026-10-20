pub const OBSERVE: &str = "See the desktop. mode: elements (default: a window as element lines with refs like e12; \
query filters; all_windows with query searches every window), text (full text, UI Automation first, OCR fallback), \
screenshot (an image with an img id for point targets), record (start, stop or status an MP4 with sound, or a GIF), \
listen (transcribe PC sound, the mic or a file at path), windows, monitors, apps (installed; query filters), \
clipboard or status. window picks the window for elements and text; target picks an element or window for text, \
screenshot and record. Fields of other modes are errors.";
pub const ACT: &str = "Do things: up to 50 steps per call, each {\"do\":...}, then a diff (return: diff, observe, \
screenshot or none). Steps: click, type, key (press/down/up), mouse (move/down/up/scroll/drag), set \
(value/option/checked/expanded/selected), focus, window (launch/close/minimize/maximize/restore/move), pick (file \
dialogs), wait (ms, or until appears/disappears/window/window_opens/window_closed/text/idle), clipboard, notify. Targets: refs \
(e12, w123), selectors like role:button name:\"Save\" or ocr:\"Export\" (OCR), or \
{\"x\":..,\"y\":..,\"img\":\"i3\"}. background:true never uses the real mouse, keyboard or focus.";

pub const INSTRUCTIONS: &str = "WinPilot controls the Windows desktop through UI Automation with two tools, observe and act.\n\
- observe mode=elements (the default) lists a window as lines with refs (e12); mode=text reads it whole. Other modes: screenshot, record, windows, monitors, apps, clipboard, status.\n\
- act runs several steps per call by ref (e12) or selector (role:button name:\"Save\") and returns a diff, so observe again only when you need the full list.\n\
- When you know the app, act by selector without observing first; observe all_windows with a query finds elements in every window.\n\
- Use the pick step for Open, Save or upload dialogs, window action=launch to open apps, and wait instead of sleeping.\n\
- When observe says an app exposes no UI Automation, it lists OCR lines with positions; target that text with ocr:\"Export\" (add window:, process: or nth: as needed).\n\
- Take a screenshot only when text is not enough. Give a point in it with its img id so it maps back to the screen: {\"x\":640,\"y\":360,\"img\":\"i7\"}; without img, x and y are physical screen pixels.\n\
- observe mode=record saves an MP4 with the PC's sound by default (audio: system, mic, both or none); GIFs are silent.\n\
- observe mode=listen transcribes speech on this PC: max_seconds (default 10, at most 90) of system audio, the mic or both, or a recording or other media file given as path; language picks it (default: detected) and translate gives English.\n\
- act background:true works without the real mouse, keyboard or focus: UI Automation patterns first, and window messages for apps without them (clicks, drags, scrolls, typing and plain keys at a target); launches, window changes and file pickers stay behind the user's window; modifier chords still need the foreground. Windows on other virtual desktops always get that.\n\
- The user can stop all input with the kill hotkey ({hotkey}); act then fails with stopped_by_user. Tell the user instead of retrying.";
