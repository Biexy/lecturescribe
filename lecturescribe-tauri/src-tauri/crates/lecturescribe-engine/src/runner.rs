use crate::runtime::{
    cancelled_error, EventSink, JobControl, ProgressReporter, TaskContext, TaskExecutionResult,
    TaskExecutor,
};
use crate::store::{Store, TaskTransition};
use chrono::Utc;
use lecturescribe_core::{
    AppError, ArtifactKind, ErrorCategory, ItemState, JobSnapshot, JobState, ProgressMetric,
    ResourceClass, RunPlan, RunSummary, TaskKind, TaskSnapshot, TaskState, TerminalOutcome,
};
use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub metadata: usize,
    pub downloads: usize,
    pub filesystem: usize,
    pub ffmpeg: usize,
    pub gemini: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            metadata: 4,
            downloads: 2,
            filesystem: 2,
            ffmpeg: 1,
            gemini: 1,
        }
    }
}

#[derive(Clone)]
pub struct JobRunner {
    store: Arc<Store>,
    executor: Arc<dyn TaskExecutor>,
    sink: Arc<dyn EventSink>,
    limits: ResourceLimits,
    active_runners: Arc<Mutex<HashMap<String, Arc<JobControl>>>>,
}

impl JobRunner {
    pub fn new(
        store: Arc<Store>,
        executor: Arc<dyn TaskExecutor>,
        sink: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            store,
            executor,
            sink,
            limits: ResourceLimits::default(),
            active_runners: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn start(&self, plan: RunPlan) -> Result<(String, Arc<JobControl>), AppError> {
        let job_id = self.store.create_job(&plan)?;
        let control = self.launch(job_id.clone());
        Ok((job_id, control))
    }

    pub fn resume(&self, job_id: String) -> Result<Arc<JobControl>, AppError> {
        self.store.get_job_snapshot(&job_id)?;
        Ok(self.launch(job_id))
    }

    fn launch(&self, job_id: String) -> Arc<JobControl> {
        let (control, acquired) = {
            let mut active = self
                .active_runners
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            match active.get(&job_id) {
                Some(control) => (control.clone(), false),
                None => {
                    let control = Arc::new(JobControl::default());
                    active.insert(job_id.clone(), control.clone());
                    (control, true)
                }
            }
        };
        if acquired {
            self.spawn(job_id, control.clone());
        }
        control
    }

    fn spawn(&self, job_id: String, control: Arc<JobControl>) {
        let runner = self.clone();
        thread::spawn(move || {
            if let Err(error) = runner.run(&job_id, control) {
                let message = error.user_message.clone();
                if let Ok(event) =
                    runner
                        .store
                        .set_job_state(&job_id, JobState::Failed, &message, None)
                {
                    runner.sink.emit(event);
                }
            }
            runner.release_runner(&job_id);
        });
    }

    fn release_runner(&self, job_id: &str) {
        if let Ok(mut active) = self.active_runners.lock() {
            active.remove(job_id);
        }
    }

    fn run(&self, job_id: &str, control: Arc<JobControl>) -> Result<(), AppError> {
        if control.cancelled() || self.store.get_job_snapshot(job_id)?.state == JobState::Cancelling
        {
            control.cancel();
            let snapshot = self.store.get_job_snapshot(job_id)?;
            self.cancel_remaining(job_id, &snapshot)?;
            return self.finish(job_id, self.store.get_job_snapshot(job_id)?, true);
        }
        self.emit_job_state(job_id, JobState::Running, "Run started", None)?;
        let plan = self
            .store
            .get_plan(&self.store.get_job_snapshot(job_id)?.plan_id)?;
        let (result_tx, result_rx) = mpsc::channel::<TaskFinished>();
        let mut running = HashMap::<String, ResourceClass>::new();
        let mut pool = ResourcePool::new(self.limits.clone());
        let mut retries = HashMap::<String, Instant>::new();

        loop {
            if control.paused() && !control.cancelled() {
                self.emit_job_state(job_id, JobState::Paused, "Run paused", None)?;
                control.checkpoint()?;
                self.emit_job_state(job_id, JobState::Running, "Run resumed", None)?;
            }

            while let Ok(finished) = result_rx.try_recv() {
                if let Some(resource) = running.remove(&finished.context.task.id) {
                    pool.release(resource);
                }
                self.handle_finished(finished, &mut retries)?;
            }

            let snapshot = self.store.get_job_snapshot(job_id)?;
            if snapshot.items.iter().all(|item| item.outcome.is_some()) && running.is_empty() {
                return self.finish(job_id, snapshot, control.cancelled());
            }

            if control.cancelled() {
                if running.is_empty() {
                    self.cancel_remaining(job_id, &snapshot)?;
                }
                thread::sleep(Duration::from_millis(100));
                continue;
            }

            self.release_due_retries(job_id, &snapshot, &mut retries)?;
            let snapshot = self.store.get_job_snapshot(job_id)?;
            let mut launched = false;
            for item_snapshot in &snapshot.items {
                if item_snapshot.outcome.is_some() {
                    continue;
                }
                let states = item_snapshot
                    .tasks
                    .iter()
                    .map(|task| (task.id.as_str(), task.state))
                    .collect::<HashMap<_, _>>();
                for task in &item_snapshot.tasks {
                    if running.contains_key(&task.id)
                        || retries.contains_key(&task.id)
                        || !matches!(
                            task.state,
                            TaskState::Pending | TaskState::Ready | TaskState::Interrupted
                        )
                        || !task.depends_on.iter().all(|dependency| {
                            states
                                .get(dependency.as_str())
                                .is_some_and(|state| state.successful())
                        })
                        || !pool.available(task.resource)
                    {
                        continue;
                    }

                    let attempt = task.attempt + 1;
                    let item_state = item_state_for(task.kind);
                    let Some(event) = self.store.claim_task(TaskTransition {
                        job_id: job_id.to_string(),
                        item_id: item_snapshot.item.item.id.clone(),
                        task_id: task.id.clone(),
                        task_state: TaskState::Running,
                        item_state,
                        progress: Some(ProgressMetric::indeterminate(task_unit(task.kind))),
                        attempt,
                        message: task_start_message(task.kind, &item_snapshot.item.item.title),
                        error: None,
                    })?
                    else {
                        continue;
                    };
                    self.sink.emit(event);
                    pool.acquire(task.resource);
                    running.insert(task.id.clone(), task.resource);
                    self.spawn_task(
                        TaskContext {
                            job_id: job_id.to_string(),
                            plan: plan.clone(),
                            item: item_snapshot.item.clone(),
                            task: TaskSnapshot {
                                attempt,
                                state: TaskState::Running,
                                ..task.clone()
                            },
                        },
                        control.clone(),
                        result_tx.clone(),
                    );
                    launched = true;
                }
            }

            if !launched && running.is_empty() {
                let refreshed = self.store.get_job_snapshot(job_id)?;
                if retries.is_empty() && refreshed.items.iter().any(|item| item.outcome.is_none()) {
                    self.fail_stalled_items(job_id, &refreshed)?;
                }
            }

            match result_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(finished) => {
                    if let Some(resource) = running.remove(&finished.context.task.id) {
                        pool.release(resource);
                    }
                    self.handle_finished(finished, &mut retries)?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(AppError::new(
                        "scheduler_channel_closed",
                        ErrorCategory::Internal,
                        "The task scheduler stopped unexpectedly.",
                        "Worker result channel disconnected.",
                    ));
                }
            }
        }
    }

