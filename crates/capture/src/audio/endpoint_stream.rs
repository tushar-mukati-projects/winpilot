use windows::Win32::Foundation::HANDLE;
use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, IAudioCaptureClient, IAudioClient,
};
use winpilot_core::Result;

use super::device::{Endpoint, activate_default_client, audio_error};
use super::event::AudioEvent;
use super::mix_format::{DeviceFormat, MixFormat};
use super::sample_format::append_f32;

const BUFFER_HNS: i64 = 2_000_000;
const MAX_PACKETS_PER_READ: usize = 256;

pub(crate) struct Packet {
    pub(crate) at_hns: Option<i64>,
    pub(crate) frames: u64,
    pub(crate) samples: Option<Vec<f32>>,
}

pub(crate) struct EndpointStream {
    endpoint: Endpoint,
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: AudioEvent,
    format: DeviceFormat,
    running: bool,
}

struct CapturedBuffer<'a> {
    capture: &'a IAudioCaptureClient,
    data: *mut u8,
    frames: u32,
    flags: u32,
    qpc: u64,
}

impl EndpointStream {
    pub(crate) fn open(endpoint: Endpoint) -> Result<Self> {
        let client = activate_default_client(endpoint)?;
        let mix = MixFormat::of(&client, endpoint)?;
        let format = mix.describe()?;
        let flags = match endpoint {
            Endpoint::Loopback => AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_LOOPBACK,
            Endpoint::Microphone => AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
        };
        unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, BUFFER_HNS, 0, mix.as_ptr(), None) }
            .map_err(|error| audio_error(endpoint, &error, "opening"))?;
        let event = AudioEvent::new()?;
        unsafe { client.SetEventHandle(event.handle()) }.map_err(|error| audio_error(endpoint, &error, "opening"))?;
        let capture = unsafe { client.GetService::<IAudioCaptureClient>() }
            .map_err(|error| audio_error(endpoint, &error, "opening"))?;
        Ok(Self { endpoint, client, capture, event, format, running: false })
    }

    pub(crate) const fn format(&self) -> DeviceFormat {
        self.format
    }

    pub(crate) const fn event(&self) -> HANDLE {
        self.event.handle()
    }

    pub(crate) fn start(&mut self) -> Result<()> {
        unsafe { self.client.Start() }.map_err(|error| audio_error(self.endpoint, &error, "starting"))?;
        self.running = true;
        Ok(())
    }

    pub(crate) fn read(&self, packets: &mut Vec<Packet>) -> Result<()> {
        for _ in 0..MAX_PACKETS_PER_READ {
            let waiting = unsafe { self.capture.GetNextPacketSize() }
                .map_err(|error| audio_error(self.endpoint, &error, "reading"))?;
            if waiting == 0 {
                return Ok(());
            }
            let buffer =
                CapturedBuffer::take(&self.capture).map_err(|error| audio_error(self.endpoint, &error, "reading"))?;
            packets.push(buffer.packet(self.format));
        }
        Ok(())
    }
}

impl Drop for EndpointStream {
    fn drop(&mut self) {
        if self.running {
            let _ = unsafe { self.client.Stop() };
        }
    }
}

impl<'a> CapturedBuffer<'a> {
    fn take(capture: &'a IAudioCaptureClient) -> windows_core::Result<Self> {
        let mut buffer = Self { capture, data: std::ptr::null_mut(), frames: 0, flags: 0, qpc: 0 };
        unsafe {
            capture.GetBuffer(
                &raw mut buffer.data,
                &raw mut buffer.frames,
                &raw mut buffer.flags,
                None,
                Some(&raw mut buffer.qpc),
            )
        }?;
        Ok(buffer)
    }

    fn packet(&self, format: DeviceFormat) -> Packet {
        let silent = self.flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0;
        let samples = (!silent && !self.data.is_null()).then(|| {
            let mut samples = Vec::new();
            append_f32(self.bytes(format.block_align), format.sample, &mut samples);
            samples
        });
        Packet { at_hns: i64::try_from(self.qpc).ok().filter(|at| *at > 0), frames: u64::from(self.frames), samples }
    }

    fn bytes(&self, block_align: usize) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data, self.frames as usize * block_align) }
    }
}

impl Drop for CapturedBuffer<'_> {
    fn drop(&mut self) {
        let _ = unsafe { self.capture.ReleaseBuffer(self.frames) };
    }
}
