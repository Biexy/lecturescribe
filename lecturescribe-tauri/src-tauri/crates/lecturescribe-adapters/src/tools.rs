use crate::paths::{executable_name, AppPaths};
use lecturescribe_core::{AppError, AppSettings, ErrorCategory, ToolReadiness, ToolStatus};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
#[cfg(target_os = "windows")]
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const YT_DLP_VERSION: &str = "2026.06.09";
pub const YT_DLP_SHA256: &str = "3a48cb955d55c8821b60ccbdbbc6f61bc958f2f3d3b7ad5eaf3d83a543293a27";
pub const YT_DLP_URL: &str =
    "https://github.com/yt-dlp/yt-dlp/releases/download/2026.06.09/yt-dlp.exe";

#[derive(Debug, Clone)]
pub struct ResolvedTool {
    pub path: Option<PathBuf>,
    pub status: ToolStatus,
}

#[derive(Debug, Clone)]
pub struct ResolvedTools {
    pub ffmpeg: ResolvedTool,
    pub ffprobe: ResolvedTool,
    pub downloader: ResolvedTool,
}

#[derive(Debug, Clone)]
pub struct ToolResolver {
    paths: AppPaths,
}

impl ToolResolver {
    pub fn new(paths: AppPaths) -> Self {
        Self { paths }
    }

    pub fn resolve(&self, settings: &AppSettings) -> ResolvedTools {
        let ffmpeg = self.resolve_named(
            "FFmpeg",
            "ffmpeg",
            &settings.ffmpeg_path,
            &["-version"],
            |output| output.to_ascii_lowercase().starts_with("ffmpeg version"),
        );
        let derived_ffprobe = ffmpeg
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|parent| parent.join(executable_name("ffprobe")))
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let ffprobe_setting = if settings.ffprobe_path.trim().is_empty() {
            derived_ffprobe
        } else {
            settings.ffprobe_path.clone()
        };
        let ffprobe = self.resolve_named(
            "FFprobe",
            "ffprobe",
            &ffprobe_setting,
            &["-version"],
            |output| output.to_ascii_lowercase().starts_with("ffprobe version"),
        );
        let downloader = self.resolve_downloader(settings);
        ResolvedTools {
            ffmpeg,
            ffprobe,
            downloader,
        }
    }

    pub fn install_downloader(&self) -> Result<ResolvedTool, AppError> {
        self.paths.ensure()?;
        let target = self.paths.tools_dir.join(executable_name("yt-dlp"));
        let temporary = target.with_extension("exe.download");
        let client = reqwest::blocking::Client::builder()
            .user_agent("LectureScribe/0.2")
            .timeout(std::time::Duration::from_secs(180))
            .build()
            .map_err(network_error)?;
        let mut response = client.get(YT_DLP_URL).send().map_err(network_error)?;
        if !response.status().is_success() {
            return Err(AppError::new(
                "downloader_http_failed",
                ErrorCategory::Network,
                "The Downloader could not be downloaded.",
                format!("HTTP {}", response.status()),
            )
            .retryable("The existing Downloader, if any, was left unchanged."));
        }
        let mut file = fs::File::create(&temporary).map_err(filesystem_error)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = response.read(&mut buffer).map_err(network_error)?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(filesystem_error)?;
            hasher.update(&buffer[..read]);
        }
        file.flush().map_err(filesystem_error)?;
        file.sync_all().map_err(filesystem_error)?;
        let checksum = hex::encode(hasher.finalize());
        if checksum != YT_DLP_SHA256 {
            let _ = fs::remove_file(&temporary);
            return Err(AppError::new(
                "downloader_checksum_failed",
                ErrorCategory::Setup,
                "The downloaded Downloader failed its security check.",
                format!("Expected {YT_DLP_SHA256}, received {checksum}"),
            ));
        }
        atomic_replace(&temporary, &target)?;
        Ok(self.resolve_named(
            "Downloader",
            "yt-dlp",
            &target.to_string_lossy(),
            &["--version"],
            |output| output.trim().starts_with(YT_DLP_VERSION),
        ))
    }

    pub fn output_writable(&self, output_dir: &Path) -> bool {
        if fs::create_dir_all(output_dir).is_err() {
            return false;
        }
        let test = output_dir.join(format!(
            ".lecturescribe-write-test-{}",
            uuid::Uuid::new_v4()
        ));
        match fs::write(&test, b"ok") {
            Ok(()) => {
                let _ = fs::remove_file(test);
                true
            }
            Err(_) => false,
        }
    }

    pub fn free_disk_bytes(&self, path: &Path) -> Option<u64> {
        free_disk_bytes(path)
    }

    fn resolve_downloader(&self, settings: &AppSettings) -> ResolvedTool {
        let candidates = self.downloader_candidates(settings);
        resolve_candidates(
            "Downloader",
            candidates,
            &["--version"],
            |output| {
                let value = output.trim();
                value.len() >= 8
                    && value
                        .chars()
                        .take(4)
                        .all(|character| character.is_ascii_digit())
            },
            Some(YT_DLP_VERSION),
        )
    }

    fn downloader_candidates(&self, settings: &AppSettings) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if !settings.downloader_path.trim().is_empty() {
            candidates.push(PathBuf::from(settings.downloader_path.trim()));
        }
        candidates.push(self.paths.tools_dir.join(executable_name("yt-dlp")));
        candidates.extend(self.paths.bundled_downloader_candidates());
        candidates.extend(common_tool_candidates("yt-dlp"));
        candidates.push(PathBuf::from(executable_name("yt-dlp")));
        candidates
    }

    fn resolve_named(
        &self,
        label: &str,
        executable: &str,
        configured: &str,
        version_args: &[&str],
        identity: impl Fn(&str) -> bool,
    ) -> ResolvedTool {
        let candidates = self.named_candidates(executable, configured);
        resolve_candidates(label, candidates, version_args, identity, None)
    }

    fn named_candidates(&self, executable: &str, configured: &str) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if !configured.trim().is_empty() {
            candidates.push(PathBuf::from(configured.trim()));
        }
        candidates.push(self.paths.tools_dir.join(executable_name(executable)));
        candidates.push(self.paths.install_dir.join(executable_name(executable)));
        candidates.extend(common_tool_candidates(executable));
        candidates.push(PathBuf::from(executable_name(executable)));
        candidates
    }
}

