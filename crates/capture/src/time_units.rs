use std::time::Duration;

pub(crate) const NANOS_PER_SECOND: u64 = 1_000_000_000;
pub(crate) const HNS_PER_SECOND: u64 = 10_000_000;

pub(crate) fn frames_in(span: Duration, rate: u32) -> u128 {
    span.as_nanos() * u128::from(rate) / u128::from(NANOS_PER_SECOND)
}
