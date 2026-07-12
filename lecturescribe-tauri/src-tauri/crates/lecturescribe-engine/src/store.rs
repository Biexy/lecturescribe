use crate::db::{Connection, SqlValue};
use crate::schema::{CURRENT_SCHEMA_VERSION, SCHEMA};
use chrono::Utc;
use lecturescribe_core::{
    AppError, AppEvent, AppSettings, ArtifactKind, ArtifactRecord, ErrorCategory, EventType,
    ItemState, JobState, PreviewSnapshot, ProgressMetric, RunPlan, RunSummary, TaskState,
    TerminalOutcome, EVENT_SCHEMA_VERSION,
};
use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Store {
    pub(crate) path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct TaskTransition {
    pub job_id: String,
    pub item_id: String,
    pub task_id: String,
    pub task_state: TaskState,
    pub item_state: ItemState,
    pub progress: Option<ProgressMetric>,
    pub attempt: u32,
    pub message: String,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntry {
    pub cache_key: String,
    pub item_id: String,
    pub kind: ArtifactKind,
    pub path: String,
    pub checksum: String,
    pub size_bytes: u64,
    pub completed: bool,
    pub last_used_at: chrono::DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Store {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, AppError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                AppError::new(
                    "database_folder_failed",
                    ErrorCategory::Database,
                    "LectureScribe could not create its local data folder.",
                    error.to_string(),
                )
            })?;
        }
        if path.exists() {
            let backup = path.with_extension("sqlite3.backup");
            if !backup.exists() {
                let _ = fs::copy(&path, backup);
            }
        }
        let store = Self { path };
        store.initialize()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn integrity_ok(&self) -> bool {
        self.connect()
            .and_then(|connection| {
                let rows = connection.query("PRAGMA quick_check", &[])?;
                Ok(rows
                    .first()
                    .and_then(|row| row.first())
                    .and_then(SqlValue::text)
                    == Some("ok"))
            })
            .unwrap_or(false)
    }

    pub fn save_preview(&self, preview: &PreviewSnapshot) -> Result<(), AppError> {
        self.connect()?.execute(
            "INSERT OR REPLACE INTO previews(id, created_at, snapshot_json) VALUES(?, ?, ?)",
            &[
                preview.id.clone().into(),
                preview.created_at.to_rfc3339().into(),
                to_json(preview)?.into(),
            ],
        )?;
        Ok(())
    }

    pub fn get_preview(&self, id: &str) -> Result<PreviewSnapshot, AppError> {
        self.get_json(
            "SELECT snapshot_json FROM previews WHERE id = ?",
            &[id.into()],
            "preview_not_found",
            "That queue preview is no longer available. Refresh the queue.",
        )
    }

    pub fn save_plan(&self, plan: &RunPlan) -> Result<(), AppError> {
        self.connect()?.execute(
            "INSERT OR REPLACE INTO plans(id, preview_id, created_at, plan_json) VALUES(?, ?, ?, ?)",
            &[
                plan.id.clone().into(),
                plan.preview_id.clone().into(),
                plan.created_at.to_rfc3339().into(),
                to_json(plan)?.into(),
            ],
        )?;
        Ok(())
    }

    pub fn get_plan(&self, id: &str) -> Result<RunPlan, AppError> {
        self.get_json(
            "SELECT plan_json FROM plans WHERE id = ?",
            &[id.into()],
            "plan_not_found",
            "That run plan is no longer available. Review the queue again.",
        )
    }

    pub fn create_job(&self, plan: &RunPlan) -> Result<String, AppError> {
        if !plan.blocking_errors.is_empty() {
            return Err(plan.blocking_errors[0].clone());
        }
        if plan.runnable_count == 0 {
            return Err(AppError::new(
                "plan_has_no_runnable_items",
                ErrorCategory::Input,
                "None of the selected items can run in this mode.",
                "The plan contained no runnable task graph.",
            ));
        }
        self.save_plan(plan)?;
        let connection = self.connect()?;
        let job_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let (job_id, created) = connection.transaction(|connection| {
            let existing = connection.query(
                "SELECT id FROM jobs WHERE plan_id = ? AND state IN ('planned', 'running', 'paused', 'waiting', 'cancelling', 'interrupted') ORDER BY started_at DESC LIMIT 1",
                &[plan.id.clone().into()],
            )?;
            if let Some(row) = existing.first() {
                return Ok((row[0].text().unwrap_or_default().to_string(), false));
            }
            connection.execute(
                "INSERT INTO jobs(id, plan_id, state, sequence, started_at, message) VALUES(?, ?, ?, 0, ?, ?)",
                &[
                    job_id.clone().into(),
                    plan.id.clone().into(),
                    enum_name(JobState::Planned)?.into(),
                    now.clone().into(),
                    "Run created".into(),
                ],
            )?;
            for planned in &plan.items {
                let (state, outcome, message) = match planned.action {
                    lecturescribe_core::PlannedAction::Excluded => (
                        ItemState::Excluded,
                        Some(TerminalOutcome::Skipped),
                        planned.reason.clone(),
                    ),
                    lecturescribe_core::PlannedAction::Blocked => (
                        ItemState::Blocked,
                        Some(TerminalOutcome::Failed),
                        planned.reason.clone(),
                    ),
                    _ => (ItemState::Queued, None, "Queued".to_string()),
                };
                connection.execute(
                    "INSERT INTO job_items(job_id, item_id, ordinal, state, outcome, message, error_json, item_json) VALUES(?, ?, ?, ?, ?, ?, ?, ?)",
                    &[
                        job_id.clone().into(),
                        planned.item.id.clone().into(),
                        (planned.ordinal as i64).into(),
                        enum_name(state)?.into(),
                        optional_enum(outcome)?,
                        message.into(),
                        SqlValue::Null,
                        to_json(planned)?.into(),
                    ],
                )?;
                for task in &planned.tasks {
                    connection.execute(
                        "INSERT INTO tasks(id, job_id, item_id, kind, resource, state, depends_json, idempotency_key, max_attempts, weight) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                        &[
                            task.id.clone().into(),
                            job_id.clone().into(),
                            task.item_id.clone().into(),
                            enum_name(task.kind)?.into(),
                            enum_name(task.resource)?.into(),
                            enum_name(TaskState::Pending)?.into(),
                            to_json(&task.depends_on)?.into(),
                            task.idempotency_key.clone().into(),
                            (task.max_attempts as i64).into(),
                            task.weight.into(),
                        ],
                    )?;
                }
            }
            Ok((job_id.clone(), true))
        })?;
        if created {
            self.append_event(AppEvent {
                schema_version: EVENT_SCHEMA_VERSION,
                sequence: 0,
                occurred_at: Utc::now(),
                job_id: job_id.clone(),
                item_id: None,
                task_id: None,
                event_type: EventType::JobState,
                state: Some("planned".to_string()),
                progress: None,
                attempt: None,
                message: "Run created".to_string(),
                error: None,
            })?;
        }
        Ok(job_id)
    }

    pub fn claim_task(&self, transition: TaskTransition) -> Result<Option<AppEvent>, AppError> {
        if transition.task_state != TaskState::Running {
            return Err(AppError::new(
                "task_claim_invalid_state",
                ErrorCategory::Internal,
                "LectureScribe could not start this task safely.",
                "Task claims must transition a runnable task to running.",
            ));
        }
        let connection = self.connect()?;
        connection.transaction(|connection| {
            let now = Utc::now();
            let error_json = optional_json(transition.error.as_ref())?;
            let claimed = connection.execute(
                "UPDATE tasks SET state = ?, attempt = ?, progress_json = ?, message = ?, error_json = ?, started_at = COALESCE(started_at, ?) WHERE job_id = ? AND id = ? AND state IN ('pending', 'ready', 'interrupted')",
                &[
                    enum_name(TaskState::Running)?.into(),
                    (transition.attempt as i64).into(),
                    optional_json(transition.progress.as_ref())?,
                    transition.message.clone().into(),
                    error_json.clone(),
                    now.to_rfc3339().into(),
                    transition.job_id.clone().into(),
                    transition.task_id.clone().into(),
                ],
            )?;
            if claimed != 1 {
                return Ok(None);
            }
            connection.execute(
                "UPDATE job_items SET state = ?, message = ?, error_json = ? WHERE job_id = ? AND item_id = ?",
                &[
                    enum_name(transition.item_state)?.into(),
                    transition.message.clone().into(),
                    error_json,
                    transition.job_id.clone().into(),
                    transition.item_id.clone().into(),
                ],
            )?;
            append_event_on(
                connection,
                AppEvent {
                    schema_version: EVENT_SCHEMA_VERSION,
                    sequence: 0,
                    occurred_at: now,
                    job_id: transition.job_id,
                    item_id: Some(transition.item_id),
                    task_id: Some(transition.task_id),
                    event_type: EventType::Progress,
                    state: Some(enum_name(TaskState::Running)?),
                    progress: transition.progress,
                    attempt: Some(transition.attempt),
                    message: transition.message,
                    error: transition.error,
                },
            )
            .map(Some)
        })
    }

    pub fn transition_task(&self, transition: TaskTransition) -> Result<AppEvent, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            let now = Utc::now();
            let terminal = transition.task_state.terminal();
            let progress_json = optional_json(transition.progress.as_ref())?;
            let error_json = optional_json(transition.error.as_ref())?;
            let started_at = if transition.task_state == TaskState::Running {
                SqlValue::Text(now.to_rfc3339())
            } else {
                SqlValue::Null
            };
            let finished_at = if terminal {
                SqlValue::Text(now.to_rfc3339())
            } else {
                SqlValue::Null
            };
            connection.execute(
                "UPDATE tasks SET state = ?, attempt = ?, progress_json = ?, message = ?, error_json = ?, started_at = COALESCE(started_at, ?), finished_at = COALESCE(?, finished_at) WHERE job_id = ? AND id = ?",
                &[
                    enum_name(transition.task_state)?.into(),
                    (transition.attempt as i64).into(),
                    progress_json,
                    transition.message.clone().into(),
                    error_json.clone(),
                    started_at,
                    finished_at,
                    transition.job_id.clone().into(),
                    transition.task_id.clone().into(),
                ],
            )?;
            connection.execute(
                "UPDATE job_items SET state = ?, message = ?, error_json = ? WHERE job_id = ? AND item_id = ?",
                &[
                    enum_name(transition.item_state)?.into(),
                    transition.message.clone().into(),
                    error_json,
                    transition.job_id.clone().into(),
                    transition.item_id.clone().into(),
                ],
            )?;
            append_event_on(
                connection,
                AppEvent {
                    schema_version: EVENT_SCHEMA_VERSION,
                    sequence: 0,
                    occurred_at: now,
                    job_id: transition.job_id,
                    item_id: Some(transition.item_id),
                    task_id: Some(transition.task_id),
                    event_type: if transition.error.is_some() {
                        EventType::Problem
                    } else if transition.progress.is_some() {
                        EventType::Progress
                    } else {
                        EventType::TaskState
                    },
                    state: Some(enum_name(transition.task_state)?),
                    progress: transition.progress,
                    attempt: Some(transition.attempt),
                    message: transition.message,
                    error: transition.error,
                },
            )
        })
    }

    pub fn set_item_outcome(
        &self,
        job_id: &str,
        item_id: &str,
        state: ItemState,
        outcome: TerminalOutcome,
        message: &str,
        error: Option<&AppError>,
    ) -> Result<AppEvent, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            connection.execute(
                "UPDATE job_items SET state = ?, outcome = ?, message = ?, error_json = ? WHERE job_id = ? AND item_id = ?",
                &[
                    enum_name(state)?.into(),
                    enum_name(outcome)?.into(),
                    message.into(),
                    optional_json(error)?,
                    job_id.into(),
                    item_id.into(),
                ],
            )?;
            append_event_on(
                connection,
                AppEvent {
                    schema_version: EVENT_SCHEMA_VERSION,
                    sequence: 0,
                    occurred_at: Utc::now(),
                    job_id: job_id.to_string(),
                    item_id: Some(item_id.to_string()),
                    task_id: None,
                    event_type: EventType::ItemState,
                    state: Some(enum_name(state)?),
                    progress: None,
                    attempt: None,
                    message: message.to_string(),
                    error: error.cloned(),
                },
            )
        })
    }

    pub fn set_job_state(
        &self,
        job_id: &str,
        state: JobState,
        message: &str,
        summary: Option<&RunSummary>,
    ) -> Result<AppEvent, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            let finished = matches!(
                state,
                JobState::Complete | JobState::Failed | JobState::Cancelled
            )
            .then(|| Utc::now().to_rfc3339());
            connection.execute(
                "UPDATE jobs SET state = ?, message = ?, finished_at = COALESCE(?, finished_at), summary_json = COALESCE(?, summary_json) WHERE id = ?",
                &[
                    enum_name(state)?.into(),
                    message.into(),
                    finished.map(SqlValue::Text).unwrap_or(SqlValue::Null),
                    optional_json(summary)?,
                    job_id.into(),
                ],
            )?;
            append_event_on(
                connection,
                AppEvent {
                    schema_version: EVENT_SCHEMA_VERSION,
                    sequence: 0,
                    occurred_at: Utc::now(),
                    job_id: job_id.to_string(),
                    item_id: None,
                    task_id: None,
                    event_type: if summary.is_some() {
                        EventType::Summary
                    } else {
                        EventType::JobState
                    },
                    state: Some(enum_name(state)?),
                    progress: None,
                    attempt: None,
                    message: message.to_string(),
                    error: None,
                },
            )
        })
    }

    pub fn append_event(&self, event: AppEvent) -> Result<AppEvent, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| append_event_on(connection, event))
    }

    pub fn events_since(&self, job_id: &str, sequence: i64) -> Result<Vec<AppEvent>, AppError> {
        self.connect()?
            .query(
                "SELECT event_json FROM events WHERE job_id = ? AND sequence > ? ORDER BY sequence",
                &[job_id.into(), sequence.into()],
            )?
            .into_iter()
            .map(|row| from_json(row[0].text().unwrap_or_default()))
            .collect()
    }

    pub fn record_artifact(&self, artifact: &ArtifactRecord) -> Result<AppEvent, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            connection.execute(
                "INSERT OR REPLACE INTO artifacts(id, job_id, item_id, task_id, kind, path, checksum, size_bytes, created_at, artifact_json) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                &[
                    artifact.id.clone().into(),
                    artifact.job_id.clone().into(),
                    artifact.item_id.clone().into(),
                    artifact.task_id.clone().into(),
                    enum_name(artifact.kind)?.into(),
                    artifact.path.clone().into(),
                    artifact.checksum.clone().into(),
                    (artifact.size_bytes as i64).into(),
                    artifact.created_at.to_rfc3339().into(),
                    to_json(artifact)?.into(),
                ],
            )?;
            append_event_on(
                connection,
                AppEvent {
                    schema_version: EVENT_SCHEMA_VERSION,
                    sequence: 0,
                    occurred_at: Utc::now(),
                    job_id: artifact.job_id.clone(),
                    item_id: Some(artifact.item_id.clone()),
                    task_id: Some(artifact.task_id.clone()),
                    event_type: EventType::Artifact,
                    state: None,
                    progress: None,
                    attempt: None,
                    message: "Artifact verified".to_string(),
                    error: None,
                },
            )
        })
    }

    pub fn artifacts_for_item(
        &self,
        job_id: &str,
        item_id: &str,
    ) -> Result<Vec<ArtifactRecord>, AppError> {
        self.connect()?
            .query(
                "SELECT artifact_json FROM artifacts WHERE job_id = ? AND item_id = ? ORDER BY created_at",
                &[job_id.into(), item_id.into()],
            )?
            .into_iter()
            .map(|row| from_json(row[0].text().unwrap_or_default()))
            .collect()
    }

    pub fn latest_artifact(
        &self,
        item_id: &str,
        kind: ArtifactKind,
    ) -> Result<Option<ArtifactRecord>, AppError> {
        let rows = self.connect()?.query(
            "SELECT artifact_json FROM artifacts WHERE item_id = ? AND kind = ? ORDER BY created_at DESC LIMIT 1",
            &[item_id.into(), enum_name(kind)?.into()],
        )?;
        rows.first()
            .map(|row| from_json(row[0].text().unwrap_or_default()))
            .transpose()
    }

    pub fn put_cache(&self, entry: &CacheEntry) -> Result<(), AppError> {
        self.connect()?.execute(
            "INSERT OR REPLACE INTO cache_entries(cache_key, item_id, kind, path, checksum, size_bytes, completed, last_used_at, metadata_json) VALUES(?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                entry.cache_key.clone().into(),
                entry.item_id.clone().into(),
                enum_name(entry.kind)?.into(),
                entry.path.clone().into(),
                entry.checksum.clone().into(),
                (entry.size_bytes as i64).into(),
                (entry.completed as i64).into(),
                entry.last_used_at.to_rfc3339().into(),
                to_json(&entry.metadata)?.into(),
            ],
        )?;
        Ok(())
    }

    pub fn get_cache(&self, key: &str) -> Result<Option<CacheEntry>, AppError> {
        let rows = self.connect()?.query(
            "SELECT item_id, kind, path, checksum, size_bytes, completed, last_used_at, metadata_json FROM cache_entries WHERE cache_key = ?",
            &[key.into()],
        )?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        Ok(Some(CacheEntry {
            cache_key: key.to_string(),
            item_id: row[0].text().unwrap_or_default().to_string(),
            kind: parse_enum(row[1].text().unwrap_or_default())?,
            path: row[2].text().unwrap_or_default().to_string(),
            checksum: row[3].text().unwrap_or_default().to_string(),
            size_bytes: row[4].integer().unwrap_or_default().max(0) as u64,
            completed: row[5].integer().unwrap_or_default() != 0,
            last_used_at: parse_time(row[6].text().unwrap_or_default())?,
            metadata: from_json(row[7].text().unwrap_or("{}"))?,
        }))
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        self.connect()?.execute(
            "INSERT OR REPLACE INTO settings(key, value_json, updated_at) VALUES('app', ?, ?)",
            &[to_json(settings)?.into(), Utc::now().to_rfc3339().into()],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Option<AppSettings>, AppError> {
        let rows = self
            .connect()?
            .query("SELECT value_json FROM settings WHERE key = 'app'", &[])?;
        rows.first()
            .map(|row| from_json(row[0].text().unwrap_or_default()))
            .transpose()
    }

    pub fn mark_interrupted(&self) -> Result<usize, AppError> {
        let connection = self.connect()?;
        connection.transaction(|connection| {
            connection.execute(
                "UPDATE tasks SET state = 'cancelled', message = 'Cancelled by application exit', finished_at = COALESCE(finished_at, ?) WHERE job_id IN (SELECT id FROM jobs WHERE state = 'cancelling') AND state NOT IN ('succeeded', 'reused', 'skipped', 'failed', 'cancelled')",
                &[Utc::now().to_rfc3339().into()],
            )?;
            connection.execute(
                "UPDATE job_items SET state = 'cancelled', outcome = 'cancelled', message = 'Run cancelled by application exit' WHERE job_id IN (SELECT id FROM jobs WHERE state = 'cancelling') AND outcome IS NULL",
                &[],
            )?;
            let cancelled = connection.execute(
                "UPDATE jobs SET state = 'cancelled', message = 'Run cancelled by application exit', finished_at = COALESCE(finished_at, ?) WHERE state = 'cancelling'",
                &[Utc::now().to_rfc3339().into()],
            )?;
            let interrupted = connection.execute(
                "UPDATE jobs SET state = 'interrupted', message = 'Run interrupted; verified work is available to resume' WHERE state IN ('running', 'waiting')",
                &[],
            )?;
            connection.execute(
                "UPDATE tasks SET state = 'interrupted', message = 'Interrupted by application exit' WHERE state IN ('running', 'waiting') AND job_id IN (SELECT id FROM jobs WHERE state = 'interrupted')",
                &[],
            )?;
            Ok(cancelled + interrupted)
        })
    }

    fn initialize(&self) -> Result<(), AppError> {
        let connection = self.connect()?;
        connection.execute_batch(SCHEMA)?;
        let rows = connection.query("PRAGMA user_version", &[])?;
        let version = rows
            .first()
            .and_then(|row| row.first())
            .and_then(SqlValue::integer)
            .unwrap_or_default();
        if version != CURRENT_SCHEMA_VERSION {
            return Err(AppError::new(
                "database_schema_unsupported",
                ErrorCategory::Database,
                "The local database version is not supported by this build.",
                format!("Expected schema {CURRENT_SCHEMA_VERSION}, found {version}"),
            ));
        }
        self.mark_interrupted()?;
        Ok(())
    }

    pub(crate) fn connect(&self) -> Result<Connection, AppError> {
        let connection = Connection::open(&self.path)?;
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        Ok(connection)
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        sql: &str,
        params: &[SqlValue],
        code: &str,
        message: &str,
    ) -> Result<T, AppError> {
        let rows = self.connect()?.query(sql, params)?;
        let value = rows
            .first()
            .and_then(|row| row.first())
            .and_then(SqlValue::text)
            .ok_or_else(|| AppError::new(code, ErrorCategory::Database, message, message))?;
        from_json(value)
    }
}

fn append_event_on(connection: &Connection, mut event: AppEvent) -> Result<AppEvent, AppError> {
    let rows = connection.query(
        "SELECT sequence FROM jobs WHERE id = ?",
        &[event.job_id.clone().into()],
    )?;
    let sequence = rows
        .first()
        .and_then(|row| row.first())
        .and_then(SqlValue::integer)
        .ok_or_else(|| {
            AppError::new(
                "job_not_found",
                ErrorCategory::Database,
                "That run is no longer available.",
                "No job row existed while appending an event.",
            )
        })?
        + 1;
    event.sequence = sequence;
    connection.execute(
        "UPDATE jobs SET sequence = ? WHERE id = ?",
        &[sequence.into(), event.job_id.clone().into()],
    )?;
    connection.execute(
        "INSERT INTO events(job_id, sequence, occurred_at, event_type, item_id, task_id, event_json) VALUES(?, ?, ?, ?, ?, ?, ?)",
        &[
            event.job_id.clone().into(),
            sequence.into(),
            event.occurred_at.to_rfc3339().into(),
            enum_name(event.event_type)?.into(),
            event.item_id.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null),
            event.task_id.clone().map(SqlValue::Text).unwrap_or(SqlValue::Null),
            to_json(&event)?.into(),
        ],
    )?;
    Ok(event)
}