fn resolve_candidates(
    label: &str,
    candidates: Vec<PathBuf>,
    version_args: &[&str],
    identity: impl Fn(&str) -> bool,
    recommended_version: Option<&str>,
) -> ResolvedTool {
    let mut invalid = None;
    let mut seen = HashSet::new();
    for candidate in candidates {
        let path = locate_path_candidate(&candidate);
        if !seen.insert(candidate_identity(&path)) {
            continue;
        }
        if path.components().count() > 1 && !path.exists() {
            continue;
        }
        if let Some(output) = version_output(&path, version_args) {
            if !identity(&output) {
                invalid = Some((path, output));
                continue;
            }
            let version = output.lines().next().unwrap_or_default().trim().to_string();
            let outdated = recommended_version.is_some_and(|expected| !version.contains(expected));
            return ResolvedTool {
                path: Some(path.clone()),
                status: ToolStatus {
                    name: label.to_string(),
                    readiness: if outdated {
                        ToolReadiness::Outdated
                    } else {
                        ToolReadiness::Ready
                    },
                    version: Some(version),
                    path: Some(path.to_string_lossy().to_string()),
                    detail: if outdated {
                        format!("A newer verified {label} is available.")
                    } else {
                        format!("{label} is ready.")
                    },
                    fix_action: outdated.then(|| "update_downloader".to_string()),
                },
            };
        }
    }
    if let Some((path, output)) = invalid {
        return ResolvedTool {
            path: None,
            status: ToolStatus {
                name: label.to_string(),
                readiness: ToolReadiness::Invalid,
                version: output.lines().next().map(ToString::to_string),
                path: Some(path.to_string_lossy().to_string()),
                detail: format!("The selected file is not a valid {label} executable."),
                fix_action: Some(format!("choose_{}", label.to_ascii_lowercase())),
            },
        };
    }
    ResolvedTool {
        path: None,
        status: ToolStatus {
            name: label.to_string(),
            readiness: ToolReadiness::Missing,
            version: None,
            path: None,
            detail: format!("{label} is missing."),
            fix_action: Some(format!("install_{}", label.to_ascii_lowercase())),
        },
    }
}

fn locate_path_candidate(candidate: &Path) -> PathBuf {
    let directories = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    locate_path_candidate_in(candidate, &directories)
}

fn locate_path_candidate_in(candidate: &Path, directories: &[PathBuf]) -> PathBuf {
    if candidate.components().count() > 1 {
        return candidate.to_path_buf();
    }
    directories
        .iter()
        .map(|directory| directory.join(candidate))
        .find(|path| path.is_file())
        .unwrap_or_else(|| candidate.to_path_buf())
}

