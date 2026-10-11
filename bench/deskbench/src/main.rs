mod capture;
mod encode;
mod enumwin;
mod ocr;
mod uia;
mod util;

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};

use enumwin::WinInfo;
use util::{Frame, fmt_ms, median, min, ms_since};

const RUNS: usize = 5;
const BROWSERS: [&str; 4] = ["chrome.exe", "msedge.exe", "brave.exe", "firefox.exe"];

fn ft_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn ms_since_process_start() -> f64 {
    unsafe {
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return f64::NAN;
        }
        let now = ft_u64(GetSystemTimePreciseAsFileTime());
        now.saturating_sub(ft_u64(c)) as f64 / 10_000.0
    }
}

fn set_dpi_aware() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

fn probe() {
    let entry = ms_since_process_start();
    set_dpi_aware();
    match uia::probe_first_result() {
        Ok((init, first, _name_len, snap, count)) => {
            let total = ms_since_process_start();
            println!("PROBE create->main={entry:.1} main->uia_ready={init:.1} create->first_uia_result={:.1} create->interactive_snapshot={total:.1} snapshot_elements={count} (snapshot itself {:.1})", entry + first, snap - first);
        }
        Err(e) => println!("PROBE error {e}"),
    }
}

fn mentions_recovery(w: &WinInfo) -> bool {
    w.title.to_lowercase().contains("recovery")
}

fn pick<'a>(wins: &'a [WinInfo], f: impl Fn(&WinInfo) -> bool) -> Option<&'a WinInfo> {
    wins.iter().filter(|w| w.is_app_window() && f(w)).min_by_key(|w| (w.minimized, w.z))
}

