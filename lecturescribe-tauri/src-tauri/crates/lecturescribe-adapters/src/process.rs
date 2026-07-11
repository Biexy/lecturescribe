use lecturescribe_core::{AppError, ErrorCategory};
use lecturescribe_engine::{cancelled_error, JobControl};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::io::AsRawHandle;
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const MAX_CAPTURE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub current_dir: Option<PathBuf>,
    pub timeout: Duration,
}

impl CommandSpec {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            current_dir: None,
            timeout: Duration::from_secs(30 * 60),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Stdout,
    Stderr,
}

#[derive(Debug)]
pub struct CommandResult {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_streaming(
    spec: &CommandSpec,
    control: &JobControl,
    on_line: &mut dyn FnMut(StreamKind, &str),
) -> Result<CommandResult, AppError> {
    control.checkpoint()?;
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    if let Some(current_dir) = &spec.current_dir {
        command.current_dir(current_dir);
    }
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);

    let mut child = command.spawn().map_err(|error| {
        AppError::new(
            "process_start_failed",
            ErrorCategory::Setup,
            "LectureScribe could not start a required media tool.",
            format!("{}: {error}", spec.program.display()),
        )
    })?;
    let process_job = ProcessJob::attach(&child);
    let (sender, receiver) = mpsc::channel::<(StreamKind, String)>();
    if let Some(stdout) = child.stdout.take() {
        spawn_reader(stdout, StreamKind::Stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_reader(stderr, StreamKind::Stderr, sender.clone());
    }
    drop(sender);

    let started = Instant::now();
    let mut stdout = String::new();
    let mut stderr = String::new();
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok((kind, line)) => {
                on_line(kind, &line);
                match kind {
                    StreamKind::Stdout => append_limited(&mut stdout, &line),
                    StreamKind::Stderr => append_limited(&mut stderr, &line),
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {}
        }

        if control.cancelled() {
            terminate_process_tree(&mut child, process_job.as_ref());
            return Err(cancelled_error());
        }
        if control.paused() {
            control.checkpoint()?;
        }
        if started.elapsed() > spec.timeout {
            terminate_process_tree(&mut child, process_job.as_ref());
            return Err(AppError::new(
                "process_timed_out",
                ErrorCategory::Media,
                "A media tool took too long and was stopped.",
                format!(
                    "{} exceeded a timeout of {} seconds",
                    spec.program.display(),
                    spec.timeout.as_secs()
                ),
            )
            .retryable("Existing temporary files were retained for a safe retry."));
        }

        if let Some(status) = child.try_wait().map_err(|error| {
            AppError::new(
                "process_wait_failed",
                ErrorCategory::Internal,
                "LectureScribe could not read a media tool's result.",
                error.to_string(),
            )
        })? {
            while let Ok((kind, line)) = receiver.try_recv() {
                on_line(kind, &line);
                match kind {
                    StreamKind::Stdout => append_limited(&mut stdout, &line),
                    StreamKind::Stderr => append_limited(&mut stderr, &line),
                }
            }
            return Ok(CommandResult {
                status,
                stdout,
                stderr,
            });
        }
    }
}

pub fn run_output(spec: &CommandSpec, control: &JobControl) -> Result<CommandResult, AppError> {
    run_streaming(spec, control, &mut |_, _| {})
}

fn spawn_reader(
    stream: impl std::io::Read + Send + 'static,
    kind: StreamKind,
    sender: mpsc::Sender<(StreamKind, String)>,
) {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines().map_while(Result::ok) {
            let _ = sender.send((kind, line));
        }
    });
}

fn append_limited(target: &mut String, line: &str) {
    if target.len() >= MAX_CAPTURE_BYTES {
        return;
    }
    let available = MAX_CAPTURE_BYTES - target.len();
    let value = if line.len() > available {
        &line[..line.floor_char_boundary(available)]
    } else {
        line
    };
    target.push_str(value);
    target.push('\n');
}

fn terminate_process_tree(child: &mut Child, job: Option<&ProcessJob>) {
    #[cfg(target_os = "windows")]
    if let Some(job) = job {
        job.terminate();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "windows")]
struct ProcessJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(target_os = "windows")]
impl ProcessJob {
    fn attach(child: &Child) -> Option<Self> {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &information as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            let assigned = AssignProcessToJobObject(handle, child.as_raw_handle() as _);
            if configured == 0 || assigned == 0 {
                CloseHandle(handle);
                None
            } else {
                Some(Self { handle })
            }
        }
    }

    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1);
        }
    }
}

#[cfg(target_os = "windows")]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(not(target_os = "windows"))]
struct ProcessJob;

#[cfg(not(target_os = "windows"))]
impl ProcessJob {
    fn attach(_child: &Child) -> Option<Self> {
        None
    }
}

#[derive(Debug)]
pub struct SleepGuard;

impl SleepGuard {
    pub fn acquire() -> Self {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::System::Power::{
                SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
            };
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
        Self
    }
}

impl Drop for SleepGuard {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        unsafe {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
            SetThreadExecutionState(ES_CONTINUOUS);
        }
    }
}
