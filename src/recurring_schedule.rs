//! Pure construction and validation for recurring runtime definitions.

use chrono::{DateTime, Duration, Utc};
use stasis::domain::runtime::recurring::RecurringDefinition;
use stasis::prelude::{Result as StasisResult, StasisError};

pub const MIN_SCHEDULE_INTERVAL_SECS: i64 = 60;
pub const CRON_FORMAT_HINT: &str =
    "sec min hour day-of-month month day-of-week year (example every 4h: 0 0 */4 * * * *)";

/// Expand common 5/6-field Unix cron into the 7-field form Stasis expects.
///
/// - 5 fields (`min hour dom month dow`) → `0 min hour dom month dow *`
/// - 6 fields (`sec min hour dom month dow`) → `sec min hour dom month dow *`
/// - 7 fields left unchanged
pub fn normalize_recurring_cron_expr(cron_expr: &str) -> String {
    let trimmed = cron_expr.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    match parts.len() {
        5 => format!(
            "0 {} {} {} {} {} *",
            parts[0], parts[1], parts[2], parts[3], parts[4]
        ),
        6 => format!(
            "{} {} {} {} {} {} *",
            parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]
        ),
        _ => trimmed.to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecurringStartPolicy {
    #[default]
    NextScheduled,
    Immediate,
}

/// Pure input for building a Stasis recurring definition.
///
/// The spec owns correlated construction fields and next-run semantics, but
/// does not persist, bind delivery, or perform any other I/O.
#[derive(Debug, Clone)]
pub struct RecurringScheduleSpec {
    id: String,
    queue: String,
    job_type: String,
    payload_template_ref: String,
    cron_expr: String,
    timezone: String,
    jitter_seconds: i64,
    enabled: bool,
    max_attempts: u32,
    start_policy: RecurringStartPolicy,
}

impl RecurringScheduleSpec {
    pub fn new(
        id: impl Into<String>,
        queue: impl Into<String>,
        job_type: impl Into<String>,
        payload_template_ref: impl Into<String>,
        cron_expr: impl Into<String>,
        timezone: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            queue: queue.into(),
            job_type: job_type.into(),
            payload_template_ref: payload_template_ref.into(),
            cron_expr: cron_expr.into(),
            timezone: timezone.into(),
            jitter_seconds: 0,
            enabled: true,
            max_attempts: 1,
            start_policy: RecurringStartPolicy::NextScheduled,
        }
    }

    pub fn jitter_seconds(mut self, value: i64) -> Self {
        self.jitter_seconds = value;
        self
    }

    pub fn enabled(mut self, value: bool) -> Self {
        self.enabled = value;
        self
    }

    pub fn max_attempts(mut self, value: u32) -> Self {
        self.max_attempts = value;
        self
    }

    pub fn start_policy(mut self, value: RecurringStartPolicy) -> Self {
        self.start_policy = value;
        self
    }

    pub fn start_immediately(self, value: bool) -> Self {
        self.start_policy(if value {
            RecurringStartPolicy::Immediate
        } else {
            RecurringStartPolicy::NextScheduled
        })
    }

    pub fn build(self, now: DateTime<Utc>) -> StasisResult<RecurringDefinition> {
        require_non_blank("recurring_id", &self.id)?;
        require_non_blank("queue", &self.queue)?;
        require_non_blank("job_type", &self.job_type)?;
        require_non_blank("payload_template_ref", &self.payload_template_ref)?;
        require_non_blank("cron_expr", &self.cron_expr)?;
        require_non_blank("timezone", &self.timezone)?;

        let cron_expr = normalize_recurring_cron_expr(&self.cron_expr);
        require_non_blank("cron_expr", &cron_expr)?;

        let mut definition = RecurringDefinition {
            id: self.id,
            queue: self.queue,
            job_type: self.job_type,
            payload_template_ref: self.payload_template_ref,
            cron_expr,
            timezone: self.timezone,
            jitter_seconds: self.jitter_seconds,
            enabled: self.enabled,
            max_attempts: self.max_attempts,
            next_run_at: now,
            last_run_at: None,
            lease_owner: None,
            lease_expires_at: None,
        };

        let first = definition.compute_next_run_at(now)?;
        let second = definition.compute_next_run_at(first + Duration::seconds(1))?;
        let interval = second.signed_duration_since(first).num_seconds();
        if interval < MIN_SCHEDULE_INTERVAL_SECS {
            return Err(StasisError::PortFailure(format!(
                "cron schedule fires too frequently (interval={interval}s); minimum is {MIN_SCHEDULE_INTERVAL_SECS}s. Use 7-field cron: {CRON_FORMAT_HINT}"
            )));
        }

        definition.next_run_at = match self.start_policy {
            RecurringStartPolicy::Immediate => now,
            RecurringStartPolicy::NextScheduled => first,
        };
        Ok(definition)
    }
}

fn require_non_blank(label: &str, value: &str) -> StasisResult<()> {
    if value.trim().is_empty() {
        return Err(StasisError::PortFailure(format!("{label} is required")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{RecurringScheduleSpec, RecurringStartPolicy};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .single()
            .expect("valid timestamp")
    }

    fn spec() -> RecurringScheduleSpec {
        RecurringScheduleSpec::new(
            "recur-test",
            "default",
            "workflow.test",
            "payload",
            "0 0 */4 * * * *",
            "UTC",
        )
    }

    #[test]
    fn build_injects_now_for_immediate_schedules() {
        let definition = spec()
            .start_policy(RecurringStartPolicy::Immediate)
            .build(now())
            .expect("valid schedule");
        assert_eq!(definition.next_run_at, now());
        assert_eq!(definition.id, "recur-test");
    }

    #[test]
    fn build_computes_the_first_scheduled_run() {
        let definition = spec().build(now()).expect("valid schedule");
        assert!(definition.next_run_at > now());
    }

    #[test]
    fn build_rejects_blank_invariant_fields() {
        let error = RecurringScheduleSpec::new(
            " ",
            "default",
            "workflow.test",
            "payload",
            "0 0 */4 * * * *",
            "UTC",
        )
        .build(now())
        .unwrap_err();
        assert!(error.to_string().contains("recurring_id is required"));
    }

    #[test]
    fn build_accepts_five_field_unix_cron() {
        let definition = RecurringScheduleSpec::new(
            "recur-5field",
            "default",
            "workflow.test",
            "payload",
            "0 9 * * *",
            "UTC",
        )
        .build(now())
        .expect("5-field cron should normalize");
        assert_eq!(definition.cron_expr, "0 0 9 * * * *");
    }
}
