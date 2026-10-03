//! Display-time "graph settle" animation.
//!
//! When the signal-flow graph opens it must visibly settle from a compact
//! seed into its solved positions over a short window, then freeze. The solved
//! layout (`App.graph_positions`) stays the single source of truth: this module
//! only interpolates the *drawn* positions, so the animation can never mutate
//! the layout. Pure functions over `(f32, f32)` positions plus an `Instant`-
//! based [`GraphSettle`] driver — no egui/winit dependency, testable headless.

use std::time::Instant;

/// Wall-clock length of the settle animation.
pub const SETTLE_DURATION_MS: u64 = 500;

/// Fraction of the solved extent the compact seed collapses to (per axis
/// around the layout centroid). Small enough to read as a visible contraction.
pub const SETTLE_SEED_SCALE: f32 = 0.08;

/// Standard cubic ease-out: fast start, decelerating finish. `t` is clamped to
/// `0..=1`, so the endpoints are exactly `0.0` and `1.0`.
pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// Collapse `positions` toward their centroid by `scale`: each seed is
/// `centroid + (p - centroid) * scale`. Empty or single-element input is
/// returned as a clone unchanged (no meaningful centroid contraction).
pub fn compact_seed(positions: &[(f32, f32)], scale: f32) -> Vec<(f32, f32)> {
    if positions.len() < 2 {
        return positions.to_vec();
    }
    let (sum_x, sum_y) = positions
        .iter()
        .fold((0.0_f32, 0.0_f32), |(sx, sy), &(x, y)| (sx + x, sy + y));
    let n = positions.len() as f32;
    let centroid = (sum_x / n, sum_y / n);
    positions
        .iter()
        .map(|&(x, y)| {
            (
                centroid.0 + (x - centroid.0) * scale,
                centroid.1 + (y - centroid.1) * scale,
            )
        })
        .collect()
}

/// A running settle animation: the seed positions `from` plus the instant it
/// started. [`positions`](GraphSettle::positions) interpolates from `from` to a
/// target layout under the eased progress; the target is never written back.
#[derive(Debug, Clone)]
pub struct GraphSettle {
    started: Instant,
    from: Vec<(f32, f32)>,
}

impl GraphSettle {
    /// Start a settle now from `from`.
    pub fn new(from: Vec<(f32, f32)>) -> Self {
        Self {
            started: Instant::now(),
            from,
        }
    }

    /// Start a settle with an explicit start instant (test seam).
    pub fn with_started_at(from: Vec<(f32, f32)>, started: Instant) -> Self {
        Self { started, from }
    }

    /// The seed positions this settle interpolates from.
    pub fn from(&self) -> &[(f32, f32)] {
        &self.from
    }

    /// Eased progress in `0.0..=1.0` (elapsed / duration, clamped). Saturating
    /// duration so an instant in the future reads as `0.0` rather than panic.
    pub fn progress(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        let duration = SETTLE_DURATION_MS as f32 / 1000.0;
        (elapsed.as_secs_f32() / duration).clamp(0.0, 1.0)
    }

    /// Whether the animation has reached its end at `now`.
    pub fn is_done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0
    }

    /// The drawn positions at `now`: per-node linear interpolation from the
    /// seed to `target` under the eased progress. A length mismatch (the layout
    /// changed under the animation) falls back to `target` unchanged, so the
    /// renderer never sees an inconsistent position list.
    pub fn positions(&self, target: &[(f32, f32)], now: Instant) -> Vec<(f32, f32)> {
        if self.from.len() != target.len() {
            return target.to_vec();
        }
        let eased = ease_out_cubic(self.progress(now));
        self.from
            .iter()
            .zip(target.iter())
            .map(|(&(fx, fy), &(tx, ty))| (fx + (tx - fx) * eased, fy + (ty - fy) * eased))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ease_out_cubic_endpoints_and_monotonic() {
        assert_eq!(ease_out_cubic(0.0), 0.0);
        assert_eq!(ease_out_cubic(1.0), 1.0);
        // Clamped outside the unit interval.
        assert_eq!(ease_out_cubic(-1.0), 0.0);
        assert_eq!(ease_out_cubic(2.0), 1.0);
        let mut prev = ease_out_cubic(0.0);
        for step in 1..=20 {
            let t = step as f32 / 20.0;
            let v = ease_out_cubic(t);
            assert!(v >= prev, "ease-out must be monotonic at t={t}");
            prev = v;
        }
        // Fast start, decelerating finish: halfway is well past halfway.
        assert!(ease_out_cubic(0.5) > 0.5);
    }

    #[test]
    fn compact_seed_collapses_toward_centroid() {
        let positions = vec![(0.0, 0.0), (100.0, 0.0), (50.0, 200.0)];
        // Centroid = (50, 200/3).
        let seed = compact_seed(&positions, SETTLE_SEED_SCALE);
        assert_eq!(seed.len(), positions.len());
        let (cx, cy) = (50.0, 200.0 / 3.0);
        for (seed, orig) in seed.iter().zip(positions.iter()) {
            // Seed is on the line centroid→orig, at the configured fraction.
            let expected = (
                cx + (orig.0 - cx) * SETTLE_SEED_SCALE,
                cy + (orig.1 - cy) * SETTLE_SEED_SCALE,
            );
            assert!((seed.0 - expected.0).abs() < 1e-4);
            assert!((seed.1 - expected.1).abs() < 1e-4);
            // Collapsed, not moved away: strictly closer to the centroid.
            let orig_dist = (orig.0 - cx).abs() + (orig.1 - cy).abs();
            let seed_dist = (seed.0 - cx).abs() + (seed.1 - cy).abs();
            assert!(seed_dist <= orig_dist + 1e-6);
        }
    }

    #[test]
    fn compact_seed_leaves_single_point_unchanged() {
        let positions = vec![(42.0, -7.0)];
        assert_eq!(compact_seed(&positions, 0.08), positions);
        assert!(compact_seed(&[], 0.08).is_empty());
    }

    #[test]
    fn positions_at_start_equals_seed_and_after_duration_equals_target() {
        let from = vec![(0.0, 0.0), (1.0, 1.0)];
        let target = vec![(100.0, 200.0), (300.0, 400.0)];
        let started = Instant::now();
        let settle = GraphSettle::with_started_at(from.clone(), started);
        assert_eq!(settle.positions(&target, started), from);
        let done = started + Duration::from_millis(SETTLE_DURATION_MS + 10);
        assert_eq!(settle.positions(&target, done), target);
        assert!(settle.is_done(done));
        assert!(!settle.is_done(started));
    }

    #[test]
    fn positions_length_mismatch_returns_target() {
        let settle = GraphSettle::with_started_at(vec![(0.0, 0.0)], Instant::now());
        let target = vec![(1.0, 1.0), (2.0, 2.0)];
        assert_eq!(settle.positions(&target, Instant::now()), target);
    }
}
