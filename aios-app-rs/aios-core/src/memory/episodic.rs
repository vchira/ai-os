//! Episodic memory — structured reflections about past interactions.
//!
//! Instead of storing raw conversation logs, episodic memory stores structured
//! reflections about what happened, what worked, what failed, and what was
//! learned.  This gives the AI "experience" to draw from in future interactions.
//!
//! Episodes are persisted to `~/.aios/memory/episodes.json` and pruned to
//! a configurable maximum count (default 1000).

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use uuid::Uuid;

/// Default maximum number of episodes to keep before pruning.
const DEFAULT_MAX_EPISODES: usize = 1000;

// ---------------------------------------------------------------------------
// Episode types
// ---------------------------------------------------------------------------

/// A single episodic memory — a reflection about something that happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Episode {
    /// Unique identifier (UUID v4).
    pub id: String,
    /// When the episode was recorded.
    pub timestamp: DateTime<Utc>,
    /// What kind of event this was.
    pub category: EpisodeCategory,
    /// Brief summary of what happened (e.g. "Attempted to fix bug in main.py").
    pub summary: String,
    /// Whether the task succeeded, failed, or partially succeeded.
    pub outcome: Outcome,
    /// Longer description of what happened.
    pub details: String,
    /// What the user was trying to accomplish.
    pub context: String,
    /// Lessons learned from this episode.
    #[serde(default)]
    pub lessons: Vec<String>,
    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Files that were involved in this episode.
    #[serde(default)]
    pub related_files: Vec<String>,
}

impl Episode {
    /// Create a new episode with a generated UUID and current timestamp.
    pub fn new(
        category: EpisodeCategory,
        summary: impl Into<String>,
        outcome: Outcome,
        details: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category,
            summary: summary.into(),
            outcome,
            details: details.into(),
            context: context.into(),
            lessons: Vec::new(),
            tags: Vec::new(),
            related_files: Vec::new(),
        }
    }

    /// Builder method: add lessons learned.
    pub fn with_lessons(mut self, lessons: Vec<String>) -> Self {
        self.lessons = lessons;
        self
    }

    /// Builder method: add searchable tags.
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder method: add related files.
    pub fn with_related_files(mut self, files: Vec<String>) -> Self {
        self.related_files = files;
        self
    }
}

/// Category of an episodic memory.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpisodeCategory {
    /// A task the user requested was completed (or attempted).
    TaskCompletion,
    /// An error was encountered and resolved (or not).
    ErrorResolution,
    /// Something was learned about the user's preferences.
    UserPreference,
    /// A system configuration change was made.
    SystemConfiguration,
    /// A file was created, modified, or deleted.
    FileModification,
    /// Web research was performed.
    WebResearch,
    /// A general conversational exchange.
    Conversation,
}

impl EpisodeCategory {
    /// Parse a category from a string (case-insensitive, underscore-separated).
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "task_completion" => Some(Self::TaskCompletion),
            "error_resolution" => Some(Self::ErrorResolution),
            "user_preference" => Some(Self::UserPreference),
            "system_configuration" => Some(Self::SystemConfiguration),
            "file_modification" => Some(Self::FileModification),
            "web_research" => Some(Self::WebResearch),
            "conversation" => Some(Self::Conversation),
            _ => None,
        }
    }

    /// Return a human-readable label for the category.
    pub fn label(&self) -> &str {
        match self {
            Self::TaskCompletion => "task_completion",
            Self::ErrorResolution => "error_resolution",
            Self::UserPreference => "user_preference",
            Self::SystemConfiguration => "system_configuration",
            Self::FileModification => "file_modification",
            Self::WebResearch => "web_research",
            Self::Conversation => "conversation",
        }
    }
}

/// Outcome of an episode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Outcome {
    /// The task completed successfully.
    Success,
    /// The task failed.
    Failure {
        /// Why it failed.
        reason: String,
    },
    /// The task partially succeeded.
    Partial {
        /// What part worked.
        what_worked: String,
        /// What part failed.
        what_failed: String,
    },
}

