use std::path::Path;
use std::process::Command;

pub trait GitClient {
    fn run_output(&self, cwd: &Path, args: &[&str], context: &str) -> Result<String, String>;
    fn run_status_code(
        &self,
        cwd: &Path,
        args: &[&str],
        context: &str,
    ) -> Result<Option<i32>, String>;
}

#[derive(Clone, Default)]
pub struct RealGitClient;

fn git_command(cwd: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(cwd).args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

impl GitClient for RealGitClient {
    fn run_output(&self, cwd: &Path, args: &[&str], context: &str) -> Result<String, String> {
        let output = git_command(cwd, args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| format!("{context}: {e}"))?;

        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("{context}: exit status {}", output.status));
        }
        Err(format!("{context}: {stderr}"))
    }

    fn run_status_code(
        &self,
        cwd: &Path,
        args: &[&str],
        context: &str,
    ) -> Result<Option<i32>, String> {
        git_command(cwd, args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()
            .map_err(|e| format!("{context}: {e}"))
            .map(|status| status.code())
    }
}