    fn spawn_task(
        &self,
        context: TaskContext,
        control: Arc<JobControl>,
        result_tx: mpsc::Sender<TaskFinished>,
    ) {
        let executor = self.executor.clone();
        let reporter = StoreProgressReporter {
            store: self.store.clone(),
            sink: self.sink.clone(),
            context: context.clone(),
            last_emit: Mutex::new(None),
        };
        thread::spawn(move || {
            let result = control
                .checkpoint()
                .and_then(|_| executor.execute(&context, &reporter, &control));
            let _ = result_tx.send(TaskFinished { context, result });
        });
    }

    fn handle_finished(
        &self,
        finished: TaskFinished,
        retries: &mut HashMap<String, Instant>,
    ) -> Result<(), AppError> {
        let TaskFinished { context, result } = finished;
        match result {
            Ok(result) => {
                for artifact in &result.artifacts {
                    let event = self.store.record_artifact(artifact)?;
                    self.sink.emit(event);
                }
                let event = self.store.transition_task(TaskTransition {
                    job_id: context.job_id.clone(),
                    item_id: context.item.item.id.clone(),
                    task_id: context.task.id.clone(),
                    task_state: if context.task.kind == TaskKind::Reuse {
                        TaskState::Reused
                    } else {
                        TaskState::Succeeded
                    },
                    item_state: item_state_for(context.task.kind),
                    progress: Some(ProgressMetric::fraction(
                        1.0,
                        1.0,
                        task_unit(context.task.kind),
                    )),
                    attempt: context.task.attempt,
                    message: if result.message.is_empty() {
                        task_done_message(context.task.kind)
                    } else {
                        result.message
                    },
                    error: None,
                })?;
                self.sink.emit(event);
                let snapshot = self.store.get_job_snapshot(&context.job_id)?;
                if let Some(item) = snapshot
                    .items
                    .iter()
                    .find(|item| item.item.item.id == context.item.item.id)
                {
                    if item.tasks.iter().all(|task| task.state.successful()) {
                        let reused = item
                            .tasks
                            .iter()
                            .all(|task| task.state == TaskState::Reused);
                        let outcome = if reused {
                            TerminalOutcome::Reused
                        } else {
                            TerminalOutcome::Complete
                        };
                        let state = if reused {
                            ItemState::Reused
                        } else {
                            ItemState::Complete
                        };
                        let event = self.store.set_item_outcome(
                            &context.job_id,
                            &context.item.item.id,
                            state,
                            outcome,
                            if reused {
                                "Reused verified work"
                            } else {
                                "Complete"
                            },
                            None,
                        )?;
                        self.sink.emit(event);
                    }
                }
            }
            Err(error) if error.category == ErrorCategory::Cancelled => {
                let event = self.store.transition_task(TaskTransition {
                    job_id: context.job_id.clone(),
                    item_id: context.item.item.id.clone(),
                    task_id: context.task.id,
                    task_state: TaskState::Cancelled,
                    item_state: ItemState::Cancelled,
                    progress: None,
                    attempt: context.task.attempt,
                    message: error.user_message.clone(),
                    error: Some(error.clone()),
                })?;
                self.sink.emit(event);
                let event = self.store.set_item_outcome(
                    &context.job_id,
                    &context.item.item.id,
                    ItemState::Cancelled,
                    TerminalOutcome::Cancelled,
                    &error.user_message,
                    Some(&error),
                )?;
                self.sink.emit(event);
            }
            Err(error) if error.retryable && context.task.attempt < context.task.max_attempts => {
                let delay = retry_delay(context.task.attempt, error.category);
                let message = format!(
                    "{} Retrying in {} seconds (attempt {} of {}).",
                    error.user_message,
                    delay.as_secs(),
                    context.task.attempt + 1,
                    context.task.max_attempts
                );
                let event = self.store.transition_task(TaskTransition {
                    job_id: context.job_id,
                    item_id: context.item.item.id,
                    task_id: context.task.id.clone(),
                    task_state: TaskState::Waiting,
                    item_state: ItemState::Waiting,
                    progress: None,
                    attempt: context.task.attempt,
                    message,
                    error: Some(error),
                })?;
                self.sink.emit(event);
                retries.insert(context.task.id, Instant::now() + delay);
            }
            Err(error) => {
                let event = self.store.transition_task(TaskTransition {
                    job_id: context.job_id.clone(),
                    item_id: context.item.item.id.clone(),
                    task_id: context.task.id,
                    task_state: TaskState::Failed,
                    item_state: ItemState::Failed,
                    progress: None,
                    attempt: context.task.attempt,
                    message: error.user_message.clone(),
                    error: Some(error.clone()),
                })?;
                self.sink.emit(event);
                self.skip_remaining_tasks(&context.job_id, &context.item.item.id, &error)?;
                let event = self.store.set_item_outcome(
                    &context.job_id,
                    &context.item.item.id,
                    ItemState::Failed,
                    TerminalOutcome::Failed,
                    &error.user_message,
                    Some(&error),
                )?;
                self.sink.emit(event);
            }
        }
        Ok(())
    }

