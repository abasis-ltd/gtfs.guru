/// Trait for handling progress events during GTFS validation
pub trait ProgressHandler: Send + Sync {
    /// Called when starting to load a file
    fn on_start_file_load(&self, file: &str);

    /// Called when finished loading a file
    fn on_finish_file_load(&self, file: &str);

    /// Called when starting a validator
    fn on_start_validation(&self, validator_name: &str);

    /// Called when finished a validator
    fn on_finish_validation(&self, validator_name: &str);

    /// Set total number of files to load (optional usage)
    fn set_total_files(&self, count: usize) {
        let _ = count;
    }

    /// Set total number of validators to run
    fn set_total_validators(&self, count: usize);

    /// Increment validation progress
    fn increment_validator_progress(&self);
}

/// A no-op progress handler
pub struct NoOpProgressHandler;

impl ProgressHandler for NoOpProgressHandler {
    fn on_start_file_load(&self, _file: &str) {}
    fn on_finish_file_load(&self, _file: &str) {}
    fn on_start_validation(&self, _validator_name: &str) {}
    fn on_finish_validation(&self, _validator_name: &str) {}
    fn set_total_validators(&self, _count: usize) {}
    fn increment_validator_progress(&self) {}
}
