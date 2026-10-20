use std::time::Duration;

pub fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn millis_saturate() {
        assert_eq!(millis(Duration::from_millis(1500)), 1500);
        assert_eq!(millis(Duration::MAX), u64::MAX);
    }
}