    fn release_due_retries(
        &self,
        job_id: &str,
        snapshot: &JobSnapshot,
        retries: &mut HashMap<String, Instant>,
    ) -> Result<(), AppError> {
        let due = retries
            .iter()
            .filter(|(_, when)| Instant::now() >= **when)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for task_id in due {
            if let Some((item, task)) = snapshot.items.iter().find_map(|item| {
                item.tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .map(|task| (item, task))
            }) {
                let event = self.store.transition_task(TaskTransition {
                    job_id: job_id.to_string(),
                    item_id: item.item.item.id.clone(),
                    task_id: task.id.clone(),
                    task_state: TaskState::Pending,
                    item_state: ItemState::Queued,
                    progress: None,
                    attempt: task.attempt,
                    message: "Retry ready".to_string(),
                    error: None,
                })?;
                self.sink.emit(event);
            }
            retries.remove(&task_id);
        }
        Ok(())
    }

    fn skip_remaining_tasks(
        &self,
        job_id: &str,
        item_id: &str,
        cause: &AppError,
    ) -> Result<(), AppError> {
        let snapshot = self.store.get_job_snapshot(job_id)?;
        if let Some(item) = snapshot
            .items
            .iter()
            .find(|item| item.item.item.id == item_id)
        {
            for task in &item.tasks {
                if matches!(
                    task.state,
                    TaskState::Pending | TaskState::Ready | TaskState::Interrupted
                ) {
                    let event = self.store.transition_task(TaskTransition {
                        job_id: job_id.to_string(),
                        item_id: item_id.to_string(),
                        task_id: task.id.clone(),
                        task_state: TaskState::Skipped,
                        item_state: ItemState::Failed,
                        progress: None,
                        attempt: task.attempt,
                        message: "Skipped because an earlier step failed".to_string(),
                        error: Some(cause.clone()),
                    })?;
                    self.sink.emit(event);
                }
            }
        }
        Ok(())
    }

