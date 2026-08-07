// Plateau detection — nudge an agent when it stops improving, not when a clock
// expires.
//
// Ported from CORAL (Apache 2.0), `coral/agent/heartbeat.py::streak_for_epsilon`.
//
// The manager used to nudge every five minutes regardless of what the agent was
// doing. That is wrong in both directions at once: it interrupts an agent that
// is making progress, and it waits five minutes on one that has been going in
// circles since the first minute. Progress, not elapsed time, is the thing worth
// reacting to.
//
// The model is an ANCHOR. Walk the score history keeping the most recent score
// that beat the previous anchor by at least `epsilon`; the streak is how many
// evals have happened since the anchor last moved. `epsilon` exists so that
// noise-level inch-ups do not keep resetting the streak forever — set it to the
// task's noise floor.
//
// Two details from the original are load-bearing and easy to get wrong:
//
//   * A failed eval (`None`) counts TOWARD the streak but does not move the
//     anchor. A gate that crashes is not progress, and it should still apply
//     plateau pressure once a baseline exists.
//   * `None`s BEFORE the first real score are discarded. There is nothing to
//     plateau against yet, so the streak starts at zero the moment the first
//     real score arrives — otherwise a slow start would look like a stall.

/// Evals since the score last improved by at least `epsilon`.
///
/// `history` is in submit order; `None` is an eval that produced no score.
/// Returns 0 when the latest score was itself an improvement, and 0 when there
/// are no scores at all.
pub fn streak_for_epsilon(history: &[Option<f64>], minimize: bool, epsilon: f64) -> usize {
    let mut streak = 0usize;
    let mut anchor: Option<f64> = None;
    for score in history {
        let Some(score) = *score else {
            // Before any real score there is no baseline to plateau against, so
            // a leading failure is discarded rather than counted.
            if anchor.is_some() {
                streak += 1;
            }
            continue;
        };
        let Some(current) = anchor else {
            anchor = Some(score);
            streak = 0;
            continue;
        };
        let improved = if minimize {
            score < current - epsilon
        } else {
            score > current + epsilon
        };
        if improved {
            anchor = Some(score);
            streak = 0;
        } else {
            streak += 1;
        }
    }
    streak
}

/// What the caller should do about a history, so the decision lives in one
/// place rather than being re-derived by every caller in JS.
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct PlateauVerdict {
    pub streak: usize,
    /// True once the streak reaches `threshold` — the moment to intervene.
    pub stalled: bool,
    /// Best score seen so far, for display.
    pub best: Option<f64>,
    /// Most recent real score, for display.
    pub latest: Option<f64>,
}

/// `threshold` evals without an epsilon-sized improvement means stalled.
#[tauri::command]
pub fn plateau_check(
    history: Vec<Option<f64>>,
    threshold: usize,
    epsilon: Option<f64>,
    minimize: Option<bool>,
) -> PlateauVerdict {
    let minimize = minimize.unwrap_or(false);
    let epsilon = epsilon.unwrap_or(0.0);
    let streak = streak_for_epsilon(&history, minimize, epsilon);
    let reals: Vec<f64> = history.iter().flatten().copied().collect();
    let best = reals
        .iter()
        .copied()
        .reduce(|a, b| if minimize { a.min(b) } else { a.max(b) });
    PlateauVerdict {
        streak,
        // threshold 0 would mean "always stalled", which would nudge forever.
        stalled: threshold > 0 && streak >= threshold,
        best,
        latest: reals.last().copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(v: &[f64]) -> Vec<Option<f64>> {
        v.iter().map(|x| Some(*x)).collect()
    }

    #[test]
    fn steady_improvement_never_stalls() {
        let hist = h(&[0.1, 0.3, 0.5, 0.7]);
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 0);
    }

    #[test]
    fn a_flat_run_counts_every_eval_since_the_anchor() {
        let hist = h(&[0.5, 0.5, 0.5, 0.5]);
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 3);
    }

    #[test]
    fn epsilon_ignores_inch_ups_below_the_noise_floor() {
        // Without epsilon these tiny gains reset the streak forever and the
        // agent is never nudged, which is the bug epsilon exists to fix.
        let hist = h(&[0.50, 0.501, 0.502, 0.503]);
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 0);
        assert_eq!(streak_for_epsilon(&hist, false, 0.01), 3);
    }

    #[test]
    fn a_real_gain_resets_the_streak() {
        let hist = h(&[0.5, 0.5, 0.5, 0.9, 0.9]);
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 1);
    }

    #[test]
    fn failed_evals_apply_pressure_but_do_not_move_the_anchor() {
        let hist = vec![Some(0.5), None, None, Some(0.5)];
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 3);
        // And a later real improvement still resets, proving the anchor stayed
        // at 0.5 rather than being dragged by the failures.
        let hist = vec![Some(0.5), None, Some(0.9)];
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 0);
    }

    #[test]
    fn leading_failures_are_discarded_not_counted() {
        // A slow start is not a stall: there is no baseline to plateau against.
        let hist = vec![None, None, Some(0.4)];
        assert_eq!(streak_for_epsilon(&hist, false, 0.0), 0);
    }

    #[test]
    fn minimize_direction_treats_lower_as_better() {
        let hist = h(&[10.0, 8.0, 6.0]);
        assert_eq!(streak_for_epsilon(&hist, true, 0.0), 0);
        let rising = h(&[6.0, 8.0, 10.0]);
        assert_eq!(streak_for_epsilon(&rising, true, 0.0), 2);
    }

    #[test]
    fn empty_history_is_not_a_stall() {
        assert_eq!(streak_for_epsilon(&[], false, 0.0), 0);
        assert!(!plateau_check(vec![], 2, None, None).stalled);
    }

    #[test]
    fn threshold_zero_does_not_mean_always_stalled() {
        // Guard against a config typo turning into an infinite nudge loop.
        let v = plateau_check(h(&[0.5, 0.5, 0.5]), 0, None, None);
        assert!(!v.stalled);
    }

    #[test]
    fn verdict_reports_best_and_latest() {
        let v = plateau_check(h(&[0.2, 0.9, 0.4]), 2, None, None);
        assert_eq!(v.best, Some(0.9));
        assert_eq!(v.latest, Some(0.4));
        assert_eq!(v.streak, 1);
        assert!(!v.stalled);
    }
}
