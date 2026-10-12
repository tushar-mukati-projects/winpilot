pub(crate) fn remap(input: &[f32], from: usize, to: usize) -> Vec<f32> {
    if from == to || from == 0 || to == 0 {
        return input.to_vec();
    }
    let mut output = Vec::with_capacity(input.len() / from * to);
    for frame in input.chunks_exact(from) {
        output.extend((0..to).map(|channel| mapped(frame, channel, to)));
    }
    output
}

fn mapped(frame: &[f32], channel: usize, to: usize) -> f32 {
    let from = frame.len();
    if to == 1 {
        return frame.iter().sum::<f32>() / from as f32;
    }
    if from < to {
        return frame[channel % from];
    }
    let folded: Vec<f32> = frame.iter().skip(channel).step_by(to).copied().collect();
    folded.iter().sum::<f32>() / folded.len().max(1) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_layouts_pass_through() {
        assert_eq!(remap(&[0.1, 0.2, 0.3, 0.4], 2, 2), [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn mono_is_spread_to_every_channel() {
        assert_eq!(remap(&[0.5, -0.25], 1, 2), [0.5, 0.5, -0.25, -0.25]);
    }

    #[test]
    fn stereo_downmixes_to_the_average() {
        assert_eq!(remap(&[1.0, 0.0, -0.5, -0.5], 2, 1), [0.5, -0.5]);
    }

    #[test]
    fn wider_layouts_fold_onto_narrower_ones() {
        assert_eq!(remap(&[0.2, 0.4, 0.6, 0.8], 4, 2), [0.4, 0.6]);
        assert_eq!(remap(&[0.1, 0.2], 2, 4), [0.1, 0.2, 0.1, 0.2]);
    }

    #[test]
    fn a_partial_trailing_frame_is_dropped() {
        assert_eq!(remap(&[0.5, 0.5, 0.25], 2, 1), [0.5]);
    }
}