    fn cancel_remaining(&self, job_id: &str, snapshot: &JobSnapshot) -> Result<(), AppError> {
        let error = cancelled_error();
        for item in &snapshot.items {
            if item.outcome.is_some() {
                continue;
            }
            for task in &item.tasks {
                if !task.state.terminal() {
                    let event = self.store.transition_task(TaskTransition {
                        job_id: job_id.to_string(),
                        item_id: item.item.item.id.clone(),
                        task_id: task.id.clone(),
                        task_state: TaskState::Cancelled,
                        item_state: ItemState::Cancelled,
                        progress: None,
                        attempt: task.attempt,
                        message: error.user_message.clone(),
                        error: Some(error.clone()),
                    })?;
                    self.sink.emit(event);
                }
            }
            let event = self.store.set_item_outcome(
                job_id,
                &item.item.item.id,
                ItemState::Cancelled,
                TerminalOutcome::Cancelled,
                &error.user_message,
                Some(&error),
            )?;
            self.sink.emit(event);
        }
        Ok(())
    }

    fn fail_stalled_items(&self, job_id: &str, snapshot: &JobSnapshot) -> Result<(), AppError> {
        let error = AppError::new(
            "task_graph_stalled",
            ErrorCategory::Internal,
            "This item could not continue because its task dependencies were incomplete.",
            "No runnable task or active worker remained in the task graph.",
        );
        for item in &snapshot.items {
            if item.outcome.is_none() {
                self.skip_remaining_tasks(job_id, &item.item.item.id, &error)?;
                let event = self.store.set_item_outcome(
                    job_id,
                    &item.item.item.id,
                    ItemState::Failed,
                    TerminalOutcome::Failed,
                    &error.user_message,
                    Some(&error),
                )?;
                self.sink.emit(event);
            }
        }
        Ok(())
    }

