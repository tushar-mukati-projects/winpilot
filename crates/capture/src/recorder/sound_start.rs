use super::outcome::AudioTrack;
use crate::audio::{AudioCapture, AudioSource};

pub(crate) fn start_sound(source: Option<AudioSource>) -> (Option<AudioCapture>, AudioTrack) {
    let Some(source) = source else { return (None, AudioTrack::default()) };
    match AudioCapture::start(source) {
        Ok(capture) => (Some(capture), AudioTrack { source: Some(source), note: None }),
        Err(error) if source == AudioSource::Both => system_only(&error.message),
        Err(error) => (None, without_sound(&error.message)),
    }
}

fn system_only(mic_problem: &str) -> (Option<AudioCapture>, AudioTrack) {
    match AudioCapture::start(AudioSource::System) {
        Ok(capture) => {
            (Some(capture), AudioTrack { source: Some(AudioSource::System), note: Some(mic_dropped(mic_problem)) })
        }
        Err(error) => (None, without_sound(&error.message)),
    }
}

fn mic_dropped(mic_problem: &str) -> String {
    format!("recorded the system sound only, because the mic could not be used: {mic_problem}")
}

pub(crate) fn without_sound(reason: &str) -> AudioTrack {
    AudioTrack { source: None, note: Some(format!("recorded without sound: {reason}")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_source_means_a_silent_recording_without_a_note() {
        let (capture, track) = start_sound(None);
        assert!(capture.is_none());
        assert_eq!(track, AudioTrack::default());
    }

    #[test]
    fn a_dropped_mic_is_explained() {
        assert_eq!(
            mic_dropped("access denied"),
            "recorded the system sound only, because the mic could not be used: access denied"
        );
    }

    #[test]
    fn a_missing_device_explains_the_silence() {
        let track = without_sound("no microphone is available");
        assert_eq!(track.source, None);
        assert_eq!(track.note.as_deref(), Some("recorded without sound: no microphone is available"));
    }
}
