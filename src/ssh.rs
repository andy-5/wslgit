use std::process::Command;

use crate::config::Config;

/// Find an SSH executable.
///
/// Strategy (in order):
/// 1. `GIT_SSH` env var — user-specified override
/// 2. `GIT_INSTALL_ROOT` env var pointing at a Git for Windows root
/// 3. Walk PATH: find git.exe, look for ssh.exe relative to its install root
/// 4. Fall back to `ssh` and let the OS resolve it
fn find_ssh() -> String {
    if let Ok(v) = std::env::var("GIT_SSH") {
        if std::path::Path::new(&v).exists() {
            return v;
        }
    }

    if let Ok(root) = std::env::var("GIT_INSTALL_ROOT") {
        let p = std::path::Path::new(&root).join("usr").join("bin").join("ssh.exe");
        if p.exists() {
            return p.to_string_lossy().into_owned();
        }
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            if dir.join("git.exe").exists() {
                for ancestor in dir.join("git.exe").ancestors().skip(1) {
                    let ssh = ancestor.join("usr").join("bin").join("ssh.exe");
                    if ssh.exists() {
                        return ssh.to_string_lossy().into_owned();
                    }
                }
            }
        }
    }

    "ssh".to_string()
}

/// Build a Command that runs git on the remote host over SSH.
pub fn build_ssh_command(
    config: &Config,
    remote_path: &str,
    git_args: &[String],
    need_tty: bool,
) -> Command {
    let ssh = find_ssh();

    let mut args: Vec<String> = vec![
        "-o".to_string(), "ConnectTimeout=10".to_string(),
    ];
    if let Some(port) = config.port {
        args.extend(["-p".to_string(), port.to_string()]);
    }
    if let Some(ref id) = config.identity_file {
        args.extend(["-i".to_string(), id.replace('\\', "/")]);
    }
    args.push(if need_tty { "-t".to_string() } else { "-T".to_string() });
    args.push(config.host.clone());

    let mut parts = vec![
        "git".to_string(),
        "-C".to_string(),
        shell_quote(remote_path),
    ];
    parts.extend(git_args.iter().map(|a| shell_quote(a)));
    args.push(parts.join(" "));

    let mut cmd = Command::new(ssh);
    cmd.args(args);
    cmd
}

/// Single-quote a string for safe passage through the remote shell.
pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let needs_quoting = s.chars().any(|c| {
        !c.is_alphanumeric()
            && !matches!(c, '-' | '_' | '.' | '/' | ',' | ':' | '@' | '+' | '=')
    });
    if needs_quoting {
        format!("'{}'", s.replace('\'', r#"'"'"'"#))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_plain_identifiers() {
        assert_eq!(shell_quote("status"), "status");
        assert_eq!(shell_quote("src/main.rs"), "src/main.rs");
        assert_eq!(shell_quote("refs/heads/main"), "refs/heads/main");
        assert_eq!(shell_quote("-n1"), "-n1");
        assert_eq!(shell_quote("--oneline"), "--oneline");
    }

    #[test]
    fn quote_empty_string() {
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn quote_string_with_spaces() {
        assert_eq!(shell_quote("my message"), "'my message'");
        assert_eq!(shell_quote("a ( b | c )"), "'a ( b | c )'");
    }

    #[test]
    fn quote_string_with_single_quote() {
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn quote_special_chars() {
        assert_eq!(
            shell_quote("--pretty=format:%(refname)%(objectname)"),
            "'--pretty=format:%(refname)%(objectname)'"
        );
        assert_eq!(shell_quote("a(b|c)"), "'a(b|c)'");
        assert_eq!(shell_quote("<!--end-->"), "'<!--end-->'");
    }

    #[test]
    fn quote_dollar_sign() {
        assert_eq!(shell_quote("$HOME"), "'$HOME'");
        assert_eq!(shell_quote("--pretty=format:$(env)"), "'--pretty=format:$(env)'");
    }

    #[test]
    fn quote_newline() {
        assert_eq!(shell_quote("ab\ncd"), "'ab\ncd'");
    }
}