fn print_capture_row(name: &str, cold: f64, warm: &[f64], note: &str) {
    println!("  {:<44} cold {:>8}   warm med {:>8}  min {:>8}  {}", name, fmt_ms(cold), fmt_ms(median(warm)), fmt_ms(if warm.is_empty() { f64::NAN } else { min(warm) }), note);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--probe") {
        probe();
        return;
    }
    let has = |s: &str| args.iter().any(|a| a == s);
    let main_entry = ms_since_process_start();
    set_dpi_aware();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    println!("deskbench: read-only desktop-control benchmark (process entry {main_entry:.1} ms after creation)");

    println!("\n[1] Window enumeration (EnumWindows, visible, title+class+process+rect+z), {RUNS} runs");
    let mut cold_cache = Vec::new();
    let mut warm_cache = Vec::new();
    let mut pid_cache = HashMap::new();
    let mut wins = Vec::new();
    for _ in 0..RUNS {
        let mut fresh = HashMap::new();
        let t = Instant::now();
        wins = enumwin::enumerate(&mut fresh);
        cold_cache.push(ms_since(t));
        let t = Instant::now();
        let w2 = enumwin::enumerate(&mut pid_cache);
        warm_cache.push(ms_since(t));
        std::hint::black_box(w2);
    }
    let app_count = wins.iter().filter(|w| w.is_app_window()).count();
    println!("  visible top-level windows: {}   app-like (titled, uncloaked, non-tool, non-zero): {}", wins.len(), app_count);
    println!("  fresh pid->name cache: median {} ms (min {})   warm pid cache: median {} ms (min {})", fmt_ms(median(&cold_cache)), fmt_ms(min(&cold_cache)), fmt_ms(median(&warm_cache)), fmt_ms(min(&warm_cache)));
    println!("  app-like windows (z, process, class, flags), titles intentionally not printed:");
    for w in wins.iter().filter(|w| w.is_app_window()) {
        println!("    z={:<4} {:<28} {:<36} {}{}", w.z, w.process, w.class, if w.minimized { "min " } else { "" }, if mentions_recovery(w) { "[recovery-guard]" } else { "" });
    }

    let fg = enumwin::foreground();
    let fg_info = wins.iter().find(|w| w.hwnd == fg.0 as isize).cloned();
    let recovery_on_screen = wins.iter().any(|w| w.is_app_window() && !w.minimized && mentions_recovery(w));
    let mut targets: Vec<uia::Target> = Vec::new();
    let mut add = |label: &str, w: Option<&WinInfo>| {
        if let Some(w) = w {
            if mentions_recovery(w) {
                println!("  skipping {label} target: window title mentions 'recovery' (privacy guard)");
                return;
            }
            if targets.iter().any(|t| t.hwnd == w.hwnd) {
                println!("  {label} target is the same window as an earlier target; reusing those numbers");
                return;
            }
            targets.push(uia::Target { label: label.into(), hwnd: w.hwnd, process: w.process.clone(), class: w.class.clone(), center: ((w.rect.left + w.rect.right) / 2, (w.rect.top + w.rect.bottom) / 2) });
        } else {
            println!("  no {label} window found");
        }
    };
    add("foreground", fg_info.as_ref());
    add("chrome", pick(&wins, |w| w.class == "Chrome_WidgetWin_1" && w.process.eq_ignore_ascii_case("chrome.exe")));
    add("explorer", pick(&wins, |w| w.class == "CabinetWClass" && w.process.eq_ignore_ascii_case("explorer.exe")));
    add("electron", pick(&wins, |w| w.class == "Chrome_WidgetWin_1" && !BROWSERS.iter().any(|b| w.process.eq_ignore_ascii_case(b))));
    add("webview2/tauri", pick(&wins, |w| w.process.to_lowercase().contains("claude-deck") || w.process.to_lowercase().contains("claude deck")));
    add("taskbar (explorer.exe stand-in)", wins.iter().find(|w| w.class == "Shell_TrayWnd"));

    if !has("--skip-uia") {
        println!("\n[2] UI Automation (raw `windows` crate, CUIAutomation8 on a COINIT_MULTITHREADED thread), {RUNS} runs per variant");
        let t = Instant::now();
        match uia::run_all(targets.clone(), RUNS) {
            Ok(rep) => {
                println!("  CoCreateInstance(CUIAutomation8): {} ms; IUIAutomation2 timeouts set (connection 2 s, transaction 20 s): {}", fmt_ms(rep.create_ms), rep.timeouts_set);
                for r in &rep.results {
                    println!("  target {} ({} / {})", r.target.label, r.target.process, r.target.class);
                    match &r.cold_err {
                        None => println!("    first-touch interactive snapshot (cold): {} ms, {} elements", fmt_ms(r.cold_ms), r.cold_summary.count),
                        Some(e) => println!("    first-touch interactive snapshot (cold): failed after {} ms: {e}", fmt_ms(r.cold_ms)),
                    }
                    for s in &r.stats {
                        match &s.err {
                            Some(e) => println!("    {:<38} error: {e}", s.name),
                            None => println!(
                                "    {:<38} elements {:>6}  median {:>8} ms  min {:>8}  runs {}  on-screen {:>5}  interactive-on-screen {:>5}  invokable {:>5}  text chars {:>7}{}",
                                s.name,
                                s.summary.count,
                                fmt_ms(median(&s.runs)),
                                fmt_ms(min(&s.runs)),
                                s.runs.len(),
                                s.summary.onscreen,
                                s.summary.interactive_onscreen,
                                s.summary.invokable,
                                s.summary.text_chars,
                                if s.summary.truncated { "  TRUNCATED" } else { "" }
                            ),
                        }
                    }
                }
            }
            Err(e) => println!("  UIA failed: {e}"),
        }
        println!("  UIA section wall time: {:.1} s", t.elapsed().as_secs_f64());
    }

    let mut frame_for_encode: Option<(String, Frame)> = None;
    let mut dxgi_keep: Option<capture::Dupl> = None;
    if recovery_on_screen {
        println!("\n[3-5] skipped: a visible window title mentions 'recovery' (privacy guard)");
    } else if !has("--skip-capture") {
        let mon = capture::primary_monitor();
        println!("\n[3] Screen capture, primary monitor {}x{} (physical px); frames kept in memory only, never written to disk", mon.width(), mon.height());

        let t = Instant::now();
        let gdi = capture::gdi_capture(&mon);
        let cold = ms_since(t);
        match gdi {
            Ok(f) => {
                let mut warm = Vec::new();
                for _ in 0..RUNS {
                    let t = Instant::now();
                    let f2 = capture::gdi_capture(&mon).unwrap();
                    warm.push(ms_since(t));
                    std::hint::black_box(f2);
                }
                print_capture_row("GDI BitBlt (DC+DIB+blit+copy per call)", cold, &warm, &format!("{}x{} non-black {:.0}%", f.width, f.height, f.non_black_pct()));
                frame_for_encode.get_or_insert(("GDI".into(), f));
            }
            Err(e) => println!("  GDI BitBlt failed: {}", util::err_str(&e)),
        }

        let t = Instant::now();
        let dupl = capture::dxgi_open(&mon);
        let open_ms = ms_since(t);
        match dupl {
            Ok(mut d) => match capture::dxgi_grab(&mut d, 1000) {
                Ok((f, _)) => {
                    let cold = ms_since(t);
                    let (acq, ptr_only) = (d.acquires, d.zero_lpt);
                    let mut warm = Vec::new();
                    let mut fresh = 0;
                    for _ in 0..RUNS {
                        let t = Instant::now();
                        let (f2, new) = capture::dxgi_grab(&mut d, 0).unwrap();
                        warm.push(ms_since(t));
                        fresh += new as usize;
                        std::hint::black_box(f2);
                    }
                    print_capture_row("DXGI Desktop Duplication (raw)", cold, &warm, &format!("open {} ms, {} via {}, {} acquires ({} pointer-only) before first image, {fresh}/{RUNS} warm grabs had a new frame, non-black {:.0}%", fmt_ms(open_ms), d.api, d.adapter_name, acq, ptr_only, f.non_black_pct()));
                    if f.non_black_pct() > 1.0 {
                        frame_for_encode = Some(("DXGI".into(), f));
                    }
                    dxgi_keep = Some(d);
                }
                Err(e) => println!("  DXGI first frame failed: {}", util::err_str(&e)),
            },
            Err(e) => println!("  DXGI Desktop Duplication open failed after {} ms: {}", fmt_ms(open_ms), util::err_str(&e)),
        }

        let t = Instant::now();
        let wgc_res = capture::d3d_default().and_then(|d3d| {
            let dev_ms = ms_since(t);
            let mut w = capture::wgc_start(&d3d, &mon)?;
            let (f, _) = capture::wgc_grab(&d3d, &mut w, Duration::from_secs(2))?;
            let cold = ms_since(t);
            let mut persist = Vec::new();
            let mut fresh = 0;
            for _ in 0..RUNS {
                let t = Instant::now();
                let (f2, new) = capture::wgc_grab(&d3d, &mut w, Duration::ZERO)?;
                persist.push(ms_since(t));
                fresh += new as usize;
                std::hint::black_box(f2);
            }
            let border_off = w.border_off;
            drop(w);
            let mut per_call = Vec::new();
            for _ in 0..RUNS {
                let t = Instant::now();
                let mut w = capture::wgc_start(&d3d, &mon)?;
                let (f2, _) = capture::wgc_grab(&d3d, &mut w, Duration::from_secs(2))?;
                per_call.push(ms_since(t));
                std::hint::black_box(f2);
            }
            Ok((f, dev_ms, cold, persist, fresh, per_call, border_off))
        });
        match wgc_res {
            Ok((f, dev_ms, cold, persist, fresh, per_call, border_off)) => {
                print_capture_row("WGC raw, persistent session (latest frame)", cold, &persist, &format!("device {} ms, {fresh}/{RUNS} warm grabs had a new frame, border disabled: {border_off}, non-black {:.0}%", fmt_ms(dev_ms), f.non_black_pct()));
                print_capture_row("WGC raw, new session per call (device reused)", f64::NAN, &per_call, "item+pool+session+first frame each call");
                if f.non_black_pct() > 1.0 && frame_for_encode.as_ref().map(|(n, _)| n != "DXGI").unwrap_or(true) {
                    frame_for_encode = Some(("WGC".into(), f));
                }
            }
            Err(e) => println!("  WGC raw failed: {}", util::err_str(&e)),
        }

        let mut wc = Vec::new();
        let mut wc_err = None;
        let mut wc_size = (0, 0);
        for _ in 0..=RUNS {
            let t = Instant::now();
            match std::thread::spawn(capture::windows_capture_wgc).join().unwrap() {
                Ok(f) => {
                    wc.push(ms_since(t));
                    wc_size = (f.width, f.height);
                }
                Err(e) => {
                    wc_err = Some(e);
                    break;
                }
            }
        }
        match wc_err {
            Some(e) => println!("  windows-capture WGC failed: {e}"),
            None => print_capture_row("windows-capture 2.0.1 WGC one-shot per call", wc[0], &wc[1..], &format!("{}x{}, own thread+device+session per call", wc_size.0, wc_size.1)),
        }

        let had_dxgi = dxgi_keep.take().is_some();
        let t = Instant::now();
        match capture::windows_capture_dxgi_open().and_then(|mut d| capture::windows_capture_dxgi_grab(&mut d, 1000).map(|f| (d, f))) {
            Ok((mut d, f)) => {
                let cold = ms_since(t);
                let mut warm = Vec::new();
                let mut timeouts = 0;
                for _ in 0..RUNS {
                    let t = Instant::now();
                    match capture::windows_capture_dxgi_grab(&mut d, 0) {
                        Ok(_) => warm.push(ms_since(t)),
                        Err(_) => timeouts += 1,
                    }
                }
                print_capture_row("windows-capture 2.0.1 DXGI duplication", cold, &warm, &format!("{}x{}, {timeouts}/{RUNS} warm grabs timed out (no new frame, no cached copy), first frame non-black {:.0}%", f.width, f.height, f.non_black_pct()));
            }
            Err(e) => println!("  windows-capture DXGI failed after {} ms: {e}", fmt_ms(ms_since(t))),
        }
        if had_dxgi {
            dxgi_keep = capture::dxgi_open(&mon).ok().and_then(|mut d| capture::dxgi_grab(&mut d, 1000).ok().map(|_| d));
        }

        let t = Instant::now();
        match capture::xcap_capture() {
            Ok(f) => {
                let cold = ms_since(t);
                let mut warm = Vec::new();
                for _ in 0..RUNS {
                    let t = Instant::now();
                    let f2 = capture::xcap_capture().unwrap();
                    warm.push(ms_since(t));
                    std::hint::black_box(f2);
                }
                print_capture_row("xcap 0.9.8 Monitor::capture_image (GDI)", cold, &warm, &format!("{}x{}, includes Monitor::all() + RGBA convert", f.width, f.height));
            }
            Err(e) => println!("  xcap failed: {e}"),
        }
    }

    if let Some((src, mut frame)) = frame_for_encode {
        frame.force_opaque();
        println!("\n[4] Downscale + encode (source frame: {src} {}x{}), {RUNS} runs median", frame.width, frame.height);
        let (rows, small) = encode::bench(&frame, RUNS);
        for r in &rows {
            println!("  {:<62} {:>8} ms  {:>9} bytes", r.name, fmt_ms(r.ms), r.bytes);
        }

        if let Some(d) = dxgi_keep.as_mut() {
            let mut resizer = fast_image_resize::Resizer::new();
            let mut v = Vec::new();
            let mut bytes = 0;
            for _ in 0..RUNS {
                let t = Instant::now();
                let (mut f, _) = capture::dxgi_grab(d, 0).unwrap();
                let (dw, dh) = encode::target_size(f.width, f.height, 1568);
                let small = encode::resize_fir_borrowed(&mut resizer, &mut f, dw, dh, fast_image_resize::ResizeAlg::Convolution(fast_image_resize::FilterType::Bilinear));
                let mut buf = Vec::with_capacity(256 * 1024);
                jpeg_encoder::Encoder::new(&mut buf, 75).encode(&small.bgra, dw as u16, dh as u16, jpeg_encoder::ColorType::Bgra).unwrap();
                bytes = buf.len();
                v.push(ms_since(t));
            }
            println!("  {:<62} {:>8} ms  {:>9} bytes", "PIPELINE warm DXGI grab + resize + JPEG q75 (end to end)", fmt_ms(median(&v)), bytes);
        }

        println!("\n[5] OCR (Windows.Media.Ocr); recognised text is never printed or stored");
        let frames: Vec<(&str, &Frame)> = vec![("downscaled", &small), ("full-res", &frame)];
        match ocr::bench(&frames, RUNS) {
            Ok(rep) => {
                println!("  recognizer languages: {:?}; MaxImageDimension {}; TryCreateFromUserProfileLanguages {} ms", rep.languages, rep.max_dim, fmt_ms(rep.engine_ms));
                for r in &rep.rows {
                    println!("  {:<40} first {:>8} ms  warm med {:>8} ms  (bitmap copy {} ms)  lines {:>4}  words {:>5}", r.name, fmt_ms(r.cold_ms), fmt_ms(r.warm_ms), fmt_ms(r.bitmap_ms), r.lines, r.words);
                }
            }
            Err(e) => println!("  OCR unavailable: {}", util::err_str(&e)),
        }
    }

    if !has("--skip-startup") {
        println!("\n[6] Process cold start to first UIA result ({RUNS} child launches of this binary with --probe)");
        let exe = std::env::current_exe().unwrap();
        let mut walls = Vec::new();
        let mut walls_snap = Vec::new();
        for i in 0..RUNS {
            let t = Instant::now();
            let child = Command::new(&exe).arg("--probe").stdout(Stdio::piped()).stderr(Stdio::null()).creation_flags(0x0800_0000).spawn();
            let Ok(mut child) = child else {
                println!("  spawn failed");
                break;
            };
            let mut line = String::new();
            let _ = BufReader::new(child.stdout.take().unwrap()).read_line(&mut line);
            let ready = ms_since(t);
            let _ = child.wait();
            let exited = ms_since(t);
            let line = line.trim().to_string();
            println!("  run {}: parent saw result after {} ms (process exit {} ms) | child: {}", i + 1, fmt_ms(ready), fmt_ms(exited), line.trim_start_matches("PROBE "));
            walls.push(ready);
            if let Some(v) = line.split_whitespace().find_map(|kv| kv.strip_prefix("create->first_uia_result=")) {
                walls_snap.push(v.parse::<f64>().unwrap_or(f64::NAN));
            }
        }
        println!("  spawn -> interactive snapshot on stdout: first {} ms, median {} ms; process create -> first UIA result: median {} ms", fmt_ms(walls.first().copied().unwrap_or(f64::NAN)), fmt_ms(median(&walls)), fmt_ms(median(&walls_snap)));
    }
    println!("\ndone, total {:.1} s", ms_since_process_start() / 1000.0);
}
