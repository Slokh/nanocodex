use std::{
    collections::BTreeMap,
    error::Error,
    future::Future,
    path::{Component, PathBuf},
    pin::Pin,
};

use chrono::{DateTime, Utc};
use nanocodex_agent::TurnUsage;
use serde::{Deserialize, Serialize};

use crate::{EvalAttempt, PhaseTiming, Task, VerifierResult};

/// Error returned by a post-verifier scorer.
pub type AttemptScoreError = Box<dyn Error + Send + Sync + 'static>;

/// Borrowing future returned by [`AttemptScorer::score`].
pub type AttemptScoreFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ScoreContribution, AttemptScoreError>> + Send + 'a>>;

/// Stable identity of one post-verifier scoring configuration.
///
/// The configuration digest must change whenever prompts, models, rubric
/// interpretation, or other behavior that can change scores changes.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ScorerIdentity {
    name: String,
    version: String,
    configuration_digest: String,
}

/// Failure to construct a stable scorer identity.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ScorerIdentityError {
    /// The scorer name was empty or unsafe for an artifact path.
    #[error(
        "scorer name must begin with an ASCII letter or digit and contain only ASCII letters, digits, '.', '_', or '-'"
    )]
    InvalidName,
    /// The scorer version was empty.
    #[error("scorer version must not be empty")]
    EmptyVersion,
    /// The configuration digest was not a lowercase SHA-256 digest.
    #[error(
        "scorer configuration digest must be a 64-character lowercase hexadecimal SHA-256 digest"
    )]
    InvalidConfigurationDigest,
}

/// Immutable inputs supplied after the task verifier has completed.
#[derive(Clone, Copy)]
pub struct ScoreContext<'a> {
    attempt: EvalAttempt<'a>,
    canonical: &'a VerifierResult,
}

/// Whether a scorer evaluated the attempt or deliberately declined to run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreDisposition {
    /// The scorer evaluated the attempt.
    Completed,
    /// The scorer deliberately skipped the attempt under its declared policy.
    Skipped,
}

/// Collision-safe rewards and evidence returned by a post-verifier scorer.
#[derive(Clone, Debug)]
pub struct ScoreContribution {
    disposition: ScoreDisposition,
    rewards: BTreeMap<String, f64>,
    reason: Option<String>,
    artifacts: BTreeMap<String, PathBuf>,
    model_usage: Option<TurnUsage>,
}

/// Durable terminal state of one configured scorer.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScorerStatus {
    /// The scorer produced validated rewards.
    Completed,
    /// The scorer deliberately skipped and produced its declared fallback rewards.
    Skipped,
    /// The scorer or its contribution contract failed after canonical verification.
    Failed,
}

/// Complete retained diagnostic for a failed scorer.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScorerDiagnostic {
    /// Human-readable primary error.
    pub message: String,
    /// Complete formatted error chain.
    pub traceback: String,
}

/// Durable provenance and output for one post-verifier scorer invocation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ScorerReport {
    /// Stable scorer implementation and configuration identity.
    pub identity: ScorerIdentity,
    /// Terminal scorer state.
    pub status: ScorerStatus,
    /// Validated rewards contributed by this scorer.
    pub rewards: BTreeMap<String, f64>,
    /// Policy reason retained for a skipped scorer.
    pub reason: Option<String>,
    /// Scorer execution interval, including evidence retention.
    pub timing: PhaseTiming,
    /// Attempt-relative evidence paths keyed by stable artifact name.
    pub artifacts: BTreeMap<String, PathBuf>,
    /// Provider usage when the scorer used a Nanocodex model turn.
    pub model_usage: Option<TurnUsage>,
    /// Error details when scoring failed.
    pub diagnostic: Option<ScorerDiagnostic>,
}

