use chrono::NaiveDateTime;

pub const MAX_SUFFIX: u32 = 999;

pub fn default_stem(now: NaiveDateTime) -> String {
    format!("deckhand-{}", now.format("%Y%m%d-%H%M%S"))
}

pub fn candidates(stem: &str, extension: &str) -> impl Iterator<Item = String> {
    let first = format!("{stem}.{extension}");
    let numbered = (2..=MAX_SUFFIX).map(move |suffix| format!("{stem}-{suffix}.{extension}"));
    std::iter::once(first).chain(numbered)
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn moment(hour: u32, minute: u32, second: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 11).unwrap().and_hms_opt(hour, minute, second).unwrap()
    }

    #[test]
    fn default_names_carry_the_local_date_and_time() {
        assert_eq!(default_stem(moment(10, 15, 0)), "deckhand-20261011-101500");
        assert_eq!(default_stem(moment(0, 0, 9)), "deckhand-20261011-000009");
    }

    #[test]
    fn candidates_start_plain_then_count_up_from_two() {
        let names: Vec<String> = candidates("deckhand-20261011-101500", "mp4").take(3).collect();
        assert_eq!(
            names,
            ["deckhand-20261011-101500.mp4", "deckhand-20261011-101500-2.mp4", "deckhand-20261011-101500-3.mp4"]
        );
        assert_eq!(candidates("x", "gif").last().as_deref(), Some("x-999.gif"));
        assert_eq!(candidates("x", "gif").count(), MAX_SUFFIX as usize);
    }
}