impl Outcome {
    /// Return a short label for the outcome type.
    pub fn label(&self) -> &str {
        match self {
            Self::Success => "success",
            Self::Failure { .. } => "failure",
            Self::Partial { .. } => "partial",
        }
    }
}

// ---------------------------------------------------------------------------
// EpisodicMemory store
// ---------------------------------------------------------------------------

/// Episodic memory store — persisted to disk as JSON.
///
/// Episodes are loaded from and saved to a JSON file (default:
/// `~/.aios/memory/episodes.json`).  When the number of episodes exceeds
/// `max_episodes`, the oldest episodes are pruned.
pub struct EpisodicMemory {
    episodes: Vec<Episode>,
    path: PathBuf,
    max_episodes: usize,
}

impl EpisodicMemory {
    /// Create a new episodic memory store.
    ///
    /// If the file at `path` exists, episodes are loaded from it.  Otherwise
    /// the store starts empty.
    pub fn new(path: PathBuf) -> Self {
        let mut mem = Self {
            episodes: Vec::new(),
            path,
            max_episodes: DEFAULT_MAX_EPISODES,
        };
        if let Err(e) = mem.load() {
            warn!("Failed to load episodic memory: {e}");
        }
        mem
    }

    /// Create with a custom maximum episode count.
    pub fn with_max_episodes(mut self, max: usize) -> Self {
        self.max_episodes = max;
        self
    }

    /// Return the configured maximum number of episodes.
    pub fn max_episodes(&self) -> usize {
        self.max_episodes
    }

    /// Return the current number of stored episodes.
    pub fn len(&self) -> usize {
        self.episodes.len()
    }

    /// Whether there are no stored episodes.
    pub fn is_empty(&self) -> bool {
        self.episodes.is_empty()
    }

    /// Return the storage path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Add a new episode and save to disk.
    ///
    /// Automatically prunes if the store exceeds `max_episodes`.
    pub fn add(&mut self, episode: Episode) -> Result<(), String> {
        debug!(id = %episode.id, summary = %episode.summary, "adding episode");
        self.episodes.push(episode);
        self.prune();
        self.save()
    }

