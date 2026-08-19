//! The GTFS specification revision and canonical-validator release this build
//! is aligned with.
//!
//! `spec_baseline.json` is the single source of truth. Reports quote it so a
//! stored report says which upstream state it was produced against, and
//! `scripts/spec_watch.py` diffs upstream against it. Moving the baseline is the
//! deliberate act of accepting a new upstream state; `docs/spec-watch.md`
//! describes the protocol.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// The committed baseline document, embedded so normal builds stay hermetic.
pub const SPEC_BASELINE_JSON: &str = include_str!("../spec_baseline.json");

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpecRevision {
    pub repository: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub commit: String,
    #[serde(rename = "committedAt")]
    pub committed_at: String,
    #[serde(rename = "specPaths")]
    pub spec_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CanonicalBaseline {
    pub repository: String,
    pub version: String,
    #[serde(rename = "publishedAt")]
    pub published_at: String,
    #[serde(rename = "rulesAsset")]
    pub rules_asset: String,
}

/// Only the fields the validator itself needs; the watcher's `acknowledged`
/// bookkeeping lives in the same file but is of no interest to a build.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpecBaseline {
    #[serde(rename = "specRevision")]
    pub spec_revision: SpecRevision,
    #[serde(rename = "canonicalBaseline")]
    pub canonical_baseline: CanonicalBaseline,
}

pub fn spec_baseline() -> &'static SpecBaseline {
    static BASELINE: OnceLock<SpecBaseline> = OnceLock::new();
    BASELINE.get_or_init(|| {
        serde_json::from_str(SPEC_BASELINE_JSON).expect("bundled spec baseline must be valid JSON")
    })
}

/// `google/transit@<commit>`: the spec revision reports are aligned with.
pub fn spec_revision_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let revision = &spec_baseline().spec_revision;
        format!("{}@{}", revision.repository, revision.commit)
    })
}

/// `MobilityData/gtfs-validator@<tag>`: the canonical release reports are
/// aligned with.
pub fn canonical_baseline_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let baseline = &spec_baseline().canonical_baseline;
        format!("{}@{}", baseline.repository, baseline.version)
    })
}

#[cfg(test)]
mod tests {
    use super::{canonical_baseline_id, spec_baseline, spec_revision_id};

    #[test]
    fn parses_the_bundled_baseline() {
        let baseline = spec_baseline();

        assert_eq!(baseline.spec_revision.repository, "google/transit");
        assert_eq!(baseline.spec_revision.commit.len(), 40);
        assert!(baseline
            .spec_revision
            .spec_paths
            .iter()
            .any(|path| path.ends_with("reference.md")));
        assert_eq!(
            baseline.canonical_baseline.repository,
            "MobilityData/gtfs-validator"
        );
        assert!(baseline.canonical_baseline.version.starts_with('v'));
    }

    #[test]
    fn builds_report_identifiers() {
        assert!(spec_revision_id().starts_with("google/transit@"));
        assert!(canonical_baseline_id().starts_with("MobilityData/gtfs-validator@v"));
    }
}
