//! Tests for the confidence scoring and decay system.

use arcana_core::confidence::{ConfidenceDecay, ConfidenceScore};
use chrono::{Duration, Utc};

#[test]
fn confidence_score_clamps_above_one() {
    let score = ConfidenceScore::new(1.5);
    assert_eq!(score.value(), 1.0);
}

#[test]
fn confidence_score_clamps_below_zero() {
    let score = ConfidenceScore::new(-0.5);
    assert_eq!(score.value(), 0.0);
}

#[test]
fn confidence_score_is_above() {
    let score = ConfidenceScore::new(0.8);
    assert!(score.is_above(0.5));
    assert!(score.is_above(0.8));
    assert!(!score.is_above(0.81));
}

#[test]
fn confidence_score_is_stale() {
    let score = ConfidenceScore::new(0.3);
    assert!(score.is_stale(0.4));
    assert!(!score.is_stale(0.3));
    assert!(!score.is_stale(0.2));
}

#[test]
fn confidence_from_f64() {
    let score: ConfidenceScore = 0.75.into();
    assert_eq!(score.value(), 0.75);
    let val: f64 = score.into();
    assert_eq!(val, 0.75);
}

#[test]
fn decay_default_values() {
    let decay = ConfidenceDecay::default();
    assert_eq!(decay.decay_rate_per_day, 0.02);
    assert_eq!(decay.stale_threshold, 0.4);
}

#[test]
fn no_decay_when_refreshed_now() {
    let decay = ConfidenceDecay::default();
    let now = Utc::now();
    let score = decay.apply(ConfidenceScore::new(1.0), now, Some(now));
    assert!((score.value() - 1.0).abs() < 1e-9);
}

#[test]
fn no_decay_for_future_refresh() {
    let decay = ConfidenceDecay::default();
    let future = Utc::now() + Duration::days(10);
    let score = decay.apply(ConfidenceScore::new(0.9), future, None);
    // refreshed_at is in the future, so no decay
    assert!((score.value() - 0.9).abs() < 1e-6);
}

#[test]
fn exponential_decay_one_day() {
    let decay = ConfidenceDecay {
        decay_rate_per_day: 0.1,
        stale_threshold: 0.4,
    };
    let refreshed_at = Utc::now() - Duration::days(1);
    let score = decay.apply(ConfidenceScore::new(1.0), refreshed_at, None);
    // e^(-0.1 * 1) ≈ 0.905
    assert!((score.value() - 0.905).abs() < 0.01);
}

#[test]
fn exponential_decay_ten_days() {
    let decay = ConfidenceDecay {
        decay_rate_per_day: 0.1,
        stale_threshold: 0.4,
    };
    let refreshed_at = Utc::now() - Duration::days(10);
    let score = decay.apply(ConfidenceScore::new(1.0), refreshed_at, None);
    // e^(-0.1 * 10) = e^(-1) ≈ 0.368
    assert!(score.value() < 0.4);
    assert!(decay.is_stale(score));
}

#[test]
fn decay_with_non_unit_initial_score() {
    let decay = ConfidenceDecay::default();
    let refreshed_at = Utc::now() - Duration::days(30);
    let score = decay.apply(ConfidenceScore::new(0.8), refreshed_at, None);
    // 0.8 * e^(-0.02 * 30) = 0.8 * e^(-0.6) ≈ 0.8 * 0.549 ≈ 0.439
    assert!(score.value() > 0.4);
    assert!(score.value() < 0.5);
}

#[test]
fn decayed_score_none_refreshed_at() {
    let decay = ConfidenceDecay::default();
    // None means "never refreshed" — should apply ~365 days of decay
    let score = decay.decayed_score(1.0, None);
    // e^(-0.02 * 365) ≈ e^(-7.3) ≈ 0.00067
    assert!(score.value() < 0.01);
    assert!(decay.is_stale(score));
}

#[test]
fn decayed_score_with_recent_refresh() {
    let decay = ConfidenceDecay::default();
    let recent = Some(Utc::now());
    let score = decay.decayed_score(0.9, recent);
    assert!((score.value() - 0.9).abs() < 0.01);
}

#[test]
fn stale_threshold_boundary() {
    let decay = ConfidenceDecay {
        decay_rate_per_day: 0.02,
        stale_threshold: 0.5,
    };
    assert!(decay.is_stale(ConfidenceScore::new(0.49)));
    assert!(!decay.is_stale(ConfidenceScore::new(0.50)));
    assert!(!decay.is_stale(ConfidenceScore::new(0.51)));
}
