use crate::db::SqlValue;
use crate::store::{from_json, parse_enum, parse_time, Store};
use lecturescribe_core::{
    AppError, HistoryEntry, ItemSnapshot, ItemState, JobCounts, JobSnapshot, JobState,
    ProgressMetric, RunSummary, TaskSnapshot, TaskState, TerminalOutcome,
};

impl Store {
    pub fn get_job_snapshot(&self, job_id: &str) -> Result<JobSnapshot, AppError> {
        let connection = self.connect()?;
        let rows = connection.query(
            "SELECT plan_id, state, sequence, started_at, finished_at, message, summary_json FROM jobs WHERE id = ?",
            &[job_id.into()],
        )?;
        let row = rows.first().ok_or_else(|| {
            AppError::new(
                "job_not_found",
                lecturescribe_core::ErrorCategory::Database,
                "That run is no longer available.",
                "No job row matched the requested ID.",
            )
        })?;
        let plan_id = text(row, 0).to_string();
        let state: JobState = parse_enum(text(row, 1))?;
        let sequence = integer(row, 2);
        let started_at = parse_time(text(row, 3))?;
        let finished_at = optional_text(row, 4).map(parse_time).transpose()?;
        let message = text(row, 5).to_string();
        let summary = optional_text(row, 6)
            .map(from_json::<RunSummary>)
            .transpose()?;
        let plan = self.get_plan(&plan_id)?;

        let item_rows = connection.query(
            "SELECT item_id, state, outcome, message, error_json, item_json FROM job_items WHERE job_id = ? ORDER BY ordinal",
            &[job_id.into()],
        )?;
        let mut items = Vec::new();
        for item_row in item_rows {
            let item_id = text(&item_row, 0);
            let item_state: ItemState = parse_enum(text(&item_row, 1))?;
            let outcome = optional_text(&item_row, 2)
                .map(parse_enum::<TerminalOutcome>)
                .transpose()?;
            let item_message = text(&item_row, 3).to_string();
            let item_error = optional_text(&item_row, 4).map(from_json).transpose()?;
            let planned = from_json(text(&item_row, 5))?;
            let tasks = self.tasks_for_item(job_id, item_id)?;
            let artifacts = self.artifacts_for_item(job_id, item_id)?;
            let progress = item_progress(&tasks, outcome);
            items.push(ItemSnapshot {
                item: planned,
                state: item_state,
                outcome,
                tasks,
                progress,
                message: item_message,
                error: item_error,
                artifacts,
            });
        }

        let counts = job_counts(&items, plan.selected_count);
        let overall_progress = overall_progress(&items);
        let current = items
            .iter()
            .flat_map(|item| {
                item.tasks
                    .iter()
                    .map(move |task| (&item.item.item.id, task))
            })
            .find(|(_, task)| matches!(task.state, TaskState::Running | TaskState::Waiting))
            .map(|(item_id, task)| (item_id.clone(), task.id.clone()));

        Ok(JobSnapshot {
            id: job_id.to_string(),
            plan_id,
            state,
            sequence,
            started_at,
            finished_at,
            items,
            counts,
            overall_progress,
            current_item_id: current.as_ref().map(|(item_id, _)| item_id.clone()),
            current_task_id: current.map(|(_, task_id)| task_id),
            message,
            summary,
        })
    }

    pub fn list_history(&self, limit: usize) -> Result<Vec<HistoryEntry>, AppError> {
        let rows = self.connect()?.query(
            "SELECT jobs.id, jobs.started_at, jobs.finished_at, jobs.state, jobs.summary_json, plans.plan_json FROM jobs JOIN plans ON plans.id = jobs.plan_id ORDER BY jobs.started_at DESC LIMIT ?",
            &[(limit.clamp(1, 100) as i64).into()],
        )?;
        rows.into_iter()
            .map(|row| {
                let job_id = text(&row, 0).to_string();
                let started_at = parse_time(text(&row, 1))?;
                let completed_at = optional_text(&row, 2).map(parse_time).transpose()?;
                let state = parse_enum(text(&row, 3))?;
                let summary = optional_text(&row, 4)
                    .map(from_json::<RunSummary>)
                    .transpose()?;
                let plan: lecturescribe_core::RunPlan = from_json(text(&row, 5))?;
                let counts = summary
                    .as_ref()
                    .map(|summary| summary.counts.clone())
                    .unwrap_or(JobCounts {
                        planned: plan.selected_count,
                        ..JobCounts::default()
                    });
                let title = plan
                    .items
                    .first()
                    .map(|item| {
                        if plan.selected_count > 1 {
                            format!("{} and {} more", item.item.title, plan.selected_count - 1)
                        } else {
                            item.item.title.clone()
                        }
                    })
                    .unwrap_or_else(|| "Empty run".to_string());
                Ok(HistoryEntry {
                    job_id,
                    started_at,
                    completed_at,
                    mode: plan.mode,
                    title,
                    counts,
                    output_dir: plan.settings.output_dir,
                    state,
                })
            })
            .collect()
    }

    pub fn unfinished_jobs(&self) -> Result<Vec<JobSnapshot>, AppError> {
        let rows = self.connect()?.query(
            "SELECT id FROM jobs WHERE state IN ('planned', 'running', 'paused', 'waiting', 'cancelling', 'interrupted') ORDER BY started_at DESC",
            &[],
        )?;
        rows.into_iter()
            .map(|row| self.get_job_snapshot(text(&row, 0)))
            .collect()
    }

