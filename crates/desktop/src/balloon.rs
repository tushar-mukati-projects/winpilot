use std::time::Duration;

use deckhand_core::{Error, Result};
use windows::Win32::UI::WindowsAndMessaging::MSG;

use crate::{
    balloon_text::BalloonText,
    balloon_window::{BALLOON_CALLBACK, BalloonWindow},
    message_thread::ThreadMessages,
    startup_cancel::StartupCancel,
    timeouts::finite_millis,
    tray_icon::TrayIcon,
    window_timer::WindowTimer,
};

pub(crate) const BALLOON_LIFETIME: Duration = Duration::from_secs(10);
const LIFETIME_TIMER: usize = 1;

#[derive(Debug)]
pub(crate) struct BalloonLifetime {
    _timer: WindowTimer,
    window: BalloonWindow,
}

impl BalloonLifetime {
    pub(crate) fn start(lifetime: Duration) -> Result<Self> {
        let window = BalloonWindow::create()?;
        let timer = WindowTimer::start(window.hwnd(), LIFETIME_TIMER, finite_millis(lifetime))
            .ok_or_else(|| Error::internal("could not start the notification's lifetime timer"))?;
        Ok(Self { _timer: timer, window })
    }
}

#[derive(Debug)]
pub(crate) struct Balloon {
    _icon: TrayIcon,
    _lifetime: BalloonLifetime,
}

impl Balloon {
    pub(crate) fn show(text: &BalloonText, cancel: &StartupCancel) -> Result<Self> {
        let lifetime = BalloonLifetime::start(BALLOON_LIFETIME)?;
        cancel.check()?;
        let icon = TrayIcon::add(lifetime.window.hwnd(), BALLOON_CALLBACK)?;
        cancel.check()?;
        icon.show_balloon(text)?;
        Ok(Self { _icon: icon, _lifetime: lifetime })
    }
}

impl ThreadMessages for Balloon {
    fn on_message(&mut self, _message: &MSG) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::message_thread::MessageThread;

    struct Lifetime {
        _lifetime: BalloonLifetime,
    }

    impl ThreadMessages for Lifetime {
        fn on_message(&mut self, _message: &MSG) -> bool {
            false
        }
    }

    fn finishes_within(thread: &MessageThread, limit: Duration) -> bool {
        let started = Instant::now();
        while !thread.is_finished() {
            if started.elapsed() > limit {
                return false;
            }
            thread::sleep(Duration::from_millis(5));
        }
        true
    }

    #[test]
    fn the_lifetime_timer_ends_the_thread_by_itself() {
        let start =
            || BalloonLifetime::start(Duration::from_millis(20)).map(|lifetime| (Lifetime { _lifetime: lifetime }, ()));
        let (thread, ()) = MessageThread::spawn("test-balloon-lifetime", start).unwrap();
        assert!(finishes_within(&thread, Duration::from_secs(5)));
    }

    #[test]
    fn dropping_the_owner_ends_a_long_lifetime_at_once() {
        let start = || BalloonLifetime::start(BALLOON_LIFETIME).map(|lifetime| (Lifetime { _lifetime: lifetime }, ()));
        let (thread, ()) = MessageThread::spawn("test-balloon-dismiss", start).unwrap();
        assert!(!thread.is_finished());
        let started = Instant::now();
        drop(thread);
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
