use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::Config;

/// Derive the socket path for a given config's host.
pub fn socket_path(config: &Config) -> PathBuf {
    let sanitized: String = config
        .host
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    config.socket_dir.join(format!("sshgit-{}.sock", sanitized))
}

/// Ensure the SSH multiplex master is running. Starts it if not.
pub fn ensure_master(config: &Config) -> Result<(), String> {
    let sock = socket_path(config);

    if let Some(parent) = sock.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create socket directory: {}", e))?;
    }

    // Check if master is already alive via the control socket
    let alive = Command::new("ssh")
        .arg("-o")
        .arg(format!("ControlPath={}", sock.display()))
        .arg("-O")
        .arg("check")
        .arg(&config.host)
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if alive {
        return Ok(());
    }

    // Start the master in the background
    let mut args: Vec<String> = vec![
        "-fN".to_string(),
        "-o".to_string(),
        "ControlMaster=yes".to_string(),
        "-o".to_string(),
        format!("ControlPath={}", sock.display()),
        "-o".to_string(),
        format!("ControlPersist={}", config.control_persist),
    ];
    if let Some(port) = config.port {
        args.extend(["-p".to_string(), port.to_string()]);
    }
    if let Some(ref id) = config.identity_file {
        args.extend(["-i".to_string(), id.clone()]);
    }
    args.push(config.host.clone());

    let status = Command::new("ssh")
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to launch SSH master: {}", e))?;

    if !status.success() {
        return Err(format!(
            "SSH master exited with code {:?}",
            status.code()
        ));
    }

    Ok(())
}

/// Build a Command that runs git on the remote host via the multiplex socket.
pub fn build_ssh_command(
    config: &Config,
    remote_path: &str,
    git_args: &[String],
    need_tty: bool,
) -> Command {
    let sock = socket_path(config);
    let mut args = mux_opts(config, &sock);

    args.push(if need_tty {
        "-t".to_string()
    } else {
        "-T".to_string()
    });
    args.push(config.host.clone());

    // Build the remote shell command string with each arg properly quoted
    let mut parts = vec![
        "git".to_string(),
        "-C".to_string(),
        shell_quote(remote_path),
    ];
    parts.extend(git_args.iter().map(|a| shell_quote(a)));
    args.push(parts.join(" "));

    let mut cmd = Command::new("ssh");
    cmd.args(args);
    cmd
}

fn mux_opts(config: &Config, sock: &Path) -> Vec<String> {
    let mut args = vec![
        "-o".to_string(),
        "ControlMaster=auto".to_string(),
        "-o".to_string(),
        format!("ControlPath={}", sock.display()),
    ];
    if let Some(port) = config.port {
        args.extend(["-p".to_string(), port.to_string()]);
    }
    if let Some(ref id) = config.identity_file {
        args.extend(["-i".to_string(), id.clone()]);
    }
    args
}

/// Single-quote a string for safe passage through the remote shell.
///
/// Simple path-safe strings are left bare. Everything else is single-quoted,
/// with embedded single quotes escaped as `'"'"'`.
///
/// Examples:
/// - `status`       → `status`
/// - `my message`   → `'my message'`
/// - `it's`         → `'it'"'"'s'`
/// - `$HOME`        → `'$HOME'`
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
        // it's  →  'it'"'"'s'
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
