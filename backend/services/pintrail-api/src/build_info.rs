//! Which build is running, for `/health` and the admin System page.
//!
//! The commit and build time come from the image: CI passes them to
//! `docker build` as `PINTRAIL_GIT_SHA` and `PINTRAIL_BUILT_AT`, and the
//! Dockerfile sets them as environment variables in the runtime stage. They are
//! read at run time rather than compiled in, so a new commit does not force a
//! recompile of the workspace. A binary run outside an image reports "dev".

use std::sync::LazyLock;

use chrono::{DateTime, Utc};
use serde::Serialize;

static STARTED_AT: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);

static COMMIT: LazyLock<String> = LazyLock::new(|| {
    std::env::var("PINTRAIL_GIT_SHA")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dev".to_string())
});

static BUILT_AT: LazyLock<Option<String>> = LazyLock::new(|| {
    std::env::var("PINTRAIL_BUILT_AT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
});

/// Records the process start time. Call once, early in `main`.
pub fn mark_started() {
    LazyLock::force(&STARTED_AT);
}

#[derive(Debug, Clone, Serialize)]
pub struct BuildInfo {
    /// Full commit hash, or "dev".
    pub commit: String,
    /// First ten characters of the commit, as the deploy log prints it.
    pub commit_short: String,
    /// When CI built the image (RFC 3339), if known.
    pub built_at: Option<String>,
    /// When this process started, which is when this version went live.
    pub started_at: DateTime<Utc>,
}

pub fn current() -> BuildInfo {
    let commit = COMMIT.clone();
    BuildInfo {
        commit_short: commit.chars().take(10).collect(),
        commit,
        built_at: BUILT_AT.clone(),
        started_at: *STARTED_AT,
    }
}

/// Whether the commit looks like a real git hash, worth linking to GitHub.
pub fn is_git_sha(commit: &str) -> bool {
    commit.len() >= 7 && commit.chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_git_hashes() {
        assert!(is_git_sha("bc91ca7a7e2f"));
        assert!(!is_git_sha("dev"));
        assert!(!is_git_sha("main"));
    }
}
