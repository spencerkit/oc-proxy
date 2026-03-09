//! Module Overview
//! Helpers for safely reading and writing WSL files via `wsl.exe`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SHORT_PREFIX: &str = "\\\\wsl$\\";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WslPath {
    pub(crate) distro: String,
    pub(crate) linux_path: String,
}

pub(crate) fn is_wsl_path(path: &Path) -> bool {
    normalize_windows_path(path).is_some()
}

pub(crate) fn normalize_windows_path(path: &Path) -> Option<PathBuf> {
    let path_str = path.to_string_lossy();
    let lower = path_str.to_lowercase();

    if lower.starts_with(SHORT_PREFIX) {
        let remainder = path_str[SHORT_PREFIX.len()..].replace('/', "\\");
        return Some(PathBuf::from(format!("{SHORT_PREFIX}{remainder}")));
    }

    for marker in ["wsl.localhost\\", "wsl.localhost/"] {
        if let Some(idx) = lower.find(marker) {
            let after = path_str[idx + marker.len()..]
                .trim_start_matches(&['\\', '/'][..])
                .replace('/', "\\");
            return Some(PathBuf::from(format!("{SHORT_PREFIX}{after}")));
        }
    }

    None
}

pub(crate) fn resolve_path(path: &Path) -> Option<WslPath> {
    let normalized = normalize_windows_path(path)?;
    let path_str = normalized.to_string_lossy();
    let remainder = path_str
        .strip_prefix(SHORT_PREFIX)?
        .trim_start_matches(&['\\', '/'][..]);

    let distro_end = remainder
        .find(|ch| ['\\', '/'].contains(&ch))
        .unwrap_or(remainder.len());
    let distro = remainder[..distro_end].trim();
    if distro.is_empty() {
        return None;
    }

    let tail = remainder[distro_end..]
        .trim_start_matches(&['\\', '/'][..])
        .replace('\\', "/");
    let linux_path = if tail.is_empty() {
        "/".to_string()
    } else {
        format!("/{tail}")
    };

    Some(WslPath {
        distro: distro.to_string(),
        linux_path,
    })
}

pub(crate) fn exists(path: &Path) -> Result<bool, String> {
    let wsl_path =
        resolve_path(path).ok_or_else(|| format!("not a WSL path: {}", path.display()))?;
    probe(
        &wsl_path,
        &["-e", &wsl_path.linux_path],
        "check path existence",
    )
}

pub(crate) fn is_dir(path: &Path) -> Result<bool, String> {
    let wsl_path =
        resolve_path(path).ok_or_else(|| format!("not a WSL path: {}", path.display()))?;
    probe(&wsl_path, &["-d", &wsl_path.linux_path], "check directory")
}

pub(crate) fn is_file(path: &Path) -> Result<bool, String> {
    let wsl_path =
        resolve_path(path).ok_or_else(|| format!("not a WSL path: {}", path.display()))?;
    probe(&wsl_path, &["-f", &wsl_path.linux_path], "check file")
}

pub(crate) fn read_file(path: &Path) -> Result<Option<String>, String> {
    let wsl_path =
        resolve_path(path).ok_or_else(|| format!("not a WSL path: {}", path.display()))?;
    if !probe(&wsl_path, &["-f", &wsl_path.linux_path], "check file")? {
        return Ok(None);
    }

    let output = run_wsl_command(
        &wsl_path,
        "cat",
        &["--", &wsl_path.linux_path],
        "read file",
        None,
    )?;
    if output.status.success() {
        return Ok(Some(String::from_utf8_lossy(&output.stdout).to_string()));
    }

    Err(format_command_failure(&wsl_path, "read file", &output))
}

pub(crate) fn write_file(path: &Path, content: &str) -> Result<(), String> {
    let wsl_path =
        resolve_path(path).ok_or_else(|| format!("not a WSL path: {}", path.display()))?;
    let parent = linux_parent_dir(&wsl_path.linux_path);
    let mkdir_output = run_wsl_command(
        &wsl_path,
        "mkdir",
        &["-p", "--", &parent],
        "create parent directory",
        None,
    )?;
    if !mkdir_output.status.success() {
        return Err(format_command_failure(
            &wsl_path,
            "create parent directory",
            &mkdir_output,
        ));
    }

    let output = run_wsl_command(
        &wsl_path,
        "tee",
        &["--", &wsl_path.linux_path],
        "write file",
        Some(content.as_bytes()),
    )?;

    if output.status.success() {
        return Ok(());
    }

    Err(format_command_failure(&wsl_path, "write file", &output))
}

