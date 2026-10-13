use windows::Win32::Foundation::HANDLE;
use winpilot_core::Result;

use super::converter::Converter;
use super::device::Endpoint;
use super::endpoint_stream::{EndpointStream, Packet};
use super::pcm::AudioFormat;
use super::timeline::Timeline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Catch {
    Idle,
    Flush,
}

pub(crate) struct Feed {
    stream: EndpointStream,
    timeline: Timeline,
    converter: Option<Converter>,
    packets: Vec<Packet>,
}

impl Feed {
    pub(crate) fn open(endpoint: Endpoint) -> Result<Self> {
        let stream = EndpointStream::open(endpoint)?;
        let rate = stream.format().audio.sample_rate;
        Ok(Self { stream, timeline: Timeline::new(0, rate), converter: None, packets: Vec::new() })
    }

    pub(crate) const fn format(&self) -> AudioFormat {
        self.stream.format().audio
    }

    pub(crate) const fn event(&self) -> HANDLE {
        self.stream.event()
    }

    pub(crate) fn convert_to(&mut self, output: AudioFormat) {
        if output != self.format() {
            self.converter = Some(Converter::new(self.format(), output));
        }
    }

    pub(crate) fn start(&mut self, origin_hns: i64) -> Result<()> {
        self.timeline = Timeline::new(origin_hns, self.format().sample_rate);
        self.stream.start()
    }

    pub(crate) fn collect(&mut self, now_hns: i64, catch: Catch) -> Result<Vec<f32>> {
        self.stream.read(&mut self.packets)?;
        let channels = self.format().channel_count();
        let mut raw = Vec::new();
        for packet in self.packets.drain(..) {
            place(&mut self.timeline, packet, channels, &mut raw);
        }
        let silence = match catch {
            Catch::Idle => self.timeline.idle_fill(now_hns),
            Catch::Flush => self.timeline.flush(now_hns),
        };
        push_silence(&mut raw, silence, channels);
        Ok(match self.converter.as_mut() {
            Some(converter) => converter.convert(&raw),
            None => raw,
        })
    }
}

fn place(timeline: &mut Timeline, packet: Packet, channels: usize, raw: &mut Vec<f32>) {
    let placement = timeline.place(packet.at_hns, packet.frames);
    push_silence(raw, placement.silence, channels);
    let kept = packet.frames - placement.skip;
    match packet.samples {
        Some(samples) => raw.extend(samples.iter().skip(placement.skip as usize * channels)),
        None => push_silence(raw, kept, channels),
    }
}

fn push_silence(raw: &mut Vec<f32>, frames: u64, channels: usize) {
    raw.resize(raw.len() + frames as usize * channels, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placed_packets_keep_their_samples_and_gaps_become_zeros() {
        let mut timeline = Timeline::new(0, 1000);
        let mut raw = Vec::new();
        let first = Packet { at_hns: Some(0), frames: 2, samples: Some(vec![0.1, 0.2, 0.3, 0.4]) };
        place(&mut timeline, first, 2, &mut raw);
        let after_gap = Packet { at_hns: Some(1_000_000), frames: 1, samples: Some(vec![0.5, 0.6]) };
        place(&mut timeline, after_gap, 2, &mut raw);
        assert_eq!(raw.len(), 2 * 101);
        assert_eq!(&raw[..4], [0.1, 0.2, 0.3, 0.4]);
        assert!(raw[4..200].iter().all(|sample| *sample == 0.0));
        assert_eq!(&raw[200..], [0.5, 0.6]);
    }

    #[test]
    fn silent_packets_and_trimmed_heads_are_handled() {
        let mut timeline = Timeline::new(1_000_000, 1000);
        let mut raw = Vec::new();
        let early = Packet { at_hns: Some(0), frames: 150, samples: Some((0..150).map(|n| n as f32).collect()) };
        place(&mut timeline, early, 1, &mut raw);
        assert_eq!(raw, (100..150).map(|n| n as f32).collect::<Vec<_>>());
        raw.clear();
        place(&mut timeline, Packet { at_hns: None, frames: 3, samples: None }, 1, &mut raw);
        assert_eq!(raw, [0.0, 0.0, 0.0]);
    }
}