    fn finish(&self, job_id: &str, snapshot: JobSnapshot, cancelled: bool) -> Result<(), AppError> {
        let state = if cancelled || snapshot.counts.cancelled > 0 {
            JobState::Cancelled
        } else if snapshot.counts.failed == snapshot.counts.planned {
            JobState::Failed
        } else {
            JobState::Complete
        };
        let downloaded_media = snapshot
            .items
            .iter()
            .filter(|item| {
                item.artifacts
                    .iter()
                    .any(|artifact| artifact.kind == ArtifactKind::DownloadedMedia)
            })
            .count();
        let saved_transcripts = snapshot
            .items
            .iter()
            .filter(|item| {
                item.artifacts.iter().any(|artifact| {
                    matches!(
                        artifact.kind,
                        ArtifactKind::TextTranscript
                            | ArtifactKind::MarkdownTranscript
                            | ArtifactKind::SrtTranscript
                            | ArtifactKind::VttTranscript
                    )
                })
            })
            .count();
        let gemini_requests = snapshot
            .items
            .iter()
            .flat_map(|item| &item.artifacts)
            .filter(|artifact| artifact.kind == ArtifactKind::SegmentTranscript)
            .count();
        let processed_seconds = snapshot
            .items
            .iter()
            .filter(|item| {
                matches!(
                    item.outcome,
                    Some(TerminalOutcome::Complete | TerminalOutcome::Reused)
                )
            })
            .filter_map(|item| item.item.item.duration_seconds)
            .sum();
        let plan = self.store.get_plan(&snapshot.plan_id)?;
        let output_dir = if plan.batch_output_dir.trim().is_empty() {
            plan.settings.output_dir
        } else {
            plan.batch_output_dir
        };
        let summary = RunSummary {
            job_id: job_id.to_string(),
            outcome: state,
            counts: snapshot.counts,
            output_dir,
            downloaded_media,
            saved_transcripts,
            gemini_requests,
            processed_seconds,
            elapsed_seconds: (Utc::now() - snapshot.started_at).num_seconds().max(0) as u64,
            completed_at: Utc::now(),
        };
        let message = match state {
            JobState::Complete if summary.counts.failed > 0 => {
                "Run complete with items needing attention"
            }
            JobState::Complete => "Run complete",
            JobState::Cancelled => "Run cancelled; verified work was preserved",
            _ => "Run failed",
        };
        self.emit_job_state(job_id, state, message, Some(&summary))?;
        Ok(())
    }

    fn emit_job_state(
        &self,
        job_id: &str,
        state: JobState,
        message: &str,
        summary: Option<&RunSummary>,
    ) -> Result<(), AppError> {
        let event = self.store.set_job_state(job_id, state, message, summary)?;
        self.sink.emit(event);
        Ok(())
    }
}

struct StoreProgressReporter {
    store: Arc<Store>,
    sink: Arc<dyn EventSink>,
    context: TaskContext,
    last_emit: Mutex<Option<Instant>>,
}

impl ProgressReporter for StoreProgressReporter {
    fn report(&self, progress: ProgressMetric, message: &str) {
        let final_progress = progress.percent().is_some_and(|value| value >= 100.0);
        if let Ok(mut last) = self.last_emit.lock() {
            if !final_progress
                && last
                    .as_ref()
                    .is_some_and(|when| when.elapsed() < Duration::from_millis(100))
            {
                return;
            }
            *last = Some(Instant::now());
        }
        if let Ok(event) = self.store.transition_task(TaskTransition {
            job_id: self.context.job_id.clone(),
            item_id: self.context.item.item.id.clone(),
            task_id: self.context.task.id.clone(),
            task_state: TaskState::Running,
            item_state: item_state_for(self.context.task.kind),
            progress: Some(progress),
            attempt: self.context.task.attempt,
            message: message.to_string(),
            error: None,
        }) {
            self.sink.emit(event);
        }
    }
}

