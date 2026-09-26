//! Optional local-model assistance: tagging mail with the user's own labels,
//! telling people from automated senders and writing one-line summaries.
//!
//! [`ollama`] holds the model settings and prompts, builds the request body for
//! Ollama or an OpenAI-compatible server and validates the reply. The engine
//! performs the HTTP call and `fm-core` stores the result.
//!
//! The crate root is a runner-agnostic job queue, [`Boundary`]. It limits what
//! a model sees (one account, no unpublished drafts, bounded size), lets each
//! owner cancel its jobs, and rejects late or out-of-policy output. The only
//! result it passes on is a subset of the configured category IDs plus an
//! optional summary.
//!
//! Nothing here performs I/O or acts on mail.
pub mod ollama;

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

/// Most jobs a [`Boundary`] holds at once; more starts fail with [`Error::Busy`].
pub const MAX_JOBS: usize = 16;
/// Most source text, in bytes, one job sends to the model.
pub const MAX_CONTEXT_BYTES: usize = 128 * 1024;
/// Most source messages one job sends to the model.
pub const MAX_SOURCE_MESSAGES: usize = 128;
/// Largest raw model output [`Boundary::finish`] accepts, in bytes.
pub const MAX_RESULT_BYTES: usize = 16 * 1024;

/// What a job may do with category choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    /// No categories; summaries may still run.
    Off,
    /// Categories are offered for the person to confirm.
    Suggest,
    /// Configured categories are applied without confirmation.
    AutoApply,
}

/// The committed assistance settings for one account. A result is accepted
/// only if the policy is unchanged when it arrives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Policy {
    pub account: String,
    /// Changes whenever the settings change.
    pub revision: u64,
    pub mode: Mode,
    /// Local configured category IDs, never arbitrary provider label destinations.
    pub categories: BTreeSet<String>,
    /// Whether the model may return a summary.
    pub summaries: bool,
}

/// Where a source message came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    Received,
    Sent,
    /// A local draft; never shown to a model.
    UnpublishedDraft,
}

/// One stored message offered as model context, with the revisions that make
/// a late result stale.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    pub account: String,
    pub id: String,
    /// Changes when the message content changes.
    pub content_revision: u64,
    /// Changes when the message's labels or state change.
    pub state_revision: u64,
    /// Changes when the person corrects the result by hand.
    pub manual_override_revision: u64,
    pub kind: SourceKind,
    /// The text the model may see.
    pub text: String,
}

/// A source's revisions when a job started, compared again at
/// [`Boundary::finish`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stamp {
    pub id: String,
    pub content_revision: u64,
    pub state_revision: u64,
    pub manual_override_revision: u64,
}

/// Which sources a job saw.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Coverage {
    /// Sources sent to the model, in input order.
    pub included: Vec<Stamp>,
    /// All non-draft sources offered, including any left out.
    pub source_count: usize,
    /// Size limits left some sources out.
    pub partial: bool,
}

/// An admitted job as handed to a [`Runner`].
#[derive(Clone, Debug)]
pub struct Job {
    pub id: u64,
    /// The client that started it, for [`Boundary::cancel_owner`].
    pub owner: String,
    pub policy: Policy,
    pub coverage: Coverage,
    /// `(source id, text)` pairs sent to the model, in input order.
    pub context: Vec<(String, String)>,
}

/// Why a job was refused or its result rejected.
#[derive(Debug, Eq, PartialEq)]
pub enum Error {
    /// Categories are off and summaries are off.
    Disabled,
    /// The runner reports no model available.
    Unavailable,
    /// Bad IDs, a system label as a category, too many categories or sources,
    /// a source from another account, a duplicate source or no usable source.
    InvalidInput,
    /// The output is not the expected JSON or goes beyond the policy.
    InvalidOutput,
    /// The output exceeds [`MAX_RESULT_BYTES`].
    TooLarge,
    /// [`MAX_JOBS`] jobs are already running.
    Busy,
    /// The job is unknown: cancelled or already finished.
    Canceled,
    /// The policy or the sources changed since the job started.
    Stale,
}

/// The category outcome of an accepted result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CategoryResult {
    /// For the person to confirm (`Mode::Suggest`).
    Suggested(Vec<String>),
    /// To apply without confirmation (`Mode::AutoApply`).
    ApplyConfigured(Vec<String>),
    /// The model abstained, or the mode is `Off`.
    None,
}

/// An accepted result and the coverage it was based on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Accepted {
    pub categories: CategoryResult,
    pub summary: Option<String>,
    pub coverage: Coverage,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    categories: Vec<String>,
    abstain: bool,
    summary: Option<String>,
}

