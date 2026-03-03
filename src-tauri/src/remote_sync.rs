mod git_client;

use crate::models::{RemoteGitConfig, RemoteRulesUploadResult};
use chrono::{DateTime, Utc};
use git_client::{GitClient, RealGitClient};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

const REMOTE_RULES_FILE_PATH: &str = "groups-rules-backup.json";

struct TempRepoGuard {
    path: PathBuf,
}

impl TempRepoGuard {
    fn new(path: PathBuf) -> Result<Self, String> {
        if path.exists() {
            std::fs::remove_dir_all(&path)
                .map_err(|e| format!("cleanup previous temp repo failed: {e}"))?;
        }
        std::fs::create_dir_all(&path).map_err(|e| format!("create temp repo failed: {e}"))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRepoGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn authenticated_repo_url(repo_url: &str, token: &str) -> Result<String, String> {
    let mut url =
        Url::parse(repo_url).map_err(|_| "remoteGit.repoUrl must be a valid URL".to_string())?;
    if url.scheme() != "https" && url.scheme() != "http" {
        return Err("remoteGit.repoUrl must use http or https".to_string());
    }
    if token.trim().is_empty() {
        return Err("remoteGit.token cannot be empty".to_string());
    }

    let username = if url.host_str() == Some("github.com") {
        "x-access-token"
    } else {
        "oauth2"
    };
    url.set_username(username)
        .map_err(|_| "failed to set auth username".to_string())?;
    url.set_password(Some(token))
        .map_err(|_| "failed to set auth token".to_string())?;
    Ok(url.to_string())
}

fn require_remote_ready(remote: &RemoteGitConfig) -> Result<(), String> {
    if remote.repo_url.trim().is_empty() || remote.token.trim().is_empty() {
        return Err("Please configure remote repository info first".to_string());
    }
    if remote.branch.trim().is_empty() {
        return Err("remoteGit.branch cannot be empty".to_string());
    }
    Ok(())
}

fn init_repo(client: &impl GitClient, tmp_repo: &Path, auth_repo_url: &str) -> Result<(), String> {
    client.run_output(tmp_repo, &["init", "-q"], "git init failed")?;
    client.run_output(
        tmp_repo,
        &["config", "user.name", "AI Open Router"],
        "git config user.name failed",
    )?;
    client.run_output(
        tmp_repo,
        &["config", "user.email", "aor@local"],
        "git config user.email failed",
    )?;
    client.run_output(
        tmp_repo,
        &["remote", "add", "origin", auth_repo_url],
        "git remote add failed",
    )?;
    Ok(())
}

fn checkout_branch(
    client: &impl GitClient,
    tmp_repo: &Path,
    branch: &str,
    create_if_missing: bool,
) -> Result<bool, String> {
    let fetch_args = ["fetch", "--depth", "1", "origin", branch];
    let fetched = client
        .run_output(tmp_repo, &fetch_args, "git fetch failed")
        .is_ok();

    if fetched {
        client.run_output(
            tmp_repo,
            &["checkout", "-B", branch, "FETCH_HEAD"],
            "git checkout branch failed",
        )?;
        return Ok(true);
    }

    if !create_if_missing {
        return Ok(false);
    }

    client.run_output(
        tmp_repo,
        &["checkout", "--orphan", branch],
        "git checkout orphan branch failed",
    )?;
    let _ = client.run_output(tmp_repo, &["reset", "--hard"], "git reset temp repo failed");
    Ok(false)
}

fn write_remote_rules_file(tmp_repo: &Path, json_text: &str) -> Result<PathBuf, String> {
    let output_file = tmp_repo.join(REMOTE_RULES_FILE_PATH);
    if let Some(parent) = output_file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create remote file parent failed: {e}"))?;
    }
    std::fs::write(&output_file, json_text)
        .map_err(|e| format!("write remote rules file failed: {e}"))?;
    Ok(output_file)
}

fn pull_groups_json_from_remote_with_client(
    client: &impl GitClient,
    app_data_dir: &Path,
    remote: &RemoteGitConfig,
) -> Result<String, String> {
    require_remote_ready(remote)?;
    let auth_repo_url = authenticated_repo_url(&remote.repo_url, &remote.token)?;
    let tmp_repo = TempRepoGuard::new(app_data_dir.join("remote-sync-tmp"))?;

    init_repo(client, tmp_repo.path(), &auth_repo_url)?;
    let found = checkout_branch(client, tmp_repo.path(), remote.branch.trim(), false)?;
    if !found {
        return Err(format!("Remote branch not found: {}", remote.branch.trim()));
    }

    let content = std::fs::read_to_string(tmp_repo.path().join(REMOTE_RULES_FILE_PATH))
        .map_err(|e| format!("read remote rules file failed: {e}"))?;
    Ok(content)
}

fn upload_groups_json_to_remote_with_client(
    client: &impl GitClient,
    app_data_dir: &Path,
    remote: &RemoteGitConfig,
    json_text: &str,
    group_count: usize,
    local_updated_at: Option<String>,
    force: bool,
) -> Result<RemoteRulesUploadResult, String> {
    require_remote_ready(remote)?;
    let branch = remote.branch.trim().to_string();
    let auth_repo_url = authenticated_repo_url(&remote.repo_url, &remote.token)?;
    let tmp_repo = TempRepoGuard::new(app_data_dir.join("remote-sync-tmp"))?;

    init_repo(client, tmp_repo.path(), &auth_repo_url)?;
    let found_branch = checkout_branch(client, tmp_repo.path(), &branch, true)?;

    let remote_updated_at = if found_branch {
        read_remote_exported_at(tmp_repo.path()).ok().flatten()
    } else {
        None
    };

    if !force && is_local_older(local_updated_at.as_deref(), remote_updated_at.as_deref()) {
        return Ok(RemoteRulesUploadResult {
            ok: true,
            changed: false,
            branch,
            file_path: REMOTE_RULES_FILE_PATH.to_string(),
            group_count,
            needs_confirmation: true,
            warning: Some("remote_newer_than_local".to_string()),
            local_updated_at,
            remote_updated_at,
        });
    }

    let _ = write_remote_rules_file(tmp_repo.path(), json_text)?;

    client.run_output(
        tmp_repo.path(),
        &["add", REMOTE_RULES_FILE_PATH],
        "git add remote rules file failed",
    )?;
    let diff_status_code = client.run_status_code(
        tmp_repo.path(),
        &["diff", "--cached", "--quiet"],
        "git diff check failed",
    )?;
    let changed = match diff_status_code {
        Some(0) => false,
        Some(1) => true,
        _ => return Err("git diff check failed with unexpected exit status".to_string()),
    };

    if changed {
        client.run_output(
            tmp_repo.path(),
            &[
                "commit",
                "-m",
                "chore: sync groups and rules from AI Open Router",
            ],
            "git commit failed",
        )?;
        client.run_output(
            tmp_repo.path(),
            &["push", "-u", "origin", &branch],
            "git push failed",
        )?;
    }

    Ok(RemoteRulesUploadResult {
        ok: true,
        changed,
        branch,
        file_path: REMOTE_RULES_FILE_PATH.to_string(),
        group_count,
        needs_confirmation: false,
        warning: None,
        local_updated_at,
        remote_updated_at,
    })
}

pub fn pull_groups_json_from_remote(
    app_data_dir: &Path,
    remote: &RemoteGitConfig,
) -> Result<String, String> {
    pull_groups_json_from_remote_with_client(&RealGitClient, app_data_dir, remote)
}

pub fn upload_groups_json_to_remote(
    app_data_dir: &Path,
    remote: &RemoteGitConfig,
    json_text: &str,
    group_count: usize,
    local_updated_at: Option<String>,
    force: bool,
) -> Result<RemoteRulesUploadResult, String> {
    upload_groups_json_to_remote_with_client(
        &RealGitClient,
        app_data_dir,
        remote,
        json_text,
        group_count,
        local_updated_at,
        force,
    )
}

pub fn remote_rules_file_path() -> &'static str {
    REMOTE_RULES_FILE_PATH
}