    /// Search episodes by keyword across summary, details, tags, and context.
    ///
    /// The query is matched case-insensitively.  Returns matching episodes
    /// sorted by timestamp (most recent first).
    pub fn search(&self, query: &str) -> Vec<&Episode> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<&Episode> = self
            .episodes
            .iter()
            .filter(|ep| {
                ep.summary.to_lowercase().contains(&query_lower)
                    || ep.details.to_lowercase().contains(&query_lower)
                    || ep.context.to_lowercase().contains(&query_lower)
                    || ep.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
                    || ep.lessons.iter().any(|l| l.to_lowercase().contains(&query_lower))
            })
            .collect();
        results.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        results
    }

    /// Return the most recent N episodes.
    pub fn recent(&self, n: usize) -> Vec<&Episode> {
        let mut sorted: Vec<&Episode> = self.episodes.iter().collect();
        sorted.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        sorted.into_iter().take(n).collect()
    }

    /// Return all episodes matching a given category.
    pub fn by_category(&self, cat: &EpisodeCategory) -> Vec<&Episode> {
        self.episodes.iter().filter(|ep| &ep.category == cat).collect()
    }

    /// Return all episodes matching an outcome type label ("success", "failure", "partial").
    pub fn by_outcome(&self, outcome_type: &str) -> Vec<&Episode> {
        self.episodes
            .iter()
            .filter(|ep| ep.outcome.label() == outcome_type)
            .collect()
    }

    /// Return all episodes involving a given file path (substring match).
    pub fn related_to_file(&self, path: &str) -> Vec<&Episode> {
        self.episodes
            .iter()
            .filter(|ep| ep.related_files.iter().any(|f| f.contains(path)))
            .collect()
    }

    /// Save episodes to disk as pretty-printed JSON.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create episodes directory: {e}"))?;
        }
        let json = serde_json::to_string_pretty(&self.episodes)
            .map_err(|e| format!("Failed to serialize episodes: {e}"))?;
        fs::write(&self.path, json)
            .map_err(|e| format!("Failed to write episodes file: {e}"))?;
        debug!(count = self.episodes.len(), "saved episodes");
        Ok(())
    }

    /// Load episodes from disk.
    pub fn load(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            self.episodes = Vec::new();
            return Ok(());
        }
        let text = fs::read_to_string(&self.path)
            .map_err(|e| format!("Failed to read episodes file: {e}"))?;
        if text.trim().is_empty() {
            self.episodes = Vec::new();
            return Ok(());
        }
        self.episodes = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse episodes JSON: {e}"))?;
        debug!(count = self.episodes.len(), "loaded episodes");
        Ok(())
    }

    /// Prune episodes to keep only `max_episodes`, removing the oldest first.
    pub fn prune(&mut self) {
        if self.episodes.len() > self.max_episodes {
            // Sort by timestamp ascending so oldest come first.
            self.episodes.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
            let excess = self.episodes.len() - self.max_episodes;
            self.episodes.drain(0..excess);
            debug!(
                removed = excess,
                remaining = self.episodes.len(),
                "pruned episodes"
            );
        }
    }

    /// Format the most recent N episodes as a context block for the LLM system prompt.
    ///
    /// Returns a multi-line string like:
    /// ```text
    /// Recent experiences:
    /// - [2h ago] Successfully configured SSH keys (task_completion)
    /// - [yesterday] Failed to install package X (error_resolution, lesson: check deps first)
    /// ```
    pub fn to_context_string(&self, n: usize) -> String {
        let recent = self.recent(n);
        if recent.is_empty() {
            return String::new();
        }

        let now = Utc::now();
        let mut lines = vec!["Recent experiences:".to_string()];

        for ep in &recent {
            let age = format_age(now, ep.timestamp);
            let outcome_str = match &ep.outcome {
                Outcome::Success => String::new(),
                Outcome::Failure { reason } => format!(", failed: {reason}"),
                Outcome::Partial { what_worked, what_failed } => {
                    format!(", partial: worked={what_worked}, failed={what_failed}")
                }
            };
            let lessons_str = if ep.lessons.is_empty() {
                String::new()
            } else {
                format!(", lessons: {}", ep.lessons.join("; "))
            };
            lines.push(format!(
                "- [{age}] {summary} ({category}{outcome}{lessons})",
                summary = ep.summary,
                category = ep.category.label(),
                outcome = outcome_str,
                lessons = lessons_str,
            ));
        }

        lines.join("\n")
    }
}

