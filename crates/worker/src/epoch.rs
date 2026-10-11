const SLOTS: u32 = 40;
const SPAN: u32 = 100_000_000;

pub fn first_ref(epoch: u32) -> u32 {
    1 + (epoch % SLOTS) * SPAN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_worker_gets_its_own_range() {
        assert_eq!(first_ref(0), 1);
        assert_eq!(first_ref(1), 100_000_001);
        assert_eq!(first_ref(39), 3_900_000_001);
    }

    #[test]
    fn ranges_wrap_without_overflowing() {
        assert_eq!(first_ref(40), first_ref(0));
        assert_eq!(first_ref(u32::MAX), first_ref(u32::MAX % SLOTS));
    }
}