/// Optional post-verifier scorer for one evaluation attempt.
///
/// The canonical verifier result is immutable. Implementations return a
/// contribution, and the evaluator rejects non-finite values, malformed
/// artifact paths, and reward names already owned by the verifier.
pub trait AttemptScorer: Send + Sync {
    /// Returns the stable identity bound into durable run identity.
    fn identity(&self) -> &ScorerIdentity;

    /// Scores one attempt after canonical verification completes.
    fn score<'a>(&'a self, context: ScoreContext<'a>) -> AttemptScoreFuture<'a>;
}

impl ScorerIdentity {
    /// Constructs a stable scorer identity.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsafe name, empty version, or digest other
    /// than 64 lowercase hexadecimal characters.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        configuration_digest: impl Into<String>,
    ) -> Result<Self, ScorerIdentityError> {
        let name = name.into();
        let version = version.into();
        let configuration_digest = configuration_digest.into();
        let valid_name = name.starts_with(|character: char| character.is_ascii_alphanumeric())
            && name.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
            });
        if !valid_name {
            return Err(ScorerIdentityError::InvalidName);
        }
        if version.is_empty() {
            return Err(ScorerIdentityError::EmptyVersion);
        }
        if configuration_digest.len() != 64
            || !configuration_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ScorerIdentityError::InvalidConfigurationDigest);
        }
        Ok(Self {
            name,
            version,
            configuration_digest,
        })
    }

    /// Returns the filesystem-safe scorer name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the scorer implementation version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the scorer configuration SHA-256 digest.
    #[must_use]
    pub fn configuration_digest(&self) -> &str {
        &self.configuration_digest
    }
}

impl<'a> ScoreContext<'a> {
    pub(crate) const fn new(attempt: EvalAttempt<'a>, canonical: &'a VerifierResult) -> Self {
        Self { attempt, canonical }
    }

    /// Returns the immutable task definition.
    #[must_use]
    pub const fn task(&self) -> &Task {
        self.attempt.task()
    }

    /// Returns the immutable attempt paths and task metadata.
    #[must_use]
    pub const fn attempt(&self) -> EvalAttempt<'a> {
        self.attempt
    }

    /// Returns the canonical verifier result before scorer contributions.
    #[must_use]
    pub const fn canonical(&self) -> &'a VerifierResult {
        self.canonical
    }
}

impl ScoreContribution {
    /// Creates a completed scorer contribution.
    #[must_use]
    pub const fn completed(rewards: BTreeMap<String, f64>) -> Self {
        Self {
            disposition: ScoreDisposition::Completed,
            rewards,
            reason: None,
            artifacts: BTreeMap::new(),
            model_usage: None,
        }
    }

    /// Creates a policy-skipped contribution with explicit fallback rewards.
    #[must_use]
    pub fn skipped(rewards: BTreeMap<String, f64>, reason: impl Into<String>) -> Self {
        Self {
            disposition: ScoreDisposition::Skipped,
            rewards,
            reason: Some(reason.into()),
            artifacts: BTreeMap::new(),
            model_usage: None,
        }
    }

    /// Adds one attempt-relative evidence artifact.
    #[must_use]
    pub fn artifact(mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        self.artifacts.insert(name.into(), path.into());
        self
    }

    /// Retains model usage consumed by this scorer.
    #[must_use]
    pub fn model_usage(mut self, usage: TurnUsage) -> Self {
        self.model_usage = Some(usage);
        self
    }
}

impl ScorerReport {
    pub(crate) fn from_contribution(
        identity: ScorerIdentity,
        started_at: DateTime<Utc>,
        canonical: &mut VerifierResult,
        contribution: ScoreContribution,
    ) -> Result<Self, AttemptScoreError> {
        validate_contribution(canonical, &contribution)?;
        canonical.rewards.extend(contribution.rewards.clone());
        Ok(Self {
            identity,
            status: match contribution.disposition {
                ScoreDisposition::Completed => ScorerStatus::Completed,
                ScoreDisposition::Skipped => ScorerStatus::Skipped,
            },
            rewards: contribution.rewards,
            reason: contribution.reason,
            timing: PhaseTiming::finished(started_at),
            artifacts: contribution.artifacts,
            model_usage: contribution.model_usage,
            diagnostic: None,
        })
    }

