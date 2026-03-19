use chrono::{DateTime, Utc};

/// A confidence score in the range [0.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ConfidenceScore(f64);

impl ConfidenceScore {
    /// Construct a new confidence score, clamping to [0.0, 1.0].
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// The raw score value.
    pub fn value(&self) -> f64 {
        self.0
    }

    /// Returns `true` if the score is at or above the given threshold.
    pub fn is_above(&self, threshold: f64) -> bool {
        self.0 >= threshold
    }

    /// Returns `true` if the score is below the stale threshold (default 0.4).
    pub fn is_stale(&self, stale_threshold: f64) -> bool {
        self.0 < stale_threshold
    }
}

impl From<f64> for ConfidenceScore {
    fn from(v: f64) -> Self {
        Self::new(v)
    }
}

impl From<ConfidenceScore> for f64 {
    fn from(c: ConfidenceScore) -> f64 {
        c.0
    }
}

/// Configuration for confidence decay.
#[derive(Debug, Clone)]
pub struct ConfidenceDecay {
    /// Fraction of confidence lost per day (e.g., 0.02 = 2% per day).
    pub decay_rate_per_day: f64,
    /// Score below which an entity is considered stale.
    pub stale_threshold: f64,
}

impl Default for ConfidenceDecay {
    fn default() -> Self {
        Self {
            decay_rate_per_day: 0.02,
            stale_threshold: 0.4,
        }
    }
}

impl ConfidenceDecay {
    /// Apply exponential decay to a confidence score based on elapsed time.
    ///
    /// Uses the formula: `score * e^(-decay_rate * days_elapsed)`
    ///
    /// # Arguments
    /// * `current_score` - The score at the time of the last refresh.
    /// * `refreshed_at` - When the score was last refreshed.
    /// * `now` - Current time (defaults to `Utc::now()` if not provided).
    pub fn apply(
        &self,
        current_score: ConfidenceScore,
        refreshed_at: DateTime<Utc>,
        now: Option<DateTime<Utc>>,
    ) -> ConfidenceScore {
        let now = now.unwrap_or_else(Utc::now);
        let elapsed = now.signed_duration_since(refreshed_at);
        let days_elapsed = elapsed.num_seconds() as f64 / 86_400.0;

        if days_elapsed <= 0.0 {
            return current_score;
        }

        let decayed = current_score.value() * (-self.decay_rate_per_day * days_elapsed).exp();
        ConfidenceScore::new(decayed)
    }

    /// Compute the decayed score for an entity given its last refresh time.
    ///
    /// Returns `None` if `refreshed_at` is `None` (never refreshed → treat as max decay).
    pub fn decayed_score(
        &self,
        current_score: f64,
        refreshed_at: Option<DateTime<Utc>>,
    ) -> ConfidenceScore {
        match refreshed_at {
            Some(refreshed_at) => {
                self.apply(ConfidenceScore::new(current_score), refreshed_at, None)
            }
            // Never refreshed: apply a large decay (e.g., 365 days).
            None => {
                let worst_case = Utc::now() - chrono::Duration::days(365);
                self.apply(ConfidenceScore::new(current_score), worst_case, None)
            }
        }
    }

    /// Whether the (decayed) score should be flagged as stale.
    pub fn is_stale(&self, score: ConfidenceScore) -> bool {
        score.is_stale(self.stale_threshold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_to_range() {
        assert_eq!(ConfidenceScore::new(1.5).value(), 1.0);
        assert_eq!(ConfidenceScore::new(-0.1).value(), 0.0);
    }

    #[test]
    fn no_decay_when_just_refreshed() {
        let decay = ConfidenceDecay::default();
        let now = Utc::now();
        let score = decay.apply(ConfidenceScore::new(1.0), now, Some(now));
        assert!((score.value() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn decays_over_time() {
        let decay = ConfidenceDecay {
            decay_rate_per_day: 0.02,
            stale_threshold: 0.4,
        };
        let refreshed_at = Utc::now() - chrono::Duration::days(50);
        let score = decay.apply(ConfidenceScore::new(1.0), refreshed_at, None);
        // e^(-0.02 * 50) = e^(-1) ≈ 0.368
        assert!(score.value() < 0.4);
        assert!(decay.is_stale(score));
    }
}
