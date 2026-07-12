use crate::migration::initial_settings;
use lecturescribe_adapters::{
    AppPaths, CredentialStore, GeminiClient, PipelineExecutor, SourceInspector, ToolResolver,
    TraceLogger,
};
use lecturescribe_core::{AppError, AppEvent, AppSettings};
use lecturescribe_engine::{EventSink, JobControl, JobRunner, Store};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub const APP_EVENT_NAME: &str = "lecturescribe-event";

pub struct AppState {
    pub paths: AppPaths,
    pub store: Arc<Store>,
    pub credentials: CredentialStore,
    pub tools: ToolResolver,
    pub inspector: SourceInspector,
    pub runner: JobRunner,
    pub logger: Arc<TraceLogger>,
    pub api_key_verified: AtomicBool,
    controls: Mutex<HashMap<String, Arc<JobControl>>>,
}

impl AppState {
    pub fn initialize(app: AppHandle) -> Result<Self, AppError> {
        let paths = AppPaths::discover();
        paths.ensure()?;
        let store = Arc::new(Store::open(paths.database_path.clone())?);
        let settings = initial_settings(&store, &paths)?;
        store.save_settings(&settings)?;

        let credentials = CredentialStore;
        let api_key_verified = credentials.gemini_key_verified();
        let tools = ToolResolver::new(paths.clone());
        let inspector = SourceInspector::new(store.clone(), tools.clone());
        let gemini = GeminiClient::new(credentials.clone())?;
        let pipeline = Arc::new(PipelineExecutor::new(
            paths.clone(),
            store.clone(),
            tools.clone(),
            gemini,
        ));
        let logger = Arc::new(TraceLogger::new(paths.logs_dir.clone()));
        let sink = Arc::new(TauriEventSink {
            app,
            logger: logger.clone(),
        });
        let runner = JobRunner::new(store.clone(), pipeline, sink);

        Ok(Self {
            paths,
            store,
            credentials,
            tools,
            inspector,
            runner,
            logger,
            api_key_verified: AtomicBool::new(api_key_verified),
            controls: Mutex::new(HashMap::new()),
        })
    }

    pub fn settings(&self) -> Result<AppSettings, AppError> {
        let settings = self.store.load_settings()?.unwrap_or_default();
        Ok(self.paths.settings_with_defaults(settings))
    }

    pub fn save_settings(&self, settings: AppSettings) -> Result<AppSettings, AppError> {
        let settings = self.paths.settings_with_defaults(settings);
        self.store.save_settings(&settings)?;
        Ok(settings)
    }

    pub fn set_control(&self, job_id: String, control: Arc<JobControl>) {
        if let Ok(mut controls) = self.controls.lock() {
            controls.insert(job_id, control);
        }
    }

    pub fn control(&self, job_id: &str) -> Option<Arc<JobControl>> {
        self.controls
            .lock()
            .ok()
            .and_then(|controls| controls.get(job_id).cloned())
    }

    pub fn remove_control(&self, job_id: &str) {
        if let Ok(mut controls) = self.controls.lock() {
            controls.remove(job_id);
        }
    }

    pub fn mark_api_verified(&self, verified: bool) {
        self.api_key_verified.store(verified, Ordering::SeqCst);
    }

    pub fn api_verified(&self) -> bool {
        self.api_key_verified.load(Ordering::SeqCst)
    }
}

struct TauriEventSink {
    app: AppHandle,
    logger: Arc<TraceLogger>,
}

impl EventSink for TauriEventSink {
    fn emit(&self, event: AppEvent) {
        self.logger.event(&event);
        let _ = self.app.emit(APP_EVENT_NAME, event);
    }
}
