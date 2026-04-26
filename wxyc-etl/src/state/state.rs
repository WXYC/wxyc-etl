use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

/// Current state file version.
pub const STATE_VERSION: u32 = 3;

/// Step names for the v3 pipeline.
pub const STEP_NAMES: &[&str] = &[
    "create_schema",
    "import_csv",
    "create_indexes",
    "dedup",
    "import_tracks",
    "create_track_indexes",
    "prune",
    "vacuum",
    "set_logged",
];

const V1_STEP_NAMES: &[&str] = &[
    "create_schema",
    "import_csv",
    "create_indexes",
    "dedup",
    "prune",
    "vacuum",
];

const V2_STEP_NAMES: &[&str] = &[
    "create_schema",
    "import_csv",
    "create_indexes",
    "dedup",
    "import_tracks",
    "create_track_indexes",
    "prune",
    "vacuum",
];

/// Status of a pipeline step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum StepStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed {
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

/// Tracks step completion status for resumable ETL runs.
///
/// Ported from `discogs-cache/lib/pipeline_state.py`. Supports save/load with
/// atomic writes and version migration from v1 and v2 formats.
#[derive(Debug, Serialize, Deserialize)]
pub struct PipelineState {
    version: u32,
    database_url: String,
    csv_dir: String,
    steps: HashMap<String, StepStatus>,
}

impl PipelineState {
    /// Create a new state with all steps pending.
    pub fn new(db_url: &str, csv_dir: &str, steps: &[&str]) -> Self {
        let mut step_map = HashMap::new();
        for &step in steps {
            step_map.insert(step.to_string(), StepStatus::Pending);
        }
        Self {
            version: STATE_VERSION,
            database_url: db_url.to_string(),
            csv_dir: csv_dir.to_string(),
            steps: step_map,
        }
    }

    /// Return true if the step has been completed.
    pub fn is_completed(&self, step: &str) -> bool {
        matches!(self.steps.get(step), Some(StepStatus::Completed))
    }

    /// Mark a step as completed.
    pub fn mark_completed(&mut self, step: &str) {
        self.steps.insert(step.to_string(), StepStatus::Completed);
    }

    /// Mark a step as failed with an error message.
    pub fn mark_failed(&mut self, step: &str, error: &str) {
        self.steps.insert(
            step.to_string(),
            StepStatus::Failed {
                error: Some(error.to_string()),
            },
        );
    }

    /// Return the status string of a step.
    pub fn step_status(&self, step: &str) -> &str {
        match self.steps.get(step) {
            Some(StepStatus::Pending) => "pending",
            Some(StepStatus::Completed) => "completed",
            Some(StepStatus::Failed { .. }) => "failed",
            None => "unknown",
        }
    }

    /// Return the error message for a failed step, or None.
    pub fn step_error(&self, step: &str) -> Option<&str> {
        match self.steps.get(step) {
            Some(StepStatus::Failed { error }) => error.as_deref(),
            _ => None,
        }
    }

    /// Raise an error if db_url or csv_dir don't match this state.
    pub fn validate_resume(&self, db_url: &str, csv_dir: &str) -> Result<()> {
        if self.database_url != db_url {
            bail!(
                "database_url mismatch: state has {:?}, got {:?}",
                self.database_url,
                db_url
            );
        }
        if self.csv_dir != csv_dir {
            bail!(
                "csv_dir mismatch: state has {:?}, got {:?}",
                self.csv_dir,
                csv_dir
            );
        }
        Ok(())
    }

