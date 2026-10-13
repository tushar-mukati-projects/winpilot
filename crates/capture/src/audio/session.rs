use std::time::Instant;

use windows::Win32::Foundation::HANDLE;
use winpilot_core::Result;

use super::device::Endpoint;
use super::feed::{Catch, Feed};
use super::mixer::Mixer;
use super::pcm::AudioFormat;
use super::qpc::qpc_now_hns;
use super::source::AudioSource;

#[derive(Debug, Clone, Copy)]
pub(crate) struct Opened {
    pub(crate) format: AudioFormat,
    pub(crate) origin: Instant,
}

pub(crate) struct Session {
    system: Option<Feed>,
    mic: Option<Feed>,
    mixer: Mixer,
    opened: Opened,
}

impl Session {
    pub(crate) fn open(source: AudioSource) -> Result<Self> {
        let system = source.includes_system().then(|| Feed::open(Endpoint::Loopback)).transpose()?;
        let mut mic = source.includes_mic().then(|| Feed::open(Endpoint::Microphone)).transpose()?;
        let format = match (&system, &mic) {
            (Some(feed), _) | (None, Some(feed)) => feed.format(),
            (None, None) => AudioFormat { sample_rate: 48_000, channels: 2 },
        };
        if let Some(feed) = mic.as_mut() {
            feed.convert_to(format);
        }
        let mut session = Self { system, mic, mixer: Mixer::new(), opened: Opened { format, origin: Instant::now() } };
        session.start()?;
        Ok(session)
    }

    pub(crate) const fn opened(&self) -> Opened {
        self.opened
    }

    pub(crate) fn events(&self) -> Vec<HANDLE> {
        self.feeds().map(Feed::event).collect()
    }

    pub(crate) fn collect(&mut self, catch: Catch) -> Result<Vec<f32>> {
        let now = qpc_now_hns()?;
        let system = self.system.as_mut().map(|feed| feed.collect(now, catch)).transpose()?;
        let mic = self.mic.as_mut().map(|feed| feed.collect(now, catch)).transpose()?;
        Ok(match (system, mic) {
            (Some(system), Some(mic)) => self.mix(&system, &mic, catch),
            (Some(only), None) | (None, Some(only)) => only,
            (None, None) => Vec::new(),
        })
    }

    fn mix(&mut self, system: &[f32], mic: &[f32], catch: Catch) -> Vec<f32> {
        self.mixer.push(system, mic);
        match catch {
            Catch::Idle => self.mixer.mixed(),
            Catch::Flush => self.mixer.drained(),
        }
    }

    fn start(&mut self) -> Result<()> {
        let origin_hns = qpc_now_hns()?;
        self.opened.origin = Instant::now();
        for feed in [self.system.as_mut(), self.mic.as_mut()].into_iter().flatten() {
            feed.start(origin_hns)?;
        }
        Ok(())
    }

    fn feeds(&self) -> impl Iterator<Item = &Feed> {
        [self.system.as_ref(), self.mic.as_ref()].into_iter().flatten()
    }
}
