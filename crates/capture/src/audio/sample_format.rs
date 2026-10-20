use windows::core::GUID;

const WAVE_FORMAT_PCM: u32 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u32 = 3;
const KSDATAFORMAT_BASE: GUID = GUID::from_u128(0x0000_0000_0000_0010_8000_00aa_0038_9b71);
const I16_SCALE: f32 = 32_768.0;
const I24_SCALE: f32 = 8_388_608.0;
const I32_SCALE: f32 = 2_147_483_648.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleFormat {
    F32,
    I16,
    I24,
    I32,
}

impl SampleFormat {
    pub(crate) fn identify(tag: u32, container_bits: u16) -> Option<Self> {
        match (tag, container_bits) {
            (WAVE_FORMAT_IEEE_FLOAT, 32) => Some(Self::F32),
            (WAVE_FORMAT_PCM, 16) => Some(Self::I16),
            (WAVE_FORMAT_PCM, 24) => Some(Self::I24),
            (WAVE_FORMAT_PCM, 32) => Some(Self::I32),
            _ => None,
        }
    }

    pub(crate) const fn bytes(self) -> usize {
        match self {
            Self::I16 => 2,
            Self::I24 => 3,
            Self::F32 | Self::I32 => 4,
        }
    }
}

pub(crate) fn tag_of_subformat(subformat: GUID) -> Option<u32> {
    let base = GUID { data1: 0, ..subformat };
    (base == KSDATAFORMAT_BASE).then_some(subformat.data1)
}

pub(crate) fn append_f32(bytes: &[u8], format: SampleFormat, output: &mut Vec<f32>) {
    let width = format.bytes();
    output.reserve(bytes.len() / width);
    for sample in bytes.chunks_exact(width) {
        output.push(decode(sample, format));
    }
}

fn decode(sample: &[u8], format: SampleFormat) -> f32 {
    match *sample {
        [a, b] if format == SampleFormat::I16 => f32::from(i16::from_le_bytes([a, b])) / I16_SCALE,
        [a, b, c] => (i32::from_le_bytes([0, a, b, c]) >> 8) as f32 / I24_SCALE,
        [a, b, c, d] if format == SampleFormat::F32 => f32::from_le_bytes([a, b, c, d]),
        [a, b, c, d] => i32::from_le_bytes([a, b, c, d]) as f32 / I32_SCALE,
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_formats_are_identified_by_tag_and_container_size() {
        assert_eq!(SampleFormat::identify(3, 32), Some(SampleFormat::F32));
        assert_eq!(SampleFormat::identify(1, 16), Some(SampleFormat::I16));
        assert_eq!(SampleFormat::identify(1, 24), Some(SampleFormat::I24));
        assert_eq!(SampleFormat::identify(1, 32), Some(SampleFormat::I32));
        assert_eq!(SampleFormat::identify(3, 64), None);
        assert_eq!(SampleFormat::identify(2, 16), None);
    }

    #[test]
    fn extensible_subformats_resolve_to_their_tags() {
        let float = GUID::from_u128(0x0000_0003_0000_0010_8000_00aa_0038_9b71);
        let pcm = GUID::from_u128(0x0000_0001_0000_0010_8000_00aa_0038_9b71);
        assert_eq!(tag_of_subformat(float), Some(3));
        assert_eq!(tag_of_subformat(pcm), Some(1));
        assert_eq!(tag_of_subformat(GUID::from_u128(0x1234)), None);
    }

    #[test]
    fn floats_are_copied_and_integers_are_scaled_to_unit_range() {
        let mut output = Vec::new();
        append_f32(&[0.5f32.to_le_bytes(), (-1.0f32).to_le_bytes()].concat(), SampleFormat::F32, &mut output);
        append_f32(&[i16::MIN.to_le_bytes(), 16_384i16.to_le_bytes()].concat(), SampleFormat::I16, &mut output);
        append_f32(&[0x00, 0x00, 0x40, 0xFF, 0xFF, 0xFF], SampleFormat::I24, &mut output);
        append_f32(&i32::MAX.to_le_bytes(), SampleFormat::I32, &mut output);
        assert_eq!(&output[..5], [0.5, -1.0, -1.0, 0.5, 0.5]);
        assert!((output[5] + 1.0 / I24_SCALE).abs() < 1e-9);
        assert!((output[6] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_trailing_partial_sample_is_ignored() {
        let mut output = Vec::new();
        append_f32(&[0, 0, 0], SampleFormat::I16, &mut output);
        assert_eq!(output, [0.0]);
    }
}
