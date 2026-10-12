use deckhand_core::{Context, Result};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

const HNS_PER_SECOND: i128 = 10_000_000;

pub(crate) fn qpc_now_hns() -> Result<i64> {
    let mut counter = 0i64;
    let mut frequency = 0i64;
    unsafe { QueryPerformanceCounter(&raw mut counter) }.context("reading the performance counter")?;
    unsafe { QueryPerformanceFrequency(&raw mut frequency) }.context("reading the performance counter rate")?;
    Ok(hns_of(counter, frequency))
}

fn hns_of(counter: i64, frequency: i64) -> i64 {
    let hns = i128::from(counter) * HNS_PER_SECOND / i128::from(frequency.max(1));
    i64::try_from(hns).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_ticks_become_hundred_nanosecond_units() {
        assert_eq!(hns_of(10_000_000, 10_000_000), 10_000_000);
        assert_eq!(hns_of(3_000_000, 1_000_000), 30_000_000);
        assert_eq!(hns_of(5, 0), 50_000_000);
    }

    #[test]
    fn the_clock_moves_forward() {
        let first = qpc_now_hns().unwrap();
        let second = qpc_now_hns().unwrap();
        assert!(first > 0 && second >= first);
    }
}
