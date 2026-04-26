//! Pipeline state tracking for resumable ETL runs.
//!
//! Ported from `discogs-cache/lib/pipeline_state.py` and `db_introspect.py`.
//! Tracks step completion in a JSON state file so that a failed pipeline can
//! be resumed from where it left off.

pub mod introspect;
mod state;

pub use state::{PipelineState, StepStatus, STATE_VERSION, STEP_NAMES};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_state_creation() {
        let steps = &["create_schema", "import_csv", "create_indexes"];
        let state = PipelineState::new("postgresql:///discogs", "/tmp/csv", steps);

        assert_eq!(state.step_status("create_schema"), "pending");
        assert!(!state.is_completed("create_schema"));
        assert!(!state.is_completed("import_csv"));
    }

    #[test]
    fn mark_completed() {
        let steps = &["step1", "step2"];
        let mut state = PipelineState::new("db_url", "csv_dir", steps);
        state.mark_completed("step1");
        assert!(state.is_completed("step1"));
        assert!(!state.is_completed("step2"));
        assert_eq!(state.step_status("step1"), "completed");
    }

    #[test]
    fn mark_failed() {
        let steps = &["step1"];
        let mut state = PipelineState::new("db_url", "csv_dir", steps);
        state.mark_failed("step1", "connection refused");
        assert_eq!(state.step_status("step1"), "failed");
        assert_eq!(state.step_error("step1"), Some("connection refused"));
    }

    #[test]
    fn step_error_returns_none_for_non_failed() {
        let steps = &["step1"];
        let state = PipelineState::new("db_url", "csv_dir", steps);
        assert_eq!(state.step_error("step1"), None);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let steps = &["step1", "step2"];
        let mut state = PipelineState::new("postgresql:///discogs", "/tmp/csv", steps);
        state.mark_completed("step1");
        state.save(&path).unwrap();

        let loaded = PipelineState::load(&path).unwrap();
        assert!(loaded.is_completed("step1"));
        assert!(!loaded.is_completed("step2"));
    }

    #[test]
    fn save_load_with_failed_step() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut state = PipelineState::new("db", "csv", STEP_NAMES);
        state.mark_completed("create_schema");
        state.mark_failed("import_csv", "disk full");
        state.save(&path).unwrap();

        let loaded = PipelineState::load(&path).unwrap();
        assert!(loaded.is_completed("create_schema"));
        assert_eq!(loaded.step_status("import_csv"), "failed");
        assert_eq!(loaded.step_error("import_csv"), Some("disk full"));
    }

    #[test]
    fn migrate_v1() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let v1_json = r#"{
            "version": 1,
            "database_url": "postgresql:///discogs",
            "csv_dir": "/tmp/csv",
            "steps": {
                "create_schema": {"status": "completed"},
                "import_csv": {"status": "completed"},
                "create_indexes": {"status": "pending"},
                "dedup": {"status": "pending"},
                "prune": {"status": "pending"},
                "vacuum": {"status": "pending"}
            }
        }"#;
        std::fs::write(&path, v1_json).unwrap();

        let state = PipelineState::load(&path).unwrap();
        // V1→V3: import_csv completed implies import_tracks completed
        assert!(state.is_completed("import_tracks"));
        assert!(state.is_completed("create_schema"));
        assert!(state.is_completed("import_csv"));
        // create_indexes not completed, dedup not completed → create_track_indexes stays pending
        assert!(!state.is_completed("create_track_indexes"));
        // vacuum not completed → set_logged stays pending
        assert!(!state.is_completed("set_logged"));
    }

    #[test]
    fn migrate_v1_dedup_completed_infers_track_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let v1_json = r#"{
            "version": 1,
            "database_url": "db",
            "csv_dir": "csv",
            "steps": {
                "create_schema": {"status": "completed"},
                "import_csv": {"status": "completed"},
                "create_indexes": {"status": "completed"},
                "dedup": {"status": "completed"},
                "prune": {"status": "completed"},
                "vacuum": {"status": "completed"}
            }
        }"#;
        std::fs::write(&path, v1_json).unwrap();

        let state = PipelineState::load(&path).unwrap();
        assert!(state.is_completed("create_track_indexes"));
        assert!(state.is_completed("import_tracks"));
        assert!(state.is_completed("set_logged"));
    }

    #[test]
    fn migrate_v2() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let v2_json = r#"{
            "version": 2,
            "database_url": "db",
            "csv_dir": "csv",
            "steps": {
                "create_schema": {"status": "completed"},
                "import_csv": {"status": "completed"},
                "create_indexes": {"status": "completed"},
                "dedup": {"status": "completed"},
                "import_tracks": {"status": "completed"},
                "create_track_indexes": {"status": "completed"},
                "prune": {"status": "completed"},
                "vacuum": {"status": "completed"}
            }
        }"#;
        std::fs::write(&path, v2_json).unwrap();

        let state = PipelineState::load(&path).unwrap();
        // vacuum completed → set_logged completed
        assert!(state.is_completed("set_logged"));
    }

    #[test]
    fn migrate_v2_vacuum_pending() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let v2_json = r#"{
            "version": 2,
            "database_url": "db",
            "csv_dir": "csv",
            "steps": {
                "create_schema": {"status": "completed"},
                "import_csv": {"status": "pending"},
                "create_indexes": {"status": "pending"},
                "dedup": {"status": "pending"},
                "import_tracks": {"status": "pending"},
                "create_track_indexes": {"status": "pending"},
                "prune": {"status": "pending"},
                "vacuum": {"status": "pending"}
            }
        }"#;
        std::fs::write(&path, v2_json).unwrap();

        let state = PipelineState::load(&path).unwrap();
        assert!(!state.is_completed("set_logged"));
    }

    #[test]
    fn unsupported_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        std::fs::write(&path, r#"{"version": 99}"#).unwrap();
        assert!(PipelineState::load(&path).is_err());
    }

    #[test]
    fn validate_resume_ok() {
        let steps = &["step1"];
        let state = PipelineState::new("postgresql:///discogs", "/tmp/csv", steps);
        assert!(state
            .validate_resume("postgresql:///discogs", "/tmp/csv")
            .is_ok());
    }

    #[test]
    fn validate_resume_db_mismatch() {
        let steps = &["step1"];
        let state = PipelineState::new("postgresql:///discogs", "/tmp/csv", steps);
        assert!(state
            .validate_resume("postgresql:///other", "/tmp/csv")
            .is_err());
    }

    #[test]
    fn validate_resume_csv_dir_mismatch() {
        let steps = &["step1"];
        let state = PipelineState::new("postgresql:///discogs", "/tmp/csv", steps);
        assert!(state
            .validate_resume("postgresql:///discogs", "/other/csv")
            .is_err());
    }

    #[test]
    fn atomic_save_no_leftover_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let state = PipelineState::new("db", "csv", &["step1"]);
        state.save(&path).unwrap();

        assert!(path.exists());
        assert!(!path.with_extension("tmp").exists());
    }
}