/// Format the age of a timestamp relative to now in human-readable form.
fn format_age(now: DateTime<Utc>, then: DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(then);
    let secs = duration.num_seconds();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        let mins = secs / 60;
        format!("{mins}m ago")
    } else if secs < 86400 {
        let hours = secs / 3600;
        format!("{hours}h ago")
    } else if secs < 172800 {
        "yesterday".to_string()
    } else {
        let days = secs / 86400;
        format!("{days}d ago")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_memory() -> (EpisodicMemory, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory").join("episodes.json");
        let mem = EpisodicMemory::new(path);
        (mem, dir)
    }

    fn sample_episode(summary: &str, category: EpisodeCategory, outcome: Outcome) -> Episode {
        Episode::new(category, summary, outcome, "details here", "user wanted X")
    }

    // -- CRUD tests -------------------------------------------------------

    #[test]
    fn add_and_count() {
        let (mut mem, _dir) = temp_memory();
        assert!(mem.is_empty());

        let ep = sample_episode("did thing", EpisodeCategory::TaskCompletion, Outcome::Success);
        mem.add(ep).unwrap();

        assert_eq!(mem.len(), 1);
        assert!(!mem.is_empty());
    }

    #[test]
    fn add_persists_to_disk() {
        let (mut mem, _dir) = temp_memory();
        let ep = sample_episode("persisted", EpisodeCategory::Conversation, Outcome::Success);
        let path = mem.path().to_path_buf();
        mem.add(ep).unwrap();

        // Reload from disk.
        let mut mem2 = EpisodicMemory::new(path);
        mem2.load().unwrap();
        assert_eq!(mem2.len(), 1);
        assert_eq!(mem2.recent(1)[0].summary, "persisted");
    }

    #[test]
    fn recent_returns_newest_first() {
        let (mut mem, _dir) = temp_memory();
        for i in 0..5 {
            let mut ep = sample_episode(
                &format!("episode {i}"),
                EpisodeCategory::TaskCompletion,
                Outcome::Success,
            );
            // Stagger timestamps.
            ep.timestamp = Utc::now() - chrono::Duration::seconds(100 - i * 10);
            mem.add(ep).unwrap();
        }

        let recent = mem.recent(3);
        assert_eq!(recent.len(), 3);
        // Most recent first (highest index = most recent timestamp).
        assert!(recent[0].summary.contains('4'));
        assert!(recent[1].summary.contains('3'));
        assert!(recent[2].summary.contains('2'));
    }

    // -- Search tests -----------------------------------------------------

    #[test]
    fn search_by_summary() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("fix login bug", EpisodeCategory::ErrorResolution, Outcome::Success)).unwrap();
        mem.add(sample_episode("update readme", EpisodeCategory::FileModification, Outcome::Success)).unwrap();

        let results = mem.search("login");
        assert_eq!(results.len(), 1);
        assert!(results[0].summary.contains("login"));
    }

    #[test]
    fn search_by_tag() {
        let (mut mem, _dir) = temp_memory();
        let ep = sample_episode("set up ssh", EpisodeCategory::SystemConfiguration, Outcome::Success)
            .with_tags(vec!["ssh".into(), "security".into()]);
        mem.add(ep).unwrap();

        let results = mem.search("security");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_by_lesson() {
        let (mut mem, _dir) = temp_memory();
        let ep = sample_episode("install failed", EpisodeCategory::ErrorResolution, Outcome::Failure {
            reason: "missing dep".into(),
        })
        .with_lessons(vec!["always check dependencies first".into()]);
        mem.add(ep).unwrap();

        let results = mem.search("dependencies");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_case_insensitive() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("Configure SSH Keys", EpisodeCategory::SystemConfiguration, Outcome::Success)).unwrap();

        let results = mem.search("ssh keys");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn search_no_results() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("hello world", EpisodeCategory::Conversation, Outcome::Success)).unwrap();

        let results = mem.search("nonexistent");
        assert!(results.is_empty());
    }

    // -- Filter tests -----------------------------------------------------

    #[test]
    fn by_category_filters() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("task a", EpisodeCategory::TaskCompletion, Outcome::Success)).unwrap();
        mem.add(sample_episode("error b", EpisodeCategory::ErrorResolution, Outcome::Failure {
            reason: "bug".into(),
        })).unwrap();
        mem.add(sample_episode("task c", EpisodeCategory::TaskCompletion, Outcome::Success)).unwrap();

        let tasks = mem.by_category(&EpisodeCategory::TaskCompletion);
        assert_eq!(tasks.len(), 2);

        let errors = mem.by_category(&EpisodeCategory::ErrorResolution);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn by_outcome_filters() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("ok", EpisodeCategory::TaskCompletion, Outcome::Success)).unwrap();
        mem.add(sample_episode("bad", EpisodeCategory::TaskCompletion, Outcome::Failure {
            reason: "oops".into(),
        })).unwrap();
        mem.add(sample_episode("meh", EpisodeCategory::TaskCompletion, Outcome::Partial {
            what_worked: "part a".into(),
            what_failed: "part b".into(),
        })).unwrap();

        assert_eq!(mem.by_outcome("success").len(), 1);
        assert_eq!(mem.by_outcome("failure").len(), 1);
        assert_eq!(mem.by_outcome("partial").len(), 1);
        assert_eq!(mem.by_outcome("unknown").len(), 0);
    }

    #[test]
    fn related_to_file_filters() {
        let (mut mem, _dir) = temp_memory();
        let ep = sample_episode("edit main", EpisodeCategory::FileModification, Outcome::Success)
            .with_related_files(vec!["/home/user/main.py".into(), "/home/user/utils.py".into()]);
        mem.add(ep).unwrap();
        mem.add(sample_episode("other", EpisodeCategory::Conversation, Outcome::Success)).unwrap();

        let results = mem.related_to_file("main.py");
        assert_eq!(results.len(), 1);

        let results = mem.related_to_file("utils.py");
        assert_eq!(results.len(), 1);

        let results = mem.related_to_file("nonexistent.py");
        assert!(results.is_empty());
    }

    // -- Pruning tests ----------------------------------------------------

    #[test]
    fn prune_removes_oldest() {
        let (mem, _dir) = temp_memory();
        let mut mem = mem.with_max_episodes(3);

        for i in 0..5 {
            let mut ep = sample_episode(
                &format!("ep {i}"),
                EpisodeCategory::Conversation,
                Outcome::Success,
            );
            ep.timestamp = Utc::now() - chrono::Duration::seconds(50 - i * 10);
            mem.add(ep).unwrap();
        }

        assert_eq!(mem.len(), 3);
        // Oldest (ep 0, ep 1) should have been pruned.
        let summaries: Vec<&str> = mem.recent(5).iter().map(|e| e.summary.as_str()).collect();
        assert!(!summaries.contains(&"ep 0"));
        assert!(!summaries.contains(&"ep 1"));
        assert!(summaries.contains(&"ep 2"));
        assert!(summaries.contains(&"ep 3"));
        assert!(summaries.contains(&"ep 4"));
    }

    #[test]
    fn prune_noop_when_under_limit() {
        let (mut mem, _dir) = temp_memory();
        mem.add(sample_episode("a", EpisodeCategory::Conversation, Outcome::Success)).unwrap();
        mem.add(sample_episode("b", EpisodeCategory::Conversation, Outcome::Success)).unwrap();
        mem.prune();
        assert_eq!(mem.len(), 2);
    }

    // -- Serialization tests ----------------------------------------------

    #[test]
    fn episode_roundtrips_json() {
        let ep = Episode::new(
            EpisodeCategory::ErrorResolution,
            "fixed the bug",
            Outcome::Failure { reason: "syntax error".into() },
            "detailed explanation",
            "user asked to fix login",
        )
        .with_lessons(vec!["check syntax".into()])
        .with_tags(vec!["bug".into(), "login".into()])
        .with_related_files(vec!["/home/user/login.py".into()]);

        let json = serde_json::to_string(&ep).unwrap();
        let back: Episode = serde_json::from_str(&json).unwrap();

        assert_eq!(back.summary, "fixed the bug");
        assert_eq!(back.category, EpisodeCategory::ErrorResolution);
        assert_eq!(back.lessons.len(), 1);
        assert_eq!(back.tags.len(), 2);
        assert_eq!(back.related_files.len(), 1);
    }

    #[test]
    fn outcome_variants_serialize() {
        let success = Outcome::Success;
        let failure = Outcome::Failure { reason: "oops".into() };
        let partial = Outcome::Partial {
            what_worked: "part a".into(),
            what_failed: "part b".into(),
        };

        // Round-trip each variant.
        for outcome in [success, failure, partial] {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: Outcome = serde_json::from_str(&json).unwrap();
            assert_eq!(outcome, back);
        }
    }

    #[test]
    fn category_from_str_opt() {
        assert_eq!(
            EpisodeCategory::from_str_opt("task_completion"),
            Some(EpisodeCategory::TaskCompletion),
        );
        assert_eq!(
            EpisodeCategory::from_str_opt("ERROR_RESOLUTION"),
            Some(EpisodeCategory::ErrorResolution),
        );
        assert_eq!(EpisodeCategory::from_str_opt("invalid"), None);
    }

    // -- Context string tests ---------------------------------------------

    #[test]
    fn to_context_string_empty() {
        let (mem, _dir) = temp_memory();
        assert_eq!(mem.to_context_string(5), "");
    }

    #[test]
    fn to_context_string_formats_correctly() {
        let (mut mem, _dir) = temp_memory();
        let ep = Episode::new(
            EpisodeCategory::TaskCompletion,
            "configured SSH keys",
            Outcome::Success,
            "set up SSH for GitHub",
            "user wanted to push code",
        );
        mem.add(ep).unwrap();

        let ctx = mem.to_context_string(5);
        assert!(ctx.starts_with("Recent experiences:"));
        assert!(ctx.contains("configured SSH keys"));
        assert!(ctx.contains("task_completion"));
    }

    #[test]
    fn to_context_string_includes_failure_reason() {
        let (mut mem, _dir) = temp_memory();
        let ep = Episode::new(
            EpisodeCategory::ErrorResolution,
            "install failed",
            Outcome::Failure { reason: "missing dep".into() },
            "tried to install",
            "user wanted package X",
        );
        mem.add(ep).unwrap();

        let ctx = mem.to_context_string(5);
        assert!(ctx.contains("missing dep"));
    }

    #[test]
    fn to_context_string_includes_lessons() {
        let (mut mem, _dir) = temp_memory();
        let ep = Episode::new(
            EpisodeCategory::ErrorResolution,
            "resolved issue",
            Outcome::Success,
            "fixed it",
            "debugging",
        )
        .with_lessons(vec!["check deps first".into()]);
        mem.add(ep).unwrap();

        let ctx = mem.to_context_string(5);
        assert!(ctx.contains("check deps first"));
    }

    // -- Format age tests -------------------------------------------------

    #[test]
    fn format_age_just_now() {
        let now = Utc::now();
        assert_eq!(format_age(now, now), "just now");
    }

    #[test]
    fn format_age_minutes() {
        let now = Utc::now();
        let then = now - chrono::Duration::seconds(300);
        assert_eq!(format_age(now, then), "5m ago");
    }

    #[test]
    fn format_age_hours() {
        let now = Utc::now();
        let then = now - chrono::Duration::seconds(7200);
        assert_eq!(format_age(now, then), "2h ago");
    }

    #[test]
    fn format_age_yesterday() {
        let now = Utc::now();
        let then = now - chrono::Duration::seconds(100_000);
        assert_eq!(format_age(now, then), "yesterday");
    }

    #[test]
    fn format_age_days() {
        let now = Utc::now();
        let then = now - chrono::Duration::seconds(259_200);
        assert_eq!(format_age(now, then), "3d ago");
    }

    // -- Load from empty/missing file ------------------------------------

    #[test]
    fn load_from_nonexistent_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let mem = EpisodicMemory::new(path);
        assert!(mem.is_empty());
    }

    #[test]
    fn load_from_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.json");
        fs::write(&path, "").unwrap();
        let mem = EpisodicMemory::new(path);
        assert!(mem.is_empty());
    }

    #[test]
    fn max_episodes_configurable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("episodes.json");
        let mem = EpisodicMemory::new(path).with_max_episodes(50);
        assert_eq!(mem.max_episodes(), 50);
    }
}