fn candidate_identity(path: &Path) -> String {
    let value = path.to_string_lossy().to_string();
    if cfg!(target_os = "windows") {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

#[cfg(target_os = "windows")]
fn common_tool_candidates(executable: &str) -> Vec<PathBuf> {
    common_tool_candidates_with(executable, |name| std::env::var_os(name))
}

#[cfg(target_os = "windows")]
fn common_tool_candidates_with(
    executable: &str,
    environment: impl Fn(&str) -> Option<OsString>,
) -> Vec<PathBuf> {
    let name = executable_name(executable);
    let is_ffmpeg_tool = matches!(
        executable.to_ascii_lowercase().as_str(),
        "ffmpeg" | "ffprobe"
    );
    let mut candidates = Vec::new();
    if let Some(root) = environment("ChocolateyInstall") {
        candidates.push(PathBuf::from(root).join("bin").join(&name));
    }
    if let Some(root) = environment("SCOOP") {
        candidates.push(PathBuf::from(root).join("shims").join(&name));
    }
    if let Some(root) = environment("USERPROFILE") {
        candidates.push(PathBuf::from(root).join("scoop").join("shims").join(&name));
    }
    if let Some(root) = environment("LOCALAPPDATA") {
        let root = PathBuf::from(root);
        candidates.push(
            root.join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join(&name),
        );
        if is_ffmpeg_tool {
            candidates.push(root.join("Programs").join("ffmpeg").join("bin").join(&name));
        }
    }
    if let Some(root) = environment("ProgramData") {
        candidates.push(
            PathBuf::from(root)
                .join("chocolatey")
                .join("bin")
                .join(&name),
        );
    }
    if is_ffmpeg_tool {
        if let Some(root) = environment("ProgramFiles") {
            candidates.push(PathBuf::from(root).join("ffmpeg").join("bin").join(&name));
        }
        if let Some(root) = environment("SystemDrive") {
            let root = PathBuf::from(root);
            let root = if root.has_root() {
                root
            } else {
                PathBuf::from(format!("{}\\", root.display()))
            };
            candidates.push(root.join("ffmpeg").join("bin").join(&name));
        }
    }
    candidates
}

#[cfg(not(target_os = "windows"))]
fn common_tool_candidates(_executable: &str) -> Vec<PathBuf> {
    Vec::new()
}

fn version_output(path: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new(path);
    command.args(args);
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output().ok()?;
    if !output.status.success() && output.stdout.is_empty() && output.stderr.is_empty() {
        return None;
    }
    let value = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    Some(String::from_utf8_lossy(value).trim().to_string())
}

fn atomic_replace(source: &Path, target: &Path) -> Result<(), AppError> {
    if target.exists() {
        let backup = target.with_extension("exe.previous");
        let _ = fs::remove_file(&backup);
        fs::rename(target, &backup).map_err(filesystem_error)?;
        match fs::rename(source, target) {
            Ok(()) => {
                let _ = fs::remove_file(backup);
                Ok(())
            }
            Err(error) => {
                let _ = fs::rename(backup, target);
                Err(filesystem_error(error))
            }
        }
    } else {
        fs::rename(source, target).map_err(filesystem_error)
    }
}

#[cfg(target_os = "windows")]
fn free_disk_bytes(path: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_available: *mut u64,
            total: *mut u64,
            free_total: *mut u64,
        ) -> i32;
    }
    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut available = 0u64;
    let result = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (result != 0).then_some(available)
}

#[cfg(not(target_os = "windows"))]
fn free_disk_bytes(_path: &Path) -> Option<u64> {
    None
}

fn network_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "tool_download_failed",
        ErrorCategory::Network,
        "LectureScribe could not download the required tool.",
        error.to_string(),
    )
    .retryable("The existing tool, if any, was left unchanged.")
}