fn probe(wsl_path: &WslPath, args: &[&str], action: &str) -> Result<bool, String> {
    let output = run_wsl_command(wsl_path, "test", args, action, None)?;

    if output.status.success() {
        return Ok(true);
    }
    if output.status.code() == Some(1) {
        return Ok(false);
    }

    Err(format_command_failure(&wsl_path, action, &output))
}

fn run_wsl_command(
    wsl_path: &WslPath,
    program: &str,
    args: &[&str],
    action: &str,
    stdin_bytes: Option<&[u8]>,
) -> Result<Output, String> {
    let mut command = Command::new("wsl");
    configure_background_command(&mut command);
    command
        .arg("-d")
        .arg(&wsl_path.distro)
        .arg("--")
        .arg(program)
        .args(args)
        .stdin(if stdin_bytes.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|e| {
        format!(
            "failed to {action} via WSL distro {} for {}: {e}",
            wsl_path.distro, wsl_path.linux_path
        )
    })?;

    if let Some(bytes) = stdin_bytes {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            format!(
                "failed to open stdin for WSL distro {} while {action} {}",
                wsl_path.distro, wsl_path.linux_path
            )
        })?;
        stdin.write_all(bytes).map_err(|e| {
            format!(
                "failed to send file content to WSL distro {} for {}: {e}",
                wsl_path.distro, wsl_path.linux_path
            )
        })?;
        drop(stdin);
    }

    child.wait_with_output().map_err(|e| {
        format!(
            "failed to wait for WSL distro {} while {action} {}: {e}",
            wsl_path.distro, wsl_path.linux_path
        )
    })
}

#[cfg(target_os = "windows")]
fn configure_background_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn configure_background_command(_command: &mut Command) {}

fn linux_parent_dir(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }

    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(index) => path[..index].to_string(),
    }
}

fn format_command_failure(wsl_path: &WslPath, action: &str, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let suffix = if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    };

    format!(
        "{action} failed for {} in WSL distro {} (exit {:?}){suffix}",
        wsl_path.linux_path,
        wsl_path.distro,
        output.status.code()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_extended_unc_format() {
        let input = PathBuf::from("\\\\?\\UNC\\wsl.localhost\\Ubuntu\\home\\spencer\\.codex");
        let normalized = normalize_windows_path(&input).expect("WSL path should normalize");
        assert_eq!(
            normalized,
            PathBuf::from("\\\\wsl$\\Ubuntu\\home\\spencer\\.codex")
        );
    }

    #[test]
    fn resolve_short_format_separates_distro_from_linux_path() {
        let input = PathBuf::from("\\\\wsl$\\Ubuntu\\home\\spencer\\.codex\\config.toml");
        let resolved = resolve_path(&input).expect("WSL path should resolve");
        assert_eq!(resolved.distro, "Ubuntu");
        assert_eq!(resolved.linux_path, "/home/spencer/.codex/config.toml");
    }

    #[test]
    fn resolve_path_handles_mixed_separators() {
        let input = PathBuf::from("\\\\wsl$\\Ubuntu\\home\\spencer\\.codex/config.toml");
        let resolved = resolve_path(&input).expect("WSL path should resolve");
        assert_eq!(resolved.distro, "Ubuntu");
        assert_eq!(resolved.linux_path, "/home/spencer/.codex/config.toml");
    }

    #[test]
    fn non_wsl_path_is_not_normalized() {
        let input = PathBuf::from("C:\\Users\\spencer\\.codex");
        assert!(normalize_windows_path(&input).is_none());
        assert!(!is_wsl_path(&input));
    }

    #[test]
    fn parent_dir_of_linux_path_preserves_root_and_nested_paths() {
        assert_eq!(linux_parent_dir("/"), "/");
        assert_eq!(linux_parent_dir("/config.toml"), "/");
        assert_eq!(
            linux_parent_dir("/home/spencer/.codex/config.toml"),
            "/home/spencer/.codex"
        );
    }
}
