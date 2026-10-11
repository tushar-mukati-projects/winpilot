use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use deckhand_core::{Context, Error, Rect, Result, WindowRef};

use crate::actor::{self, Reply, Request};
use crate::frame::Frame;

#[derive(Clone)]
pub struct Capturer {
    shared: Arc<Shared>,
}

struct Shared {
    requests: Option<Sender<Request>>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        drop(self.requests.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Capturer {
    pub fn spawn() -> Result<Self> {
        let (requests, inbox) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("deckhand-capture".to_string())
            .spawn(move || actor::run(&inbox, &ready))
            .context("starting the capture thread")?;
        let startup = started.recv().unwrap_or_else(|_| Err(Error::internal("capture thread stopped during startup")));
        if let Err(error) = startup {
            let _ = worker.join();
            return Err(error);
        }
        Ok(Self { shared: Arc::new(Shared { requests: Some(requests), worker: Some(worker) }) })
    }

    pub fn monitor_rects(&self) -> Result<Vec<Rect>> {
        self.call(Request::MonitorRects)
    }

    pub fn screen(&self, rect: Rect) -> Result<Frame> {
        self.call(|reply| Request::Screen(rect, reply))
    }

    pub fn monitor(&self, index: usize) -> Result<Frame> {
        self.call(|reply| Request::Monitor(index, reply))
    }

    pub fn window(&self, hwnd: WindowRef) -> Result<Frame> {
        self.call(|reply| Request::Window(hwnd, reply))
    }

    #[cfg(test)]
    fn output_modes(&self) -> Result<Vec<&'static str>> {
        self.call(Request::Modes)
    }

    fn call<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> Result<T> {
        let stopped = || Error::internal("capture thread has stopped");
        let (reply, response) = mpsc::channel();
        let requests = self.shared.requests.as_ref().ok_or_else(stopped)?;
        requests.send(request(reply)).map_err(|_| stopped())?;
        response.recv().map_err(|_| stopped())?
    }
}

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;
    use std::time::{Duration, Instant};

    use deckhand_core::{ErrorCode, Point, protocol::ImageFormat};
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows_core::w;

    use super::*;
    use crate::encode::{EncodeOptions, encode};

