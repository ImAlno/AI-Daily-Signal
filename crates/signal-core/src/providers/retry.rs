use std::future::Future;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};

use crate::CancellationToken;

use super::{ProviderFailure, ProviderFailureKind, RequestChargeStatus};

const INITIAL_BACKOFF: Duration = Duration::from_millis(250);

#[async_trait::async_trait]
pub trait RetrySleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

pub struct TokioRetrySleeper;

#[async_trait::async_trait]
impl RetrySleeper for TokioRetrySleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    attempt_timeout: Duration,
    max_retries: u32,
}

impl RetryPolicy {
    pub fn new(attempt_timeout: Duration, max_retries: u32) -> Self {
        Self {
            attempt_timeout,
            max_retries,
        }
    }

    pub fn full_horizon(self) -> Duration {
        self.attempt_timeout
            .saturating_mul(self.max_retries.saturating_add(1))
    }

    pub fn maximum_total_delay(self) -> Duration {
        self.full_horizon()
    }

    pub fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
        if let Ok(seconds) = value.trim().parse::<u64>() {
            return Some(Duration::from_secs(seconds));
        }
        let requested = DateTime::parse_from_rfc2822(value.trim())
            .ok()?
            .with_timezone(&Utc);
        let now = DateTime::<Utc>::from(now);
        Some((requested - now).to_std().unwrap_or(Duration::ZERO))
    }

    fn delay(self, retry_index: u32, retry_after: Option<Duration>) -> Duration {
        if let Some(delay) = retry_after {
            return delay;
        }
        let exponent = retry_index.min(20);
        let base = INITIAL_BACKOFF.saturating_mul(1_u32 << exponent);
        let jitter_bound_ms = u64::try_from(base.as_millis() / 2).unwrap_or(u64::MAX);
        let jitter_ms = fastrand::u64(0..=jitter_bound_ms);
        base.saturating_add(Duration::from_millis(jitter_ms))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryAttemptFailure {
    failure: ProviderFailure,
    retry_after: Option<Duration>,
}

impl RetryAttemptFailure {
    pub fn new(failure: ProviderFailure, retry_after: Option<Duration>) -> Self {
        Self {
            failure,
            retry_after,
        }
    }
}

pub async fn retry_provider_operation<T, F, Fut>(
    policy: &RetryPolicy,
    sleeper: &dyn RetrySleeper,
    operation: F,
) -> Result<T, ProviderFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryAttemptFailure>>,
{
    retry_provider_operation_with_optional_cancel(policy, sleeper, None, operation).await
}

pub async fn retry_provider_operation_with_cancel<T, F, Fut>(
    policy: &RetryPolicy,
    sleeper: &dyn RetrySleeper,
    cancellation: &CancellationToken,
    operation: F,
) -> Result<T, ProviderFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryAttemptFailure>>,
{
    retry_provider_operation_with_optional_cancel(policy, sleeper, Some(cancellation), operation)
        .await
}

async fn retry_provider_operation_with_optional_cancel<T, F, Fut>(
    policy: &RetryPolicy,
    sleeper: &dyn RetrySleeper,
    cancellation: Option<&CancellationToken>,
    mut operation: F,
) -> Result<T, ProviderFailure>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RetryAttemptFailure>>,
{
    let mut retries_used = 0;
    let mut total_delay = Duration::ZERO;
    let mut charge_status = RequestChargeStatus::NotSent;

    loop {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(ProviderFailure::new(
                ProviderFailureKind::Transport,
                charge_status,
            ));
        }
        match operation().await {
            Ok(value) => return Ok(value),
            Err(attempt) => {
                charge_status = charge_status.combine(attempt.failure.charge_status());
                if retries_used >= policy.max_retries || !attempt.failure.kind().is_retryable() {
                    return Err(attempt.failure.with_charge_status(charge_status));
                }

                let remaining = policy.full_horizon().saturating_sub(total_delay);
                let delay = policy
                    .delay(retries_used, attempt.retry_after)
                    .min(remaining);
                sleeper.sleep(delay).await;
                total_delay = total_delay.saturating_add(delay);
                retries_used += 1;
            }
        }
    }
}
