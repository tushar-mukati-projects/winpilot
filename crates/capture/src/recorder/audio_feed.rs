use std::sync::mpsc::TryRecvError;
use std::time::Instant;

use winpilot_core::Error;

use super::audio_backlog::{TrackBacklog, TrackSamples};
use super::audio_lead::{Lead, lead_for};
use crate::audio::{AudioCapture, AudioChunk, AudioFormat, Converter, STEREO_48_KHZ};

pub(crate) const TRACK_FORMAT: AudioFormat = STEREO_48_KHZ;

pub(crate) struct AudioFeed {
    capture: Option<AudioCapture>,
    origin: Instant,
    device: AudioFormat,
    lead: Option<Lead>,
    converter: Converter,
    backlog: TrackBacklog,
    note: Option<String>,
}

impl AudioFeed {
    pub(crate) fn new(capture: AudioCapture) -> Self {
        let device = capture.format();
        Self {
            origin: capture.origin(),
            capture: Some(capture),
            device,
            lead: None,
            converter: Converter::new(device, TRACK_FORMAT),
            backlog: TrackBacklog::default(),
            note: None,
        }
    }

    pub(crate) fn begin(&mut self, video_start: Instant) {
        self.lead = Some(lead_for(self.origin, video_start, self.device.sample_rate));
    }

    pub(crate) fn take(&mut self, limit: u64) -> Option<TrackSamples> {
        self.lead?;
        self.receive();
        if self.capture.is_none() {
            self.backlog.pad_to(limit);
        }
        self.backlog.split_off(limit)
    }

    pub(crate) fn finish(mut self, limit: u64) -> (Option<TrackSamples>, Option<String>) {
        if self.lead.is_none() {
            self.lead = Some(Lead::Skip(0));
        }
        if let Some(capture) = self.capture.take() {
            self.end_capture(capture);
        }
        self.backlog.pad_to(limit);
        (self.backlog.split_off(limit), self.note)
    }

    fn receive(&mut self) {
        while let Some(capture) = self.capture.as_ref() {
            match capture.chunks().try_recv() {
                Ok(chunk) => self.absorb(chunk),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    if let Some(capture) = self.capture.take() {
                        self.end_capture(capture);
                    }
                }
            }
        }
    }

    fn end_capture(&mut self, capture: AudioCapture) {
        let end = capture.stop();
        for chunk in end.chunks {
            self.absorb(chunk);
        }
        if let Some(error) = end.error {
            self.note_failure(&error);
        }
    }

    fn absorb(&mut self, chunk: AudioChunk) {
        let Some(lead) = self.lead.as_mut() else { return };
        let aligned = lead.apply(chunk.samples, self.device.channel_count());
        self.backlog.extend(&self.converter.convert(&aligned));
    }

    fn note_failure(&mut self, error: &Error) {
        let seconds = self.backlog.reached() as f64 / f64::from(TRACK_FORMAT.sample_rate);
        self.note = Some(format!("sound stopped at {seconds:.1} s: {}", error.message));
    }
}