fn filesystem_error(error: impl std::fmt::Display) -> AppError {
    AppError::new(
        "tool_file_failed",
        ErrorCategory::Filesystem,
        "LectureScribe could not update the required tool.",
        error.to_string(),
    )
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_paths(root: &Path) -> AppPaths {
        let data_dir = root.join("data");
        AppPaths {
            tools_dir: data_dir.join("tools"),
            cache_dir: data_dir.join("cache"),
            logs_dir: data_dir.join("logs"),
            database_path: data_dir.join("lecturescribe.sqlite3"),
            data_dir,
            install_dir: root.join("app"),
        }
    }

    #[test]
    fn app_local_and_bundled_candidates_precede_path_lookup() {
        let paths = test_paths(Path::new(r"C:\LectureScribeTest"));
        let resolver = ToolResolver::new(paths.clone());

        let ffmpeg = resolver.named_candidates("ffmpeg", "");
        assert_eq!(ffmpeg[0], paths.tools_dir.join("ffmpeg.exe"));
        assert_eq!(ffmpeg[1], paths.install_dir.join("ffmpeg.exe"));
        assert_eq!(ffmpeg.last(), Some(&PathBuf::from("ffmpeg.exe")));

        let downloader = resolver.downloader_candidates(&AppSettings::default());
        assert_eq!(downloader[0], paths.tools_dir.join("yt-dlp.exe"));
        assert!(downloader.contains(&paths.install_dir.join("yt-dlp.exe")));
        assert!(downloader.contains(&paths.install_dir.join("resources").join("yt-dlp.exe")));
        assert_eq!(downloader.last(), Some(&PathBuf::from("yt-dlp.exe")));
    }

    #[test]
    fn configured_tool_path_has_highest_priority() {
        let resolver = ToolResolver::new(test_paths(Path::new(r"C:\LectureScribeTest")));
        let configured = PathBuf::from(r"D:\MediaTools\ffprobe.exe");
        let candidates = resolver.named_candidates("ffprobe", &configured.to_string_lossy());
        assert_eq!(candidates.first(), Some(&configured));
    }

    #[test]
    fn bare_candidate_resolves_from_path_without_changing_explicit_paths() {
        let root =
            std::env::temp_dir().join(format!("lecturescribe-tools-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).expect("create temporary tool directory");
        let executable = root.join("ffmpeg.exe");
        fs::write(&executable, b"test").expect("create temporary tool");

        assert_eq!(
            locate_path_candidate_in(Path::new("ffmpeg.exe"), std::slice::from_ref(&root)),
            executable
        );
        let explicit = PathBuf::from(r"C:\Tools\ffmpeg.exe");
        assert_eq!(
            locate_path_candidate_in(&explicit, std::slice::from_ref(&root)),
            explicit
        );

        fs::remove_dir_all(root).expect("remove temporary tool directory");
    }

    #[test]
    fn windows_candidates_cover_package_managers_and_common_ffmpeg_locations() {
        let environment = HashMap::from([
            ("ChocolateyInstall", OsString::from(r"C:\Chocolatey")),
            ("SCOOP", OsString::from(r"C:\Scoop")),
            ("USERPROFILE", OsString::from(r"C:\Users\Test")),
            (
                "LOCALAPPDATA",
                OsString::from(r"C:\Users\Test\AppData\Local"),
            ),
            ("ProgramData", OsString::from(r"C:\ProgramData")),
            ("ProgramFiles", OsString::from(r"C:\Program Files")),
            ("SystemDrive", OsString::from(r"C:")),
        ]);
        let candidates =
            common_tool_candidates_with("ffmpeg", |name| environment.get(name).cloned());

        for expected in [
            r"C:\Chocolatey\bin\ffmpeg.exe",
            r"C:\Scoop\shims\ffmpeg.exe",
            r"C:\Users\Test\scoop\shims\ffmpeg.exe",
            r"C:\Users\Test\AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe",
            r"C:\Users\Test\AppData\Local\Programs\ffmpeg\bin\ffmpeg.exe",
            r"C:\ProgramData\chocolatey\bin\ffmpeg.exe",
            r"C:\Program Files\ffmpeg\bin\ffmpeg.exe",
            r"C:\ffmpeg\bin\ffmpeg.exe",
        ] {
            assert!(
                candidates.contains(&PathBuf::from(expected)),
                "missing {expected}"
            );
        }
    }

    #[test]
    fn downloader_candidates_use_managers_without_fake_ffmpeg_folders() {
        let environment = HashMap::from([
            (
                "LOCALAPPDATA",
                OsString::from(r"C:\Users\Test\AppData\Local"),
            ),
            ("ProgramFiles", OsString::from(r"C:\Program Files")),
            ("SystemDrive", OsString::from(r"C:")),
        ]);
        let candidates =
            common_tool_candidates_with("yt-dlp", |name| environment.get(name).cloned());

        assert!(candidates.contains(&PathBuf::from(
            r"C:\Users\Test\AppData\Local\Microsoft\WinGet\Links\yt-dlp.exe"
        )));
        assert!(!candidates.iter().any(|path| {
            path.to_string_lossy()
                .to_ascii_lowercase()
                .contains("ffmpeg")
        }));
    }

    #[test]
    fn windows_candidate_identity_is_case_insensitive() {
        assert_eq!(
            candidate_identity(Path::new(r"C:\Tools\FFmpeg.exe")),
            candidate_identity(Path::new(r"c:\tools\ffmpeg.exe"))
        );
    }
}
