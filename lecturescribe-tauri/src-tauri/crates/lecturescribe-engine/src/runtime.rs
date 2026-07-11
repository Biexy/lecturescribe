use lecturescribe_core::{
    AppError, ArtifactRecord, ErrorCategory, PlannedItem, ProgressMetric, RunPlan, TaskSnapshot,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: lecturescribe_core::AppEvent);
}

#[derive(Debug, Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn emit(&self, _event: lecturescribe_core::AppEvent) {}
}

pub trait TaskExecutor: Send + Sync + 'static {
    fn execute(
        &self,
        context: &TaskContext,
        reporter: &dyn ProgressReporter,
        control: &JobControl,
    ) -> Result<TaskExecutionResult, AppError>;
}

pub trait ProgressReporter: Send + Sync {
    fn report(&self, progress: ProgressMetric, message: &str);
}

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub job_id: String,
    pub plan: RunPlan,
    pub item: PlannedItem,
    pub task: TaskSnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct TaskExecutionResult {
    pub message: String,
    pub artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Default)]
pub struct JobControl {
    cancelled: AtomicBool,
    pause_state: Mutex<bool>,
    pause_changed: Condvar,
}

impl JobControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.pause_changed.notify_all();
    }

    pub fn pause(&self) {
        if let Ok(mut paused) = self.pause_state.lock() {
            *paused = true;
        }
    }

    pub fn resume(&self) {
        if let Ok(mut paused) = self.pause_state.lock() {
            *paused = false;
            self.pause_changed.notify_all();
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn paused(&self) -> bool {
        self.pause_state.lock().map(|value| *value).unwrap_or(false)
    }

    pub fn checkpoint(&self) -> Result<(), AppError> {
        if self.cancelled() {
            return Err(cancelled_error());
        }
        let mut paused = self.pause_state.lock().map_err(|error| {
            AppError::new(
                "job_control_poisoned",
                ErrorCategory::Internal,
                "LectureScribe could not read the run control state.",
                error.to_string(),
            )
        })?;
        while *paused && !self.cancelled() {
            let result = self
                .pause_changed
                .wait_timeout(paused, Duration::from_millis(250))
                .map_err(|error| {
                    AppError::new(
                        "job_control_wait_failed",
                        ErrorCategory::Internal,
                        "LectureScribe could not pause the run safely.",
                        error.to_string(),
                    )
                })?;
            paused = result.0;
        }
        if self.cancelled() {
            Err(cancelled_error())
        } else {
            Ok(())
        }
    }
}

pub fn cancelled_error() -> AppError {
    AppError::new(
        "job_cancelled",
        ErrorCategory::Cancelled,
        "The run was cancelled.",
        "Cancellation was requested by the user.",
    )
    .retryable("Completed downloads, segments, and transcripts remain available.")
}
