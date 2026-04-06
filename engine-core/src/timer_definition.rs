use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Represents a BPMN 2.0 timer definition.
///
/// Supports all three BPMN timer types:
/// - `timeDuration` → Duration variant
/// - `timeDate` → AbsoluteDate variant  
/// - `timeCycle` → CronCycle or RepeatingInterval variant
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimerDefinition {
    /// ISO 8601 Duration (e.g. PT30S, P1DT2H, P1M).
    /// Converted to std::time::Duration at parse time.
    Duration(Duration),

    /// ISO 8601 absolute date-time (e.g. 2026-04-06T14:30:00Z).
    AbsoluteDate(DateTime<Utc>),

    /// Cron expression for recurring execution (e.g. "0 9 * * MON-FRI").
    CronCycle {
        expression: String,
        /// Optional maximum number of repetitions (None = infinite).
        max_repetitions: Option<u32>,
    },

    /// ISO 8601 repeating interval (e.g. R3/PT10M = repeat 3x every 10min).
    RepeatingInterval {
        /// Number of repetitions (None = infinite for R/...).
        repetitions: Option<u32>,
        /// Interval duration per cycle.
        interval: Duration,
    },
}

impl TimerDefinition {
    /// Calculates the next expiry DateTime from `now`.
    ///
    /// - Duration: now + duration
    /// - AbsoluteDate: the fixed date itself
    /// - CronCycle: next occurrence from cron schedule
    /// - RepeatingInterval: now + interval
    pub fn next_expiry(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            TimerDefinition::Duration(dur) => {
                let chrono_dur = chrono::Duration::from_std(*dur).ok()?;
                Some(now + chrono_dur)
            }
            TimerDefinition::AbsoluteDate(dt) => {
                if *dt > now { Some(*dt) } else { None }
            }
            TimerDefinition::CronCycle { expression, .. } => {
                // Uses croner for next occurrence
                let cron = croner::Cron::new(expression).parse().ok()?;
                cron.find_next_occurrence(&now.into(), false).ok()
            }
            TimerDefinition::RepeatingInterval { interval, .. } => {
                let chrono_dur = chrono::Duration::from_std(*interval).ok()?;
                Some(now + chrono_dur)
            }
        }
    }

    /// Returns true if this timer can fire more than once.
    pub fn is_recurring(&self) -> bool {
        matches!(
            self,
            TimerDefinition::CronCycle { .. } | TimerDefinition::RepeatingInterval { .. }
        )
    }
}
