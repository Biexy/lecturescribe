pub(crate) mod files;
pub(crate) mod jobs;
pub(crate) mod setup;

use lecturescribe_core::{AppError, ErrorCategory};

pub(crate) async fn blocking<T, F>(operation: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| {
            AppError::new(
                "background_task_failed",
                ErrorCategory::Internal,
                "LectureScribe could not complete the background operation.",
                error.to_string(),
            )
        })?
}
