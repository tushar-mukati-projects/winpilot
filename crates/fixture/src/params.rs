pub fn low_word(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

pub fn high_word(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_words() {
        assert_eq!(low_word(0x0001_07D3), 2003);
        assert_eq!(high_word(0x0001_07D3), 1);
    }

    #[test]
    fn ignores_bits_above_the_low_dword() {
        assert_eq!(high_word(0xFFFF_FFFF_0002_0003), 2);
        assert_eq!(low_word(0xFFFF_FFFF_0002_0003), 3);
    }
}