    pub fn reset_failed_items(&self, job_id: &str) -> Result<usize, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            let failed = connection.query(
                "SELECT item_id FROM job_items WHERE job_id = ? AND outcome = 'failed'",
                &[job_id.into()],
            )?;
            for row in &failed {
                let item_id = text(row, 0);
                connection.execute(
                    "UPDATE job_items SET state = 'queued', outcome = NULL, message = 'Queued for retry', error_json = NULL WHERE job_id = ? AND item_id = ?",
                    &[job_id.into(), item_id.into()],
                )?;
                connection.execute(
                    "UPDATE tasks SET state = CASE WHEN state IN ('succeeded', 'reused') THEN state ELSE 'pending' END, progress_json = NULL, message = CASE WHEN state IN ('succeeded', 'reused') THEN message ELSE '' END, error_json = NULL, finished_at = CASE WHEN state IN ('succeeded', 'reused') THEN finished_at ELSE NULL END WHERE job_id = ? AND item_id = ?",
                    &[job_id.into(), item_id.into()],
                )?;
            }
            Ok(failed.len())
        })
    }

    pub(crate) fn tasks_for_item(
        &self,
        job_id: &str,
        item_id: &str,
    ) -> Result<Vec<TaskSnapshot>, AppError> {
        let rows = self.connect()?.query(
            "SELECT id, kind, resource, state, depends_json, attempt, max_attempts, weight, progress_json, message, error_json, started_at, finished_at FROM tasks WHERE job_id = ? AND item_id = ? ORDER BY rowid",
            &[job_id.into(), item_id.into()],
        )?;
        rows.into_iter()
            .map(|row| {
                Ok(TaskSnapshot {
                    id: text(&row, 0).to_string(),
                    item_id: item_id.to_string(),
                    kind: parse_enum(text(&row, 1))?,
                    resource: parse_enum(text(&row, 2))?,
                    state: parse_enum(text(&row, 3))?,
                    depends_on: from_json(text(&row, 4))?,
                    attempt: integer(&row, 5).max(0) as u32,
                    max_attempts: integer(&row, 6).max(0) as u32,
                    weight: real(&row, 7),
                    progress: optional_text(&row, 8).map(from_json).transpose()?,
                    message: text(&row, 9).to_string(),
                    error: optional_text(&row, 10).map(from_json).transpose()?,
                    started_at: optional_text(&row, 11).map(parse_time).transpose()?,
                    finished_at: optional_text(&row, 12).map(parse_time).transpose()?,
                })
            })
            .collect()
    }
}

fn item_progress(tasks: &[TaskSnapshot], outcome: Option<TerminalOutcome>) -> ProgressMetric {
    if outcome.is_some() || tasks.is_empty() {
        return ProgressMetric::fraction(1.0, 1.0, "item");
    }
    let total = tasks.iter().map(|task| task.weight).sum::<f64>().max(1.0);
    let complete = tasks
        .iter()
        .map(|task| {
            if task.state.successful() {
                task.weight
            } else if matches!(task.state, TaskState::Running | TaskState::Waiting) {
                task.progress
                    .as_ref()
                    .and_then(ProgressMetric::percent)
                    .map(|percent| task.weight * percent / 100.0)
                    .unwrap_or(0.0)
            } else {
                0.0
            }
        })
        .sum::<f64>();
    ProgressMetric::fraction(complete.min(total), total, "work")
}

fn overall_progress(items: &[ItemSnapshot]) -> ProgressMetric {
    if items.is_empty() {
        return ProgressMetric::fraction(0.0, 0.0, "items");
    }
    let complete = items
        .iter()
        .map(|item| item.progress.percent().unwrap_or(0.0) / 100.0)
        .sum::<f64>();
    ProgressMetric::fraction(complete, items.len() as f64, "items")
}

fn job_counts(items: &[ItemSnapshot], planned: usize) -> JobCounts {
    let mut counts = JobCounts {
        planned,
        ..JobCounts::default()
    };
    for item in items {
        match item.outcome {
            Some(TerminalOutcome::Complete) => counts.complete += 1,
            Some(TerminalOutcome::Reused) => counts.reused += 1,
            Some(TerminalOutcome::Skipped) => counts.skipped += 1,
            Some(TerminalOutcome::Failed) => counts.failed += 1,
            Some(TerminalOutcome::Cancelled) => counts.cancelled += 1,
            None => counts.running += 1,
        }
    }
    counts
}

fn text(row: &[SqlValue], index: usize) -> &str {
    row.get(index).and_then(SqlValue::text).unwrap_or_default()
}

fn optional_text(row: &[SqlValue], index: usize) -> Option<&str> {
    row.get(index).and_then(SqlValue::text)
}

fn integer(row: &[SqlValue], index: usize) -> i64 {
    row.get(index)
        .and_then(SqlValue::integer)
        .unwrap_or_default()
}

fn real(row: &[SqlValue], index: usize) -> f64 {
    match row.get(index) {
        Some(SqlValue::Real(value)) => *value,
        Some(SqlValue::Integer(value)) => *value as f64,
        _ => 0.0,
    }
}