struct TaskFinished {
    context: TaskContext,
    result: Result<TaskExecutionResult, AppError>,
}

struct ResourcePool {
    limits: ResourceLimits,
    active: HashMap<ResourceClass, usize>,
}

impl ResourcePool {
    fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            active: HashMap::new(),
        }
    }

    fn available(&self, resource: ResourceClass) -> bool {
        self.active.get(&resource).copied().unwrap_or_default() < self.limit(resource)
    }

    fn acquire(&mut self, resource: ResourceClass) {
        *self.active.entry(resource).or_default() += 1;
    }

    fn release(&mut self, resource: ResourceClass) {
        let value = self.active.entry(resource).or_default();
        *value = value.saturating_sub(1);
    }

    fn limit(&self, resource: ResourceClass) -> usize {
        match resource {
            ResourceClass::Metadata => self.limits.metadata,
            ResourceClass::Download => self.limits.downloads,
            ResourceClass::Filesystem => self.limits.filesystem,
            ResourceClass::Ffmpeg => self.limits.ffmpeg,
            ResourceClass::Gemini => self.limits.gemini,
        }
        .max(1)
    }
}

fn item_state_for(kind: TaskKind) -> ItemState {
    match kind {
        TaskKind::Inspect => ItemState::Inspecting,
        TaskKind::Download => ItemState::Downloading,
        TaskKind::Verify => ItemState::Verifying,
        TaskKind::Prepare => ItemState::Preparing,
        TaskKind::Segment => ItemState::Segmenting,
        TaskKind::Transcribe => ItemState::Transcribing,
        TaskKind::Validate => ItemState::Validating,
        TaskKind::Merge => ItemState::Merging,
        TaskKind::Save => ItemState::Saving,
        TaskKind::Reuse => ItemState::Reused,
    }
}

fn task_unit(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Download => "bytes",
        TaskKind::Prepare | TaskKind::Segment => "media",
        TaskKind::Transcribe => "segments",
        _ => "step",
    }
}

fn task_start_message(kind: TaskKind, title: &str) -> String {
    let action = match kind {
        TaskKind::Inspect => "Inspecting",
        TaskKind::Download => "Downloading",
        TaskKind::Verify => "Verifying",
        TaskKind::Prepare => "Preparing audio",
        TaskKind::Segment => "Creating segments",
        TaskKind::Transcribe => "Transcribing",
        TaskKind::Validate => "Validating transcript",
        TaskKind::Merge => "Merging transcript",
        TaskKind::Save => "Saving outputs",
        TaskKind::Reuse => "Reusing verified output",
    };
    format!("{action}: {title}")
}

fn task_done_message(kind: TaskKind) -> String {
    match kind {
        TaskKind::Inspect => "Source inspected",
        TaskKind::Download => "Download complete",
        TaskKind::Verify => "Media verified",
        TaskKind::Prepare => "Audio prepared",
        TaskKind::Segment => "Segments ready",
        TaskKind::Transcribe => "Segments transcribed",
        TaskKind::Validate => "Transcript validated",
        TaskKind::Merge => "Transcript merged",
        TaskKind::Save => "Outputs saved",
        TaskKind::Reuse => "Verified output reused",
    }
    .to_string()
}