    /// Write state to a JSON file atomically (write .tmp, then rename).
    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self).context("serializing pipeline state")?;
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, format!("{}\n", json))
            .with_context(|| format!("writing temp state file {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("renaming {} to {}", tmp_path.display(), path.display()))?;
        Ok(())
    }

    /// Load state from a JSON file with version migration support.
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let raw: serde_json::Value = serde_json::from_str(&text).context("parsing state JSON")?;

        let version = raw
            .get("version")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32);

        match version {
            Some(1) => Self::migrate_v1(&raw),
            Some(2) => Self::migrate_v2(&raw),
            Some(STATE_VERSION) => Self::load_v3(&raw),
            Some(v) => bail!(
                "Unsupported state file version {} (expected {})",
                v,
                STATE_VERSION
            ),
            None => bail!("Missing version field in state file"),
        }
    }

    fn load_v3(raw: &serde_json::Value) -> Result<Self> {
        let db_url = raw["database_url"]
            .as_str()
            .context("missing database_url")?;
        let csv_dir = raw["csv_dir"].as_str().context("missing csv_dir")?;
        let steps_val = raw.get("steps").context("missing steps")?;
        let steps_map = steps_val.as_object().context("steps must be an object")?;

        let step_names: Vec<&str> = steps_map.keys().map(|k| k.as_str()).collect();
        let mut state = Self::new(db_url, csv_dir, &step_names);
        Self::copy_steps(&mut state, steps_val, &step_names);
        Ok(state)
    }

    /// Migrate a v1 state file to v3 format.
    ///
    /// V2 adds import_tracks and create_track_indexes between dedup and prune.
    /// V3 adds set_logged after vacuum.
    ///
    /// Migration rules:
    /// - All v1 steps map directly to their v3 equivalents
    /// - import_csv completed → import_tracks completed (v1 imported tracks as part of import_csv)
    /// - dedup or create_indexes completed → create_track_indexes completed
    /// - vacuum completed → set_logged completed (v1 used LOGGED tables throughout)
    fn migrate_v1(raw: &serde_json::Value) -> Result<Self> {
        let db_url = raw["database_url"]
            .as_str()
            .context("missing database_url")?;
        let csv_dir = raw["csv_dir"].as_str().context("missing csv_dir")?;
        let steps_val = raw.get("steps").context("missing steps")?;

        let mut state = Self::new(db_url, csv_dir, STEP_NAMES);
        Self::copy_steps(&mut state, steps_val, V1_STEP_NAMES);

        // Infer import_tracks from import_csv
        if Self::step_is_completed(steps_val, "import_csv") {
            state.mark_completed("import_tracks");
        }

        // Infer create_track_indexes from dedup or create_indexes
        if Self::step_is_completed(steps_val, "dedup")
            || Self::step_is_completed(steps_val, "create_indexes")
        {
            state.mark_completed("create_track_indexes");
        }

        // Infer set_logged from vacuum
        if Self::step_is_completed(steps_val, "vacuum") {
            state.mark_completed("set_logged");
        }

        Ok(state)
    }

    /// Migrate a v2 state file to v3 format.
    ///
    /// V3 adds set_logged after vacuum.
    ///
    /// Migration rules:
    /// - All v2 steps map directly to their v3 equivalents
    /// - vacuum completed → set_logged completed (v2 used LOGGED tables throughout)
    fn migrate_v2(raw: &serde_json::Value) -> Result<Self> {
        let db_url = raw["database_url"]
            .as_str()
            .context("missing database_url")?;
        let csv_dir = raw["csv_dir"].as_str().context("missing csv_dir")?;
        let steps_val = raw.get("steps").context("missing steps")?;

        let mut state = Self::new(db_url, csv_dir, STEP_NAMES);
        Self::copy_steps(&mut state, steps_val, V2_STEP_NAMES);

        // Infer set_logged from vacuum
        if Self::step_is_completed(steps_val, "vacuum") {
            state.mark_completed("set_logged");
        }

        Ok(state)
    }

    fn copy_steps(state: &mut Self, steps_val: &serde_json::Value, step_names: &[&str]) {
        for &name in step_names {
            if let Some(step_obj) = steps_val.get(name) {
                if let Some(status) = step_obj.get("status").and_then(|s| s.as_str()) {
                    match status {
                        "completed" => {
                            state.steps.insert(name.to_string(), StepStatus::Completed);
                        }
                        "failed" => {
                            let error = step_obj
                                .get("error")
                                .and_then(|e| e.as_str())
                                .map(|s| s.to_string());
                            state
                                .steps
                                .insert(name.to_string(), StepStatus::Failed { error });
                        }
                        _ => {} // keep as pending
                    }
                }
            }
        }
    }

    fn step_is_completed(steps_val: &serde_json::Value, name: &str) -> bool {
        steps_val
            .get(name)
            .and_then(|s| s.get("status"))
            .and_then(|s| s.as_str())
            == Some("completed")
    }
}