    pub(crate) fn failed(
        identity: ScorerIdentity,
        started_at: DateTime<Utc>,
        error: &(dyn Error + 'static),
    ) -> Self {
        Self {
            identity,
            status: ScorerStatus::Failed,
            rewards: BTreeMap::new(),
            reason: None,
            timing: PhaseTiming::finished(started_at),
            artifacts: BTreeMap::new(),
            model_usage: None,
            diagnostic: Some(ScorerDiagnostic {
                message: error.to_string(),
                traceback: format_error_chain(error),
            }),
        }
    }
}

fn validate_contribution(
    canonical: &VerifierResult,
    contribution: &ScoreContribution,
) -> Result<(), AttemptScoreError> {
    if contribution.disposition == ScoreDisposition::Skipped
        && contribution
            .reason
            .as_deref()
            .is_none_or(|reason| reason.trim().is_empty())
    {
        return Err(contract_error("a skipped scorer must provide a reason"));
    }
    for (name, reward) in &contribution.rewards {
        if name.trim().is_empty() {
            return Err(contract_error("scorer reward names must not be empty"));
        }
        if !reward.is_finite() {
            return Err(contract_error(format!(
                "scorer reward {name:?} must be finite"
            )));
        }
        if canonical.rewards.contains_key(name) {
            return Err(contract_error(format!(
                "scorer reward {name:?} collides with a canonical verifier reward"
            )));
        }
    }
    for (name, path) in &contribution.artifacts {
        if name.trim().is_empty()
            || path.as_os_str().is_empty()
            || path.is_absolute()
            || path
                .components()
                .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(contract_error(format!(
                "scorer artifact {name:?} must have a non-empty name and an attempt-relative path"
            )));
        }
    }
    Ok(())
}

fn contract_error(message: impl Into<String>) -> AttemptScoreError {
    Box::new(std::io::Error::other(message.into()))
}

fn format_error_chain(error: &(dyn Error + 'static)) -> String {
    let mut traceback = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        traceback.push_str("\ncaused by: ");
        traceback.push_str(&error.to_string());
        source = error.source();
    }
    traceback
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;

    use super::{ScoreContribution, ScorerIdentity, ScorerReport, ScorerStatus};
    use crate::VerifierResult;

    #[test]
    fn rejects_canonical_reward_collisions_without_mutation() {
        let identity = ScorerIdentity::new("judge", "1", "a".repeat(64)).unwrap();
        let mut verifier = VerifierResult {
            exit_code: 0,
            rewards: BTreeMap::from([("correctness".to_owned(), 1.0)]),
            scorer_reports: Vec::new(),
        };
        let contribution =
            ScoreContribution::completed(BTreeMap::from([("correctness".to_owned(), 0.0)]));

        assert!(
            ScorerReport::from_contribution(identity, Utc::now(), &mut verifier, contribution)
                .is_err()
        );
        assert_eq!(verifier.rewards["correctness"], 1.0);
    }

    #[test]
    fn merges_valid_contribution_and_retains_provenance() {
        let identity = ScorerIdentity::new("judge", "1", "b".repeat(64)).unwrap();
        let mut verifier = VerifierResult {
            exit_code: 0,
            rewards: BTreeMap::from([("correctness".to_owned(), 1.0)]),
            scorer_reports: Vec::new(),
        };
        let contribution =
            ScoreContribution::completed(BTreeMap::from([("quality".to_owned(), 0.75)]))
                .artifact("decision", "verifier/judge.json");

        let report =
            ScorerReport::from_contribution(identity, Utc::now(), &mut verifier, contribution)
                .unwrap();

        assert_eq!(report.status, ScorerStatus::Completed);
        assert_eq!(report.rewards["quality"], 0.75);
        assert_eq!(verifier.rewards["quality"], 0.75);
    }
}