pub(crate) fn enum_name<T: Serialize>(value: T) -> Result<String, AppError> {
    let value = serde_json::to_value(value).map_err(serialization_error)?;
    value
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| serialization_error("Enum did not serialize to a string"))
}

pub(crate) fn parse_enum<T: DeserializeOwned>(value: &str) -> Result<T, AppError> {
    from_json(&format!("\"{value}\""))
}

pub(crate) fn to_json<T: Serialize + ?Sized>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(serialization_error)
}

pub(crate) fn from_json<T: DeserializeOwned>(value: &str) -> Result<T, AppError> {
    serde_json::from_str(value).map_err(serialization_error)
}

pub(crate) fn parse_time(value: &str) -> Result<chrono::DateTime<Utc>, AppError> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(serialization_error)
}

fn optional_json<T: Serialize>(value: Option<&T>) -> Result<SqlValue, AppError> {
    value
        .map(|value| to_json(value).map(SqlValue::Text))
        .transpose()
        .map(|value| value.unwrap_or(SqlValue::Null))
}

fn optional_enum<T: Serialize>(value: Option<T>) -> Result<SqlValue, AppError> {
    value
        .map(|value| enum_name(value).map(SqlValue::Text))
        .transpose()
        .map(|value| value.unwrap_or(SqlValue::Null))
}

