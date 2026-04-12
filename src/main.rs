use std::env;
use std::io::IsTerminal;

mod config;
mod ssh;

const TTY_CMDS: &[&str] = &["clone", "fetch", "pull", "push", "ls-remote"];

/// Commands that GUI clients call to probe/validate a git installation.
/// These are answered locally so we never attempt an SSH connection during startup.
const LOCAL_CMDS: &[&str] = &["version", "--version", "--help", "-h"];

fn needs_tty() -> bool {
    env::args()
        .skip(1)
        .any(|arg| TTY_CMDS.contains(&arg.as_str()))
}

/// Return true if every argument is a local-only flag/command that should
/// never trigger an SSH connection (e.g. `git version`, `git --version`).
fn is_local_only(args: &[String]) -> bool {
    !args.is_empty() && args.iter().all(|a| LOCAL_CMDS.contains(&a.as_str()))
}

/// Walk up from the current directory to find the git repo root (the
/// directory containing `.git`), then return its basename.
/// This deliberately avoids calling `git` to prevent recursion back into
/// this binary when it is registered as the system git executable.
fn detect_repo_name() -> Option<String> {
    let mut dir = env::current_dir().ok()?;
    loop {
        if dir.join(".git").exists() {
            return dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }
        if !dir.pop() {
            return None;
        }
    }
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

    // Short-circuit commands that only probe the git installation.
    // We answer these locally with a static response — never SSH, and never
    // call `git` by name (which would recurse back into this binary).
    if is_local_only(&git_args) {
        println!("git version 2.47.0 (sshgit)");
        return;
    }

    // Map local repo name to its counterpart under the remote root directory
    let remote_path = match detect_repo_name() {
        Some(name) => format!("{}/{}", config.root.trim_end_matches('/'), name),
        None => config.root.clone(),
    };

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