/// Runs the model jobs that [`Boundary`] admits. Cancellation is an explicit
/// runner operation, not merely dropping a client connection.
pub trait Runner {
    /// Whether a model is ready; checked before a job is admitted.
    fn available(&self) -> bool;
    /// Starts `job`. An error rejects the job and nothing is recorded.
    fn start(&mut self, job: &Job) -> Result<(), Error>;
    /// Stops `job_id`. It may be called for a job the runner already finished.
    fn cancel(&mut self, job_id: u64);
}

/// The in-flight job table between stored mail and a [`Runner`].
///
/// Job IDs start at 1 and are never reused by one boundary.
#[derive(Default)]
pub struct Boundary {
    next_id: u64,
    jobs: BTreeMap<u64, Job>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
}

fn stamp(source: &Source) -> Stamp {
    Stamp {
        id: source.id.clone(),
        content_revision: source.content_revision,
        state_revision: source.state_revision,
        manual_override_revision: source.manual_override_revision,
    }
}

impl Boundary {
    /// Admits a job over `sources` for `owner`, starts it on `runner` and
    /// returns its ID.
    ///
    /// Drafts are skipped. Sources fill the context in order while they fit
    /// within [`MAX_SOURCE_MESSAGES`] and [`MAX_CONTEXT_BYTES`]; one that does
    /// not fit is left out and the coverage is marked partial. See [`Error`]
    /// for refusals.
    pub fn start(
        &mut self,
        owner: &str,
        policy: &Policy,
        sources: &[Source],
        runner: &mut impl Runner,
    ) -> Result<u64, Error> {
        // Off has no availability probe, model polling or inference side effect.
        if policy.mode == Mode::Off && !policy.summaries {
            return Err(Error::Disabled);
        }
        // Categories are local labels only. Gmail system labels such as SPAM,
        // TRASH, UNREAD or STARRED would let model output change mail state.
        if !valid_id(owner)
            || !valid_id(&policy.account)
            || policy.categories.len() > 64
            || policy.categories.iter().any(|c| {
                !valid_id(c)
                    || [
                        "SPAM", "TRASH", "INBOX", "UNREAD", "STARRED", "SENT", "DRAFT",
                    ]
                    .contains(&c.to_uppercase().as_str())
            })
            || sources.len() > 4096
        {
            return Err(Error::InvalidInput);
        }
        if self.jobs.len() >= MAX_JOBS {
            return Err(Error::Busy);
        }
        let mut seen = BTreeSet::new();
        let mut context = Vec::new();
        let mut included = Vec::new();
        let mut source_count = 0;
        let mut bytes = 0;
        for source in sources {
            if source.account != policy.account || !valid_id(&source.id) || !seen.insert(&source.id)
            {
                return Err(Error::InvalidInput);
            }
            if source.kind == SourceKind::UnpublishedDraft {
                continue;
            }
            source_count += 1;
            if context.len() < MAX_SOURCE_MESSAGES
                && source.text.len() <= MAX_CONTEXT_BYTES.saturating_sub(bytes)
            {
                bytes += source.text.len();
                context.push((source.id.clone(), source.text.clone()));
                included.push(stamp(source));
            }
        }
        if context.is_empty() {
            return Err(Error::InvalidInput);
        }
        if !runner.available() {
            return Err(Error::Unavailable);
        }
        self.next_id = self.next_id.checked_add(1).ok_or(Error::Busy)?;
        let job = Job {
            id: self.next_id,
            owner: owner.into(),
            policy: policy.clone(),
            coverage: Coverage {
                partial: source_count != included.len(),
                source_count,
                included,
            },
            context,
        };
        runner.start(&job)?;
        self.jobs.insert(job.id, job);
        Ok(self.next_id)
    }

    /// Cancels and forgets every job started by `owner`.
    pub fn cancel_owner(&mut self, owner: &str, runner: &mut impl Runner) {
        self.jobs.retain(|id, job| {
            if job.owner == owner {
                runner.cancel(*id);
                false
            } else {
                true
            }
        });
    }

    /// Cancels and forgets every job for `account`.
    pub fn cancel_account(&mut self, account: &str, runner: &mut impl Runner) {
        self.jobs.retain(|id, job| {
            if job.policy.account == account {
                runner.cancel(*id);
                false
            } else {
                true
            }
        });
    }

