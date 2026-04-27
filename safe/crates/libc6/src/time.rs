pub use core_runtime::time::{days_in_month, is_leap_year};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn february_in_leap_year_has_29_days() {
        assert_eq!(days_in_month(2024, 2), Some(29));
    }
}
