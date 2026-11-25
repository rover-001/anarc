//! Pure bucketing helpers. Inputs are always counts, lengths or durations —
//! never page content. Only the returned band code can leave the machine.

use crate::schema::Bucket;

/// 0 · 1-5 · 6-20 · 21-99 · 100+. Used for sessions, per-action call counts,
/// per-error counts and plugin counts.
pub fn bucket_count(value: u64) -> Bucket {
    match value {
        0 => Bucket::new(0),
        1..=5 => Bucket::new(1),
        6..=20 => Bucket::new(2),
        21..=99 => Bucket::new(3),
        _ => Bucket::new(4),
    }
}

/// `bucket_count` under its session-specific name.
pub fn bucket_sessions(sessions: u64) -> Bucket {
    bucket_count(sessions)
}

/// ref-snapshot size / raw-DOM size, as a percentage: <10 · 10-25 · 25-50 ·
/// 50-100. Ratios above 100% are clamped into the top band; a zero denominator
/// yields band 0 (no measurement available).
///
/// Integer math only; no content is inspected, only character lengths.
pub fn bucket_token_ratio(ref_chars: u64, dom_chars: u64) -> Bucket {
    if dom_chars == 0 {
        return Bucket::new(0);
    }
    let percent = (u128::from(ref_chars) * 100) / u128::from(dom_chars);
    match percent {
        0..=9 => Bucket::new(0),
        10..=24 => Bucket::new(1),
        25..=49 => Bucket::new(2),
        _ => Bucket::new(3),
    }
}

/// Snapshot wall time in milliseconds: <10 · 10-50 · 50-200 · 200+.
pub fn bucket_snapshot_ms(ms: u64) -> Bucket {
    match ms {
        0..=9 => Bucket::new(0),
        10..=49 => Bucket::new(1),
        50..=199 => Bucket::new(2),
        _ => Bucket::new(3),
    }
}

/// Element count: <50 · 50-200 · 200-1000 · 1000+.
pub fn bucket_element_count(count: u64) -> Bucket {
    match count {
        0..=49 => Bucket::new(0),
        50..=199 => Bucket::new(1),
        200..=999 => Bucket::new(2),
        _ => Bucket::new(3),
    }
}

/// Ref resolution success percentage in 10-point bands; inputs are clamped to
/// 0..=100 so malformed callers cannot emit an out-of-range code.
pub fn bucket_ref_resolve_success(percent: u64) -> Bucket {
    Bucket::new((percent.min(99) / 10) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_bands_at_every_boundary() {
        assert_eq!(bucket_count(0).code(), 0);
        assert_eq!(bucket_count(1).code(), 1);
        assert_eq!(bucket_count(5).code(), 1);
        assert_eq!(bucket_count(6).code(), 2);
        assert_eq!(bucket_count(20).code(), 2);
        assert_eq!(bucket_count(21).code(), 3);
        assert_eq!(bucket_count(99).code(), 3);
        assert_eq!(bucket_count(100).code(), 4);
        assert_eq!(bucket_count(u64::MAX).code(), 4);
        assert_eq!(bucket_sessions(5).code(), 1);
    }

    #[test]
    fn token_ratio_bands_at_every_boundary() {
        // denominator 100 makes the percentage equal to the numerator.
        assert_eq!(bucket_token_ratio(0, 100).code(), 0);
        assert_eq!(bucket_token_ratio(9, 100).code(), 0);
        assert_eq!(bucket_token_ratio(10, 100).code(), 1);
        assert_eq!(bucket_token_ratio(24, 100).code(), 1);
        assert_eq!(bucket_token_ratio(25, 100).code(), 2);
        assert_eq!(bucket_token_ratio(49, 100).code(), 2);
        assert_eq!(bucket_token_ratio(50, 100).code(), 3);
        assert_eq!(bucket_token_ratio(100, 100).code(), 3);
        assert_eq!(bucket_token_ratio(200, 100).code(), 3);
        assert_eq!(bucket_token_ratio(1, 0).code(), 0);
        // Integer math must not overflow for large DOMs.
        assert_eq!(bucket_token_ratio(u64::MAX, u64::MAX).code(), 3);
    }

    #[test]
    fn snapshot_ms_bands_at_every_boundary() {
        assert_eq!(bucket_snapshot_ms(0).code(), 0);
        assert_eq!(bucket_snapshot_ms(9).code(), 0);
        assert_eq!(bucket_snapshot_ms(10).code(), 1);
        assert_eq!(bucket_snapshot_ms(49).code(), 1);
        assert_eq!(bucket_snapshot_ms(50).code(), 2);
        assert_eq!(bucket_snapshot_ms(199).code(), 2);
        assert_eq!(bucket_snapshot_ms(200).code(), 3);
        assert_eq!(bucket_snapshot_ms(u64::MAX).code(), 3);
    }

    #[test]
    fn element_count_bands_at_every_boundary() {
        assert_eq!(bucket_element_count(0).code(), 0);
        assert_eq!(bucket_element_count(49).code(), 0);
        assert_eq!(bucket_element_count(50).code(), 1);
        assert_eq!(bucket_element_count(199).code(), 1);
        assert_eq!(bucket_element_count(200).code(), 2);
        assert_eq!(bucket_element_count(999).code(), 2);
        assert_eq!(bucket_element_count(1000).code(), 3);
        assert_eq!(bucket_element_count(u64::MAX).code(), 3);
    }

    #[test]
    fn resolve_success_bands_at_every_boundary() {
        for percent in 0..100 {
            assert_eq!(
                bucket_ref_resolve_success(percent).code(),
                (percent / 10) as u8
            );
        }
        assert_eq!(bucket_ref_resolve_success(100).code(), 9);
        assert_eq!(bucket_ref_resolve_success(101).code(), 9);
        assert_eq!(bucket_ref_resolve_success(u64::MAX).code(), 9);
    }
}