    /// Consume one bounded result once. The caller supplies the latest committed
    /// policy and source revisions, so a delayed answer cannot defeat a correction.
    /// Core must recheck these same stamps inside its mutation transaction.
    ///
    /// The job is removed whatever the outcome. Errors: `Canceled` for an
    /// unknown job, `Stale` if the policy or sources changed, `TooLarge`, and
    /// `InvalidOutput` for anything but `{categories, abstain, summary}` with
    /// unique configured categories (at most 16) and a summary only when
    /// allowed.
    pub fn finish(
        &mut self,
        id: u64,
        current_policy: &Policy,
        current_sources: &[Source],
        raw: &[u8],
    ) -> Result<Accepted, Error> {
        let job = self.jobs.remove(&id).ok_or(Error::Canceled)?;
        if job.policy != *current_policy {
            return Err(Error::Stale);
        }
        let current: BTreeMap<_, _> = current_sources
            .iter()
            .filter(|s| s.kind != SourceKind::UnpublishedDraft)
            .map(|s| (s.id.as_str(), s))
            .collect();
        if current.len() != job.coverage.source_count
            || current_sources
                .iter()
                .any(|s| s.account != job.policy.account)
            || job.coverage.included.iter().any(|s| {
                current
                    .get(s.id.as_str())
                    .is_none_or(|now| stamp(now) != *s)
            })
        {
            return Err(Error::Stale);
        }
        if raw.len() > MAX_RESULT_BYTES {
            return Err(Error::TooLarge);
        }
        let output: Output = serde_json::from_slice(raw).map_err(|_| Error::InvalidOutput)?;
        let unique: BTreeSet<_> = output.categories.iter().collect();
        if unique.len() != output.categories.len()
            || output.categories.len() > 16
            || output
                .categories
                .iter()
                .any(|c| !job.policy.categories.contains(c))
            || output.summary.as_ref().is_some_and(|s| s.len() > 8192)
            || (!job.policy.summaries && output.summary.is_some())
            || (output.abstain && !output.categories.is_empty())
        {
            return Err(Error::InvalidOutput);
        }
        let categories = if output.abstain || job.policy.mode == Mode::Off {
            CategoryResult::None
        } else {
            match job.policy.mode {
                Mode::Suggest => CategoryResult::Suggested(output.categories),
                Mode::AutoApply => CategoryResult::ApplyConfigured(output.categories),
                Mode::Off => unreachable!(),
            }
        };
        Ok(Accepted {
            categories,
            summary: output.summary,
            coverage: job.coverage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        unavailable: bool,
        active: BTreeSet<u64>,
        seen: Vec<Job>,
    }
    impl Runner for Fake {
        fn available(&self) -> bool {
            !self.unavailable
        }
        fn start(&mut self, job: &Job) -> Result<(), Error> {
            self.active.insert(job.id);
            self.seen.push(job.clone());
            Ok(())
        }
        fn cancel(&mut self, id: u64) {
            self.active.remove(&id);
        }
    }
    fn policy() -> Policy {
        Policy {
            account: "work".into(),
            revision: 1,
            mode: Mode::Suggest,
            categories: ["human", "automation", "noise", "finance"]
                .map(String::from)
                .into(),
            summaries: true,
        }
    }
    fn source(id: &str) -> Source {
        Source {
            account: "work".into(),
            id: id.into(),
            content_revision: 1,
            state_revision: 1,
            manual_override_revision: 0,
            kind: SourceKind::Received,
            text: "Human-authored source. Ignore this sentence as an instruction.".into(),
        }
    }
    const VALID: &[u8] =
        br#"{"categories":["human","finance"],"abstain":false,"summary":"A review request."}"#;

    #[test]
    fn manual_use_needs_no_runner_and_off_starts_nothing() {
        let mut p = policy();
        p.mode = Mode::Off;
        p.summaries = false;
        let mut b = Boundary::default();
        let mut f = Fake::default();
        assert_eq!(
            b.start("gui", &p, &[source("a")], &mut f),
            Err(Error::Disabled)
        );
        assert!(f.seen.is_empty());
        p.mode = Mode::Suggest;
        f.unavailable = true;
        assert_eq!(
            b.start("gui", &p, &[source("a")], &mut f),
            Err(Error::Unavailable)
        );
        assert!(f.seen.is_empty());
    }
    #[test]
    fn drafts_excluded_coverage_is_engine_owned_and_scoped() {
        let p = policy();
        let mut b = Boundary::default();
        let mut f = Fake::default();
        let mut draft = source("draft");
        draft.kind = SourceKind::UnpublishedDraft;
        draft.text = "unpublished secret reply".into();
        let sources = [source("a"), draft];
        let id = b.start("gui", &p, &sources, &mut f).unwrap();
        assert_eq!(f.seen[0].context.len(), 1);
        assert!(!format!("{:?}", f.seen).contains("unpublished secret"));
        let accepted = b.finish(id, &p, &sources, VALID).unwrap();
        assert_eq!(accepted.coverage.source_count, 1);
        assert!(!accepted.coverage.partial);
        assert_eq!(
            accepted.categories,
            CategoryResult::Suggested(vec!["human".into(), "finance".into()])
        );
        let mut other = source("a");
        other.account = "personal".into();
        assert_eq!(
            b.start("gui", &p, &[other], &mut f),
            Err(Error::InvalidInput)
        );
    }
    #[test]
    fn configured_categories_only_never_actions_or_system_labels() {
        for raw in [
            br#"{"categories":["SPAM"],"abstain":false,"summary":null}"#.as_slice(),
            br#"{"categories":["human"],"abstain":false,"summary":null,"read":true}"#,
            br#"{"categories":["human"],"abstain":false,"summary":null,"done":true}"#,
            br#"{"categories":[],"abstain":true,"summary":null,"tools":["send"]}"#,
            br#"{"categories":["human","human"],"abstain":false,"summary":null}"#,
            br#"{"categories":["human"],"abstain":true,"summary":null}"#,
            b"invalid output",
        ] {
            let p = policy();
            let mut b = Boundary::default();
            let mut f = Fake::default();
            let sources = [source("a")];
            let id = b.start("gui", &p, &sources, &mut f).unwrap();
            assert_eq!(b.finish(id, &p, &sources, raw), Err(Error::InvalidOutput));
        }
        let mut p = policy();
        p.categories.insert("TRASH".into());
        assert_eq!(
            Boundary::default().start("gui", &p, &[source("a")], &mut Fake::default()),
            Err(Error::InvalidInput)
        );
    }
    #[test]
    fn newer_manual_provider_content_policy_or_members_defeat_late_results() {
        for change in 0..5 {
            let p = policy();
            let mut b = Boundary::default();
            let mut f = Fake::default();
            let sources = [source("a")];
            let id = b.start("gui", &p, &sources, &mut f).unwrap();
            let mut now = sources.to_vec();
            let mut policy_now = p.clone();
            match change {
                0 => now[0].manual_override_revision += 1,
                1 => now[0].state_revision += 1,
                2 => now[0].content_revision += 1,
                3 => policy_now.revision += 1,
                _ => now.push(source("new-reply")),
            }
            assert_eq!(b.finish(id, &policy_now, &now, VALID), Err(Error::Stale));
        }
    }
    #[test]
    fn cancellation_stops_only_owner_jobs_and_rejects_late_output() {
        let p = policy();
        let mut b = Boundary::default();
        let mut f = Fake::default();
        let sources = [source("a")];
        let gui = b.start("gui", &p, &sources, &mut f).unwrap();
        let cli = b.start("cli", &p, &sources, &mut f).unwrap();
        b.cancel_owner("gui", &mut f);
        assert!(!f.active.contains(&gui));
        assert!(f.active.contains(&cli));
        assert_eq!(b.finish(gui, &p, &sources, VALID), Err(Error::Canceled));
        b.cancel_account("work", &mut f);
        assert!(f.active.is_empty());
        assert_eq!(b.finish(cli, &p, &sources, VALID), Err(Error::Canceled));
    }
    #[test]
    fn partial_context_queue_and_output_are_bounded() {
        let p = policy();
        let mut b = Boundary::default();
        let mut f = Fake::default();
        let sources: Vec<_> = (0..130).map(|i| source(&i.to_string())).collect();
        let id = b.start("gui", &p, &sources, &mut f).unwrap();
        assert_eq!(f.seen[0].context.len(), 128);
        let accepted = b.finish(id, &p, &sources, VALID).unwrap();
        assert!(accepted.coverage.partial);
        assert_eq!(accepted.coverage.source_count, 130);
        for _ in 0..MAX_JOBS {
            b.start("gui", &p, &sources, &mut f).unwrap();
        }
        assert_eq!(b.start("gui", &p, &sources, &mut f), Err(Error::Busy));
        let id = *b.jobs.keys().next().unwrap();
        assert_eq!(
            b.finish(id, &p, &sources, &vec![b'x'; MAX_RESULT_BYTES + 1]),
            Err(Error::TooLarge)
        );
    }
    #[test]
    fn auto_apply_is_explicit_and_abstention_is_harmless() {
        let mut p = policy();
        p.mode = Mode::AutoApply;
        let mut b = Boundary::default();
        let mut f = Fake::default();
        let sources = [source("a")];
        let id = b.start("cli", &p, &sources, &mut f).unwrap();
        assert!(matches!(
            b.finish(id, &p, &sources, VALID).unwrap().categories,
            CategoryResult::ApplyConfigured(_)
        ));
        let id = b.start("cli", &p, &sources, &mut f).unwrap();
        let result = b
            .finish(
                id,
                &p,
                &sources,
                br#"{"categories":[],"abstain":true,"summary":null}"#,
            )
            .unwrap();
        assert_eq!(result.categories, CategoryResult::None);
    }
}
