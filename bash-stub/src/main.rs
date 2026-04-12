use std::io::IsTerminal;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

const GIT_SEARCH_DIRS: &[&str] = &["usr/bin", "bin", "mingw64/bin"];

/// Find an executable by name inside the Git for Windows installation,
/// located by walking PATH for git.exe (no hardcoded install paths).
/// Falls back to Windows system directories for executables like ssh.exe
/// that aren't bundled with Git for Windows.
fn find_in_git(name: &str) -> Option<PathBuf> {
    // 1. Walk PATH to find git.exe, then resolve name relative to its root
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join("git.exe").exists() {
                for ancestor in dir.join("git.exe").ancestors().skip(1) {
                    for search_dir in GIT_SEARCH_DIRS {
                        let candidate = ancestor.join(search_dir).join(name);
                        if candidate.exists() {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }

    // 2. Windows system directories for things not bundled with Git for Windows
    let system_dirs = [
        std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string()),
    ];
    for root in &system_dirs {
        let candidate = Path::new(root).join("System32").join("OpenSSH").join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn main() {
    // Derive the target executable name from argv[0] so one binary
    // can act as bash.exe, sh.exe, ssh.exe, etc.
    let exe_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "bash.exe".to_string());

    let target = match find_in_git(&exe_name) {
        Some(p) => p,
        None => {
            eprintln!("git-stub: could not locate {} in Git for Windows", exe_name);
            std::process::exit(1);
        }
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let have_terminal = std::io::stdin().is_terminal();

    let mut cmd = Command::new(&target);
    cmd.args(&args);
    if !have_terminal {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let status = cmd.status().unwrap_or_else(|e| {
        eprintln!("git-stub: failed to launch {}: {}", target.display(), e);
        std::process::exit(1);
    });

    if let Some(code) = status.code() {
        std::process::exit(code);
    }
}