    fn shared() -> &'static Capturer {
        static CAPTURER: OnceLock<Capturer> = OnceLock::new();
        CAPTURER.get_or_init(|| Capturer::spawn().unwrap())
    }

    fn milliseconds(started: Instant) -> f64 {
        started.elapsed().as_secs_f64() * 1000.0
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn capturer_is_clone_send_and_sync() {
        assert_send_sync::<Capturer>();
        let clone = shared().clone();
        assert!(!clone.monitor_rects().unwrap().is_empty());
    }

    #[test]
    fn spawn_capture_and_drop_stops_the_thread() {
        let capturer = Capturer::spawn().unwrap();
        let copy = capturer.clone();
        let beyond = copy.monitor_rects().unwrap().iter().map(|rect| rect.bottom()).max().unwrap() + 50;
        let frame = copy.screen(Rect::new(0, beyond, 16, 16)).unwrap();
        assert_eq!((frame.width, frame.height), (16, 16));
        drop(capturer);
        assert!(copy.monitor_rects().is_ok());
        drop(copy);
    }

    #[test]
    fn primary_monitor_comes_first() {
        let rects = shared().monitor_rects().unwrap();
        assert!(!rects.is_empty());
        assert!(rects[0].contains(Point::new(0, 0)));
        assert!(rects.iter().all(|rect| !rect.is_empty()));
    }

    #[test]
    fn region_capture_has_the_requested_size() {
        let primary = shared().monitor_rects().unwrap()[0];
        let region = Rect::new(primary.x + 40, primary.y + 30, 200, 100);
        let frame = shared().screen(region).unwrap();
        assert_eq!(frame.origin, Point::new(region.x, region.y));
        assert_eq!((frame.width, frame.height, frame.bgra.len()), (200, 100, 200 * 100 * 4));
    }

    #[test]
    fn region_beyond_every_monitor_is_black() {
        let rects = shared().monitor_rects().unwrap();
        let far_right = rects.iter().map(|rect| rect.right()).max().unwrap();
        let frame = shared().screen(Rect::new(far_right + 100, 0, 20, 10)).unwrap();
        assert!(frame.bgra.iter().all(|&byte| byte == 0));
        let straddling = shared().screen(Rect::new(far_right - 10, 0, 20, 10)).unwrap();
        assert_eq!((straddling.width, straddling.height), (20, 10));
        assert!(straddling.crop(Rect::new(far_right, 0, 10, 10)).unwrap().bgra.iter().all(|&byte| byte == 0));
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let empty = shared().screen(Rect::new(0, 0, 0, 10)).unwrap_err();
        assert_eq!(empty.code, ErrorCode::InvalidArgument);
        let monitors = shared().monitor_rects().unwrap().len();
        let missing = shared().monitor(monitors).unwrap_err();
        assert_eq!(missing.code, ErrorCode::InvalidArgument);
        let gone = shared().window(WindowRef(0)).unwrap_err();
        assert_eq!(gone.code, ErrorCode::NotFound);
    }

    fn median_of_warm_captures(capturer: &Capturer, rect: Rect) -> (f64, f64) {
        let mut warm: Vec<f64> = (0..8)
            .map(|_| {
                let started = Instant::now();
                let frame = capturer.screen(rect).unwrap();
                assert_eq!(frame.rect(), rect);
                milliseconds(started)
            })
            .collect();
        warm.sort_by(f64::total_cmp);
        (warm[warm.len() / 2], warm[0])
    }

    fn primary_mode(capturer: &Capturer) -> &'static str {
        capturer.output_modes().unwrap().first().copied().unwrap_or("none")
    }

    fn wait_for_a_gpu_image(capturer: &Capturer, primary: Rect) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            capturer.screen(Rect::new(primary.x, primary.y, 1, 1)).unwrap();
            if primary_mode(capturer) == "dxgi" {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    #[test]
    fn warm_full_monitor_capture_and_encode() {
        let capturer = shared();
        let primary = capturer.monitor_rects().unwrap()[0];
        let started = Instant::now();
        let first = capturer.monitor(0).unwrap();
        let cold = milliseconds(started);
        assert_eq!(first.rect(), primary);
        let (median, best) = median_of_warm_captures(capturer, primary);
        let (width, height) = (primary.width, primary.height);
        eprintln!(
            "timing: {width}x{height} capture via {}: first {cold:.1} ms, warm median {median:.2} ms, best {best:.2} ms",
            primary_mode(capturer)
        );
        if wait_for_a_gpu_image(capturer, primary) {
            let (median, best) = median_of_warm_captures(capturer, primary);
            eprintln!(
                "timing: {width}x{height} capture via dxgi staging: warm median {median:.2} ms, best {best:.2} ms"
            );
        } else {
            eprintln!("timing: the screen did not change for 3 s, so the dxgi staging path was not timed");
        }
        let options = EncodeOptions { max_width: 1280, max_height: 1280, format: ImageFormat::Jpeg, quality: 75 };
        let started = Instant::now();
        let encoded = encode(&first, &options, &[]).unwrap();
        let encode_ms = milliseconds(started);
        let size = (encoded.width, encoded.height, encoded.bytes.len());
        eprintln!("timing: encode of the captured screen to {size:?} (w, h, bytes) JPEG q75: {encode_ms:.1} ms");
    }

    #[test]
    fn taskbar_window_has_its_visible_size() {
        let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        let mut bounds = RECT::default();
        let size = size_of::<RECT>() as u32;
        unsafe { DwmGetWindowAttribute(taskbar, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut bounds).cast(), size) }.unwrap();
        let frame = shared().window(WindowRef(taskbar.0 as i64)).unwrap();
        assert_eq!(frame.origin, Point::new(bounds.left, bounds.top));
        assert_eq!(frame.width as i32, bounds.right - bounds.left);
        assert_eq!(frame.height as i32, bounds.bottom - bounds.top);
    }
}
