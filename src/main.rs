use std::env;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command;

mod config;
mod ssh;

const TTY_CMDS: &[&str] = &["clone", "fetch", "pull", "push", "ls-remote"];

fn needs_tty() -> bool {
    env::args()
        .skip(1)
        .any(|arg| TTY_CMDS.contains(&arg.as_str()))
}

/// Run `git rev-parse --show-toplevel` locally and return the basename,
/// which is used as the repository name on the remote.
fn detect_repo_name() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let top = std::str::from_utf8(&output.stdout).ok()?.trim();
    PathBuf::from(top)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
}

fn main() {
    let config = match config::Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sshgit: {}", e);
            std::process::exit(1);
        }
    };

    let git_args: Vec<String> = env::args().skip(1).collect();

    // Map local repo name to its counterpart under the remote root directory
    let remote_path = match detect_repo_name() {
        Some(name) => format!("{}/{}", config.root.trim_end_matches('/'), name),
        None => config.root.clone(),
    };

    if let Err(e) = ssh::ensure_master(&config) {
        eprintln!("sshgit: failed to establish SSH connection: {}", e);
        std::process::exit(1);
    }

    let have_tty = std::io::stdin().is_terminal();
    let mut cmd = ssh::build_ssh_command(&config, &remote_path, &git_args, needs_tty() && have_tty);

    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("sshgit: {}", e);
            std::process::exit(1);
        }
    };

    if let Some(code) = status.code() {
        std::process::exit(code);
    }
}