fn serialization_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "database_serialization_failed",
        ErrorCategory::Database,
        "LectureScribe could not read its saved job state.",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use lecturescribe_core::{
        AppSettings, PlannedAction, PlannedItem, PreviewItem, PreviewSnapshot, ProviderKind,
        ResourceClass, RunMode, SourceKind, TaskKind, TaskSpec,
    };
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};
    use std::thread;

    fn test_store() -> Store {
        Store::open(
            std::env::temp_dir().join(format!("lecturescribe-store-{}.sqlite3", Uuid::new_v4())),
        )
        .unwrap()
    }

    fn test_plan(store: &Store, item_count: usize) -> RunPlan {
        let preview_id = Uuid::new_v4().to_string();
        let items = (0..item_count)
            .map(|ordinal| {
                let item_id = format!("item-{ordinal}");
                let item = PreviewItem {
                    id: item_id.clone(),
                    source_id: format!("source-{ordinal}"),
                    source_kind: SourceKind::LocalMedia,
                    provider: ProviderKind::Local,
                    source_group: "test".to_string(),
                    title: format!("Test item {ordinal}"),
                    source: format!("C:/test/{ordinal}.wav"),
                    canonical_source: format!("C:/test/{ordinal}.wav"),
                    url: None,
                    media_path: Some(format!("C:/test/{ordinal}.wav")),
                    existing_media_path: None,
                    existing_transcript_path: None,
                    thumbnail_url: None,
                    duration_seconds: None,
                    expected_media_name: None,
                    selected: true,
                    status: ItemState::Queued,
                    duplicate_of: None,
                    error: None,
                };
                let task = TaskSpec {
                    id: format!("task-{ordinal}"),
                    item_id: item_id.clone(),
                    kind: TaskKind::Reuse,
                    resource: ResourceClass::Filesystem,
                    depends_on: Vec::new(),
                    idempotency_key: format!("reuse-{ordinal}"),
                    max_attempts: 1,
                    weight: 1.0,
                };
                PlannedItem {
                    item,
                    ordinal: ordinal + 1,
                    action: PlannedAction::ReuseTranscript,
                    reason: "test".to_string(),
                    estimated_segments: 1,
                    estimated_requests: 0,
                    tasks: vec![task],
                    output_stem: format!("Test item {ordinal}"),
                }
            })
            .collect::<Vec<_>>();
        store
            .save_preview(&PreviewSnapshot {
                id: preview_id.clone(),
                created_at: Utc::now(),
                items: items.iter().map(|item| item.item.clone()).collect(),
                duplicate_count: 0,
                source_count: item_count,
                warnings: Vec::new(),
            })
            .unwrap();
        RunPlan {
            id: Uuid::new_v4().to_string(),
            preview_id,
            created_at: Utc::now(),
            mode: RunMode::Transcribe,
            settings: AppSettings::default(),
            batch_name: "test-batch".to_string(),
            batch_output_dir: "C:/test/output/test-batch".to_string(),
            items,
            selected_count: item_count,
            runnable_count: item_count,
            excluded_count: 0,
            blocked_count: 0,
            estimated_requests: 0,
            blocking_errors: Vec::new(),
        }
    }

    fn claim(job_id: &str, item_id: &str, task_id: &str) -> TaskTransition {
        TaskTransition {
            job_id: job_id.to_string(),
            item_id: item_id.to_string(),
            task_id: task_id.to_string(),
            task_state: TaskState::Running,
            item_state: ItemState::Reused,
            progress: Some(ProgressMetric::indeterminate("step")),
            attempt: 1,
            message: "Claimed".to_string(),
            error: None,
        }
    }

    #[test]
    fn concurrent_job_creation_reuses_one_active_job_per_plan() {
        let store = Arc::new(test_store());
        let plan = Arc::new(test_plan(&store, 1));
        let barrier = Arc::new(Barrier::new(8));
        let jobs = (0..8)
            .map(|_| {
                let store = store.clone();
                let plan = plan.clone();
                let barrier = barrier.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store.create_job(&plan).unwrap()
                })
            })
            .collect::<Vec<_>>();
        let job_ids = jobs
            .into_iter()
            .map(|job| job.join().unwrap())
            .collect::<HashSet<_>>();

        assert_eq!(job_ids.len(), 1);
    }

    #[test]
    fn concurrent_task_claims_start_exactly_one_task() {
        let store = Arc::new(test_store());
        let plan = test_plan(&store, 1);
        let job_id = store.create_job(&plan).unwrap();
        let barrier = Arc::new(Barrier::new(8));
        let claims = (0..8)
            .map(|_| {
                let store = store.clone();
                let barrier = barrier.clone();
                let job_id = job_id.clone();
                thread::spawn(move || {
                    barrier.wait();
                    store
                        .claim_task(claim(&job_id, "item-0", "task-0"))
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let claimed = claims
            .into_iter()
            .filter_map(|claim| claim.join().unwrap())
            .count();
        let snapshot = store.get_job_snapshot(&job_id).unwrap();

        assert_eq!(claimed, 1);
        assert_eq!(snapshot.items[0].tasks[0].state, TaskState::Running);
        assert_eq!(snapshot.items[0].tasks[0].attempt, 1);
    }

    #[test]
    fn restart_finishes_cancellation_without_losing_reused_work() {
        let store = test_store();
        let plan = test_plan(&store, 2);
        let job_id = store.create_job(&plan).unwrap();
        let reused = claim(&job_id, "item-0", "task-0");
        store.claim_task(reused.clone()).unwrap();
        store
            .transition_task(TaskTransition {
                task_state: TaskState::Reused,
                item_state: ItemState::Reused,
                progress: Some(ProgressMetric::fraction(1.0, 1.0, "step")),
                message: "Reused".to_string(),
                ..reused
            })
            .unwrap();
        store
            .set_item_outcome(
                &job_id,
                "item-0",
                ItemState::Reused,
                TerminalOutcome::Reused,
                "Reused",
                None,
            )
            .unwrap();
        store
            .claim_task(claim(&job_id, "item-1", "task-1"))
            .unwrap();
        store
            .set_job_state(
                &job_id,
                JobState::Cancelling,
                "Cancellation requested",
                None,
            )
            .unwrap();

        store.mark_interrupted().unwrap();
        let snapshot = store.get_job_snapshot(&job_id).unwrap();

        assert_eq!(snapshot.state, JobState::Cancelled);
        assert_eq!(snapshot.items[0].outcome, Some(TerminalOutcome::Reused));
        assert_eq!(snapshot.items[0].tasks[0].state, TaskState::Reused);
        assert_eq!(snapshot.items[1].outcome, Some(TerminalOutcome::Cancelled));
        assert_eq!(snapshot.items[1].tasks[0].state, TaskState::Cancelled);
    }
}