pub fn has_remote_git_binary() -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.status().map(|status| status.success()).unwrap_or(false)
}

pub fn read_remote_exported_at(repo_root: &Path) -> Result<Option<String>, String> {
    let file = repo_root.join(REMOTE_RULES_FILE_PATH);
    if !file.exists() {
        return Ok(None);
    }
    let raw =
        std::fs::read_to_string(file).map_err(|e| format!("read remote rules file failed: {e}"))?;
    let parsed = serde_json::from_str::<Value>(&raw)
        .map_err(|e| format!("parse remote rules file failed: {e}"))?;
    Ok(parsed
        .get("exportedAt")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string()))
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn is_local_older(local: Option<&str>, remote: Option<&str>) -> bool {
    match (local.and_then(parse_ts), remote.and_then(parse_ts)) {
        (Some(local_dt), Some(remote_dt)) => local_dt < remote_dt,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RemoteGitConfig;
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    enum FakeResponse {
        Output(Result<String, String>),
        Status(Result<Option<i32>, String>),
    }

    struct FakeStep {
        expected_args: Vec<String>,
        response: FakeResponse,
    }

    #[derive(Clone, Default)]
    struct FakeGitClient {
        steps: Arc<Mutex<VecDeque<FakeStep>>>,
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl FakeGitClient {
        fn push_output(&self, args: &[&str], result: Result<&str, &str>) {
            let step = FakeStep {
                expected_args: args.iter().map(|v| v.to_string()).collect(),
                response: FakeResponse::Output(
                    result.map(|v| v.to_string()).map_err(|e| e.to_string()),
                ),
            };
            self.steps.lock().expect("steps lock").push_back(step);
        }

        fn push_status(&self, args: &[&str], result: Result<Option<i32>, &str>) {
            let step = FakeStep {
                expected_args: args.iter().map(|v| v.to_string()).collect(),
                response: FakeResponse::Status(result.map_err(|e| e.to_string())),
            };
            self.steps.lock().expect("steps lock").push_back(step);
        }

        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().expect("calls lock").clone()
        }
    }

    impl GitClient for FakeGitClient {
        fn run_output(&self, _cwd: &Path, args: &[&str], context: &str) -> Result<String, String> {
            let args_vec = args.iter().map(|v| v.to_string()).collect::<Vec<_>>();
            self.calls
                .lock()
                .expect("calls lock")
                .push(args_vec.clone());
            let step = self
                .steps
                .lock()
                .expect("steps lock")
                .pop_front()
                .ok_or_else(|| format!("{context}: no scripted fake response"))?;
            if step.expected_args != args_vec {
                return Err(format!(
                    "{context}: fake expected {:?}, got {:?}",
                    step.expected_args, args_vec
                ));
            }
            match step.response {
                FakeResponse::Output(result) => result.map_err(|err| format!("{context}: {err}")),
                FakeResponse::Status(_) => Err(format!("{context}: fake step kind mismatch")),
            }
        }

        fn run_status_code(
            &self,
            _cwd: &Path,
            args: &[&str],
            context: &str,
        ) -> Result<Option<i32>, String> {
            let args_vec = args.iter().map(|v| v.to_string()).collect::<Vec<_>>();
            self.calls
                .lock()
                .expect("calls lock")
                .push(args_vec.clone());
            let step = self
                .steps
                .lock()
                .expect("steps lock")
                .pop_front()
                .ok_or_else(|| format!("{context}: no scripted fake response"))?;
            if step.expected_args != args_vec {
                return Err(format!(
                    "{context}: fake expected {:?}, got {:?}",
                    step.expected_args, args_vec
                ));
            }
            match step.response {
                FakeResponse::Status(result) => result.map_err(|err| format!("{context}: {err}")),
                FakeResponse::Output(_) => Err(format!("{context}: fake step kind mismatch")),
            }
        }
    }

    fn sample_remote() -> RemoteGitConfig {
        RemoteGitConfig {
            enabled: true,
            repo_url: "https://github.com/demo/repo.git".to_string(),
            token: "tok".to_string(),
            branch: "main".to_string(),
        }
    }

    fn unique_temp_root(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aor-remote-sync-{tag}-{}-{nanos}",
            std::process::id()
        ))
    }

    #[test]
    fn pull_returns_branch_not_found_when_fetch_fails() {
        let fake = FakeGitClient::default();
        fake.push_output(&["init", "-q"], Ok(""));
        fake.push_output(&["config", "user.name", "AI Open Router"], Ok(""));
        fake.push_output(&["config", "user.email", "aor@local"], Ok(""));
        fake.push_output(
            &[
                "remote",
                "add",
                "origin",
                "https://x-access-token:tok@github.com/demo/repo.git",
            ],
            Ok(""),
        );
        fake.push_output(
            &["fetch", "--depth", "1", "origin", "main"],
            Err("not found"),
        );

        let root = unique_temp_root("pull-missing");
        let err = pull_groups_json_from_remote_with_client(&fake, &root, &sample_remote())
            .expect_err("must fail when branch missing");

        assert!(err.contains("Remote branch not found: main"));
    }

    #[test]
    fn upload_returns_changed_false_when_no_diff() {
        let fake = FakeGitClient::default();
        fake.push_output(&["init", "-q"], Ok(""));
        fake.push_output(&["config", "user.name", "AI Open Router"], Ok(""));
        fake.push_output(&["config", "user.email", "aor@local"], Ok(""));
        fake.push_output(
            &[
                "remote",
                "add",
                "origin",
                "https://x-access-token:tok@github.com/demo/repo.git",
            ],
            Ok(""),
        );
        fake.push_output(&["fetch", "--depth", "1", "origin", "main"], Ok(""));
        fake.push_output(&["checkout", "-B", "main", "FETCH_HEAD"], Ok(""));
        fake.push_output(&["add", REMOTE_RULES_FILE_PATH], Ok(""));
        fake.push_status(&["diff", "--cached", "--quiet"], Ok(Some(0)));

        let root = unique_temp_root("upload-no-change");
        let result = upload_groups_json_to_remote_with_client(
            &fake,
            &root,
            &sample_remote(),
            "{\"groups\":[]}",
            0,
            None,
            true,
        )
        .expect("upload should succeed");

        assert!(!result.changed);
        let calls = fake.calls();
        assert!(!calls
            .iter()
            .any(|args| args.first().map(|v| v.as_str()) == Some("commit")));
        assert!(!calls
            .iter()
            .any(|args| args.first().map(|v| v.as_str()) == Some("push")));
    }

    #[test]
    fn upload_commits_and_pushes_when_diff_exists() {
        let fake = FakeGitClient::default();
        fake.push_output(&["init", "-q"], Ok(""));
        fake.push_output(&["config", "user.name", "AI Open Router"], Ok(""));
        fake.push_output(&["config", "user.email", "aor@local"], Ok(""));
        fake.push_output(
            &[
                "remote",
                "add",
                "origin",
                "https://x-access-token:tok@github.com/demo/repo.git",
            ],
            Ok(""),
        );
        fake.push_output(&["fetch", "--depth", "1", "origin", "main"], Ok(""));
        fake.push_output(&["checkout", "-B", "main", "FETCH_HEAD"], Ok(""));
        fake.push_output(&["add", REMOTE_RULES_FILE_PATH], Ok(""));
        fake.push_status(&["diff", "--cached", "--quiet"], Ok(Some(1)));
        fake.push_output(
            &[
                "commit",
                "-m",
                "chore: sync groups and rules from AI Open Router",
            ],
            Ok("[main abc] commit"),
        );
        fake.push_output(&["push", "-u", "origin", "main"], Ok("ok"));

        let root = unique_temp_root("upload-changed");
        let result = upload_groups_json_to_remote_with_client(
            &fake,
            &root,
            &sample_remote(),
            "{\"groups\":[1]}",
            1,
            None,
            true,
        )
        .expect("upload should succeed");

        assert!(result.changed);
        let calls = fake.calls();
        assert!(calls
            .iter()
            .any(|args| args.first().map(|v| v.as_str()) == Some("commit")));
        assert!(calls
            .iter()
            .any(|args| args.first().map(|v| v.as_str()) == Some("push")));
    }

    #[test]
    fn upload_surfaces_git_context_and_stderr() {
        let fake = FakeGitClient::default();
        fake.push_output(&["init", "-q"], Ok(""));
        fake.push_output(&["config", "user.name", "AI Open Router"], Ok(""));
        fake.push_output(&["config", "user.email", "aor@local"], Ok(""));
        fake.push_output(
            &[
                "remote",
                "add",
                "origin",
                "https://x-access-token:tok@github.com/demo/repo.git",
            ],
            Ok(""),
        );
        fake.push_output(&["fetch", "--depth", "1", "origin", "main"], Ok(""));
        fake.push_output(&["checkout", "-B", "main", "FETCH_HEAD"], Ok(""));
        fake.push_output(&["add", REMOTE_RULES_FILE_PATH], Err("permission denied"));

        let root = unique_temp_root("upload-err");
        let err = upload_groups_json_to_remote_with_client(
            &fake,
            &root,
            &sample_remote(),
            "{\"groups\":[]}",
            0,
            None,
            true,
        )
        .expect_err("upload should fail");

        assert!(err.contains("git add remote rules file failed"));
        assert!(err.contains("permission denied"));
    }
}