fn retry_delay(attempt: u32, category: ErrorCategory) -> Duration {
    let base: u64 = match category {
        ErrorCategory::Quota => 30,
        ErrorCategory::Network | ErrorCategory::Download => 3,
        _ => 2,
    };
    Duration::from_secs((base * 2u64.saturating_pow(attempt.saturating_sub(1))).min(180))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::NoopEventSink;
    use lecturescribe_core::{build_plan, PlanCapabilities};
    use lecturescribe_core::{
        AppSettings, ItemState, PlanRequest, PreviewItem, PreviewSnapshot, ProviderKind, RunMode,
        SourceKind,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Barrier, Condvar};

    struct BlockingExecutor {
        calls: AtomicUsize,
        state: (Mutex<(bool, bool)>, Condvar),
    }

    impl BlockingExecutor {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                state: (Mutex::new((false, false)), Condvar::new()),
            }
        }

        fn wait_until_started(&self) {
            let (lock, changed) = &self.state;
            let started = lock.lock().unwrap();
            let (started, _) = changed
                .wait_timeout_while(started, Duration::from_secs(2), |state| !state.0)
                .unwrap();
            assert!(started.0, "runner did not start the task");
        }

        fn release(&self) {
            let (lock, changed) = &self.state;
            let mut state = lock.lock().unwrap();
            state.1 = true;
            changed.notify_all();
        }
    }

    impl TaskExecutor for BlockingExecutor {
        fn execute(
            &self,
            _context: &TaskContext,
            _reporter: &dyn ProgressReporter,
            _control: &JobControl,
        ) -> Result<TaskExecutionResult, AppError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let (lock, changed) = &self.state;
            let mut state = lock.lock().unwrap();
            state.0 = true;
            changed.notify_all();
            let _ = changed
                .wait_timeout_while(state, Duration::from_secs(2), |state| !state.1)
                .unwrap();
            Ok(TaskExecutionResult::default())
        }
    }

    fn test_plan(store: &Store) -> RunPlan {
        let preview = PreviewSnapshot {
            id: "runner-preview".to_string(),
            created_at: Utc::now(),
            items: vec![PreviewItem {
                id: "runner-item".to_string(),
                source_id: "runner-source".to_string(),
                source_kind: SourceKind::LocalMedia,
                provider: ProviderKind::Local,
                source_group: "test".to_string(),
                title: "Runner test".to_string(),
                source: "C:/test/input.wav".to_string(),
                canonical_source: "C:/test/input.wav".to_string(),
                url: None,
                media_path: Some("C:/test/input.wav".to_string()),
                existing_media_path: None,
                existing_transcript_path: Some("C:/test/output.txt".to_string()),
                thumbnail_url: None,
                duration_seconds: None,
                expected_media_name: None,
                selected: true,
                status: ItemState::Queued,
                duplicate_of: None,
                error: None,
            }],
            duplicate_count: 0,
            source_count: 1,
            warnings: Vec::new(),
        };
        store.save_preview(&preview).unwrap();
        build_plan(
            &preview,
            PlanRequest {
                preview_id: preview.id.clone(),
                selected_item_ids: vec!["runner-item".to_string()],
                mode: RunMode::Transcribe,
                settings: AppSettings::default(),
                overrides: Default::default(),
            },
            PlanCapabilities {
                output_ready: true,
                ..PlanCapabilities::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn concurrent_resumes_acquire_one_runner_and_execute_once() {
        let store = Arc::new(
            Store::open(std::env::temp_dir().join(format!(
                "lecturescribe-runner-{}.sqlite3",
                uuid::Uuid::new_v4()
            )))
            .unwrap(),
        );
        let executor = Arc::new(BlockingExecutor::new());
        let runner = JobRunner::new(store.clone(), executor.clone(), Arc::new(NoopEventSink));
        let job_id = store.create_job(&test_plan(&store)).unwrap();
        let barrier = Arc::new(Barrier::new(6));
        let resumes = (0..6)
            .map(|_| {
                let runner = runner.clone();
                let job_id = job_id.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    runner.resume(job_id).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let controls = resumes
            .into_iter()
            .map(|resume| resume.join().unwrap())
            .collect::<Vec<_>>();

        executor.wait_until_started();
        assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
        assert!(controls
            .iter()
            .all(|control| Arc::ptr_eq(control, &controls[0])));

        executor.release();
    }
}
