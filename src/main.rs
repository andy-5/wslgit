use std::env;
use std::ffi::c_void;
use std::fmt;

use std::fs::OpenOptions;
use std::io::{self, IsTerminal, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::LazyLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[macro_use]
extern crate lazy_static;
extern crate regex;
use regex::bytes::Regex;
use regex::Regex as StrRegex;

mod fork;
mod wsl;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const BASH_EXECUTABLE: &str = "/bin/bash";
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const LOG_MUTEX_NAME: &str = r"Local\wslgit-diagnostic-log-v1";
const WAIT_OBJECT_0: u32 = 0;
const WAIT_ABANDONED: u32 = 0x80;

#[link(name = "kernel32")]
extern "system" {
    #[link_name = "CreateMutexW"]
    fn create_mutex_w(
        mutex_attributes: *mut c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> *mut c_void;
    #[link_name = "WaitForSingleObject"]
    fn wait_for_single_object(handle: *mut c_void, milliseconds: u32) -> u32;
    #[link_name = "ReleaseMutex"]
    fn release_mutex(handle: *mut c_void) -> i32;
    #[link_name = "CloseHandle"]
    fn close_handle(handle: *mut c_void) -> i32;
}

// https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags
const CREATE_NO_WINDOW: u32 = 0x08000000;

static mut DOUBLE_DASH_FOUND: bool = false;

#[derive(Clone, Copy)]
enum LogTarget {
    WindowsGit,
    Wsl,
}

#[derive(Clone, Copy)]
enum LogOperation {
    Run,
    Start,
    Wait,
}

impl fmt::Display for LogOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let value = match self {
            LogOperation::Run => "run",
            LogOperation::Start => "start",
            LogOperation::Wait => "wait",
        };
        write!(formatter, "{}", value)
    }
}

impl fmt::Display for LogTarget {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let value = match self {
            LogTarget::WindowsGit => "windows_git",
            LogTarget::Wsl => "wsl",
        };
        write!(formatter, "{}", value)
    }
}

enum LogEvent {
    Arguments {
        input_count: usize,
        forwarded_count: usize,
    },
    Complete {
        target: LogTarget,
        exit_status: Option<i32>,
        elapsed_ms: u128,
    },
    Dispatch {
        target: LogTarget,
    },
    NoDistribution,
    PathTranslation,
    ProcessError {
        target: LogTarget,
        operation: LogOperation,
        error_kind: io::ErrorKind,
        elapsed_ms: u128,
    },
    Start,
}

impl fmt::Display for LogEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LogEvent::Arguments {
                input_count,
                forwarded_count,
            } => write!(
                formatter,
                "event=arguments input_count={} forwarded_count={}",
                input_count, forwarded_count
            ),
            LogEvent::Complete {
                target,
                exit_status,
                elapsed_ms,
            } => {
                let exit_status = exit_status
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unavailable".to_string());
                write!(
                    formatter,
                    "event=complete target={} exit_status={} elapsed_ms={}",
                    target, exit_status, elapsed_ms
                )
            }
            LogEvent::Dispatch { target } => {
                write!(formatter, "event=dispatch target={}", target)
            }
            LogEvent::NoDistribution => write!(formatter, "event=no_distribution"),
            LogEvent::PathTranslation => write!(formatter, "event=path_translation"),
            LogEvent::ProcessError {
                target,
                operation,
                error_kind,
                elapsed_ms,
            } => write!(
                formatter,
                "event=process_error target={} operation={} error_kind={:?} elapsed_ms={}",
                target, operation, error_kind, elapsed_ms
            ),
            LogEvent::Start => write!(formatter, "event=start version={}", VERSION),
        }
    }
}

fn translate_path_to_unix(argument: String) -> String {
    let argument = argument.as_bytes();

    // An absolute or UNC path must:
    // 1. Be at the beginning of the string, or after a whitespace, colon, equal-sign or file://.
    // 2. Begin with <drive-letter>:\, <drive-letter>:/, \\ or //.
    // 3. Consist of 0 or more path components that does not contain the characters <>:|?'"\/ or newline,
    //    and are delimited by \ or /.
    lazy_static! {
        static ref ABS_WINPATH_RE: Regex = Regex::new(
            r#"(?-u)(?P<pre>^|[[:space:]]|:|=)(?P<path>([A-Za-z]:[\\/]|\\\\|//)([^<>:|?'"\\/\n]+[\\/]?)*)"#
        )
        .expect("Failed to compile ABS_WINPATH_RE regex.");
    }

    lazy_static! {
        static ref FILE_ABS_WINPATH_RE: Regex = Regex::new(
            r#"(?-u)(?P<pre>^file://)(?P<path>([A-Za-z]:[\\/]|\\\\|//)([^<>:|?'"\\/\n]+[\\/]?)*)"#
        )
        .expect("Failed to compile FILE_ABS_WINPATH_RE regex.");
    }

    lazy_static! {
        static ref TRANSPORT_PROTOCOL_RE: Regex =
            Regex::new(r#"(?-u)^(ssh|git|https?|ftps?|file)://"#)
                .expect("Failed to compile TRANSPORT_PROTOCOL_RE regex.");
    }

    let has_file_prefix = argument.starts_with(b"file://");
    let has_transport_protocol_prefix = TRANSPORT_PROTOCOL_RE.is_match(argument);

    let argument = if !has_transport_protocol_prefix {
        ABS_WINPATH_RE
            .replace_all(argument, &b"${pre}$(wslpath '${path}')"[..])
            .into_owned()
    } else if has_file_prefix {
        FILE_ABS_WINPATH_RE
            .replace_all(argument, &b"${pre}$(wslpath '${path}')"[..])
            .into_owned()
    } else {
        argument.to_vec()
    };

    // Relative paths that needs to have their slashes changed must:
    // 1. Be at the beginning of the string, or after a whitespace, colon, or equal-sign.
    // 2. Begin with a string of valid characters (except \)...
    // 3. Followed by one \
    // 4. And then any number of valid characters (including \).
    lazy_static! {
        static ref REL_WINPATH_RE: Regex = Regex::new(
            r#"(?-u)^(?P<before>[^\\]+([[:space:]]|:|=))?(?P<path>([^<>:|?'"\n\\]+)\\([^<>:|?'"\n]*))(?P<after>.*)"#
        )
        .expect("Failed to compile REL_WINPATH_RE regex.");
    }

    if REL_WINPATH_RE.is_match(&argument) {
        let caps = REL_WINPATH_RE.captures(&argument).unwrap();
        let path_cap = caps.name("path").unwrap();
        let path = std::str::from_utf8(&path_cap.as_bytes()).unwrap();

        let double_dash_found = unsafe { DOUBLE_DASH_FOUND };

        // If the path in the argument exists then it is definitely a relative path,
        // or if the argument is after double-dashes then it is very likely a relative path.
        let translate_relative_path =
            has_file_prefix || double_dash_found || Path::new(path).exists();

        if translate_relative_path {
            let wsl_path = path.replace("\\", "/");

            let before = match caps.name("before") {
                Some(s) => std::str::from_utf8(&s.as_bytes()).unwrap(),
                None => "",
            };
            let after = match caps.name("after") {
                Some(s) => std::str::from_utf8(&s.as_bytes()).unwrap(),
                None => "",
            };

            return format!("{}{}{}", before, wsl_path, after);
        }
    }

    std::str::from_utf8(&argument).unwrap().to_string()
}

fn translate_path_to_win(line: &[u8]) -> Vec<u8> {
    // Windows can handle both / and \ as path separator so there is no need to convert relative paths.

    // An absolute Unix path must:
    // 1. Be at the beginning of the string or after a whitespace.
    // 2. Begin with /
    // 3. Not contain the characters: <>:|?'* or newline.
    // Note that when an absolute path is found then the rest of the line is passed to wslpath as argument!
    // The only exception to this is lines ending in ` (fetch)` or ` (push)`, as in the output of `git remote -v`.
    lazy_static! {
        static ref WSLPATH_RE: Regex =
            Regex::new(r"(?m)(?P<pre>^|[[:space:]])(?P<path>/([^<>:|?'*\n]*/?)*)")
                .expect("Failed to compile WSLPATH_RE regex");
    }

    if WSLPATH_RE.is_match(line) {
        // Use wslpath to convert the path to a windows path.
        let line = WSLPATH_RE
            .replace_all(line, &b"${pre}$(wslpath -w '${path}')"[..])
            .into_owned();

        // Fixup output of `git remote -v`, i.e. lines ending in
        // ` (fetch)` or ` (push)` - move remote types outside the wslpath call.
        lazy_static! {
            static ref REMOTE_FIX_RE: Regex =
                Regex::new(r"(?m)\s(?P<type>(\(fetch\))|(\(push\)))'\)")
                    .expect("Failed to compile REMOTE_FIX_RE regex");
        }
        let line = REMOTE_FIX_RE
            .replace_all(&line, &b"') ${type}"[..])
            .into_owned();
        let line = std::str::from_utf8(&line).unwrap();

        let echo_cmd = format!("echo -n \"{}\"", line);
        let output = Command::new("wsl")
            .arg("-e")
            .arg(BASH_EXECUTABLE)
            .arg("-c")
            .arg(&echo_cmd)
            .output()
            .expect("failed to execute echo_cmd");
        if enable_logging() {
            log(LogEvent::PathTranslation);
        }
        return output.stdout;
    }
    line.to_vec()
}

fn escape_dollar_but_not_env_cmds(haystack: &str) -> String {
    static DOLLAR_AND_FOLLOWING_RE: LazyLock<StrRegex> = LazyLock::new(|| {
        StrRegex::new(r"\$([\(\)\w\\]*)").expect("Failed to compile DOLLAR_AND_FOLLOWING_RE regex")
    });
    let mut escaped = String::with_capacity(haystack.len());
    let mut last_match = 0;
    for m in DOLLAR_AND_FOLLOWING_RE.find_iter(haystack) {
        escaped.push_str(&haystack[last_match..m.start()]);
        let m_str = m.as_str();
        if m_str.starts_with("$\\")
            || m_str.starts_with("$(wslpath")
            || m_str.starts_with("$(env")
            || m_str.starts_with("$(printenv")
        {
            escaped.push_str(m_str);
        } else {
            escaped.push_str(&m_str.replace("$", "\\$"));
        }
        last_match = m.end();
    }
    escaped.push_str(&haystack[last_match..]);
    escaped
}

fn escape_characters(arg: String) -> String {
    let escaped = escape_dollar_but_not_env_cmds(&arg);
    escaped
        .replace("\n", "$'\n'")
        .replace("\"", "\\\"")
        .replace("<", "\\<")
        .replace(">", "\\>")
        .replace("!", "\\!")
}

fn invalid_characters(ch: char) -> bool {
    match ch {
        ' ' | '(' | ')' | '|' => true,
        _ => false,
    }
}

fn quote_argument(arg: String) -> String {
    if arg.contains(invalid_characters) || arg.is_empty() {
        return format!("\"{}\"", arg);
    } else {
        return arg;
    }
}

fn format_argument(arg: String) -> String {
    if arg == "--" {
        unsafe {
            DOUBLE_DASH_FOUND = true;
        };
        return arg;
    } else {
        let mut arg = arg;
        if fork::needs_patching() {
            arg = fork::patch_argument(arg);
        }
        arg = translate_path_to_unix(arg);
        arg = escape_characters(arg);
        arg = quote_argument(arg);
        arg
    }
}

/// Return `true` if the git command can access remotes and therefore might need
/// the setup of an interactive shell.
fn git_command_needs_interactive_shell() -> bool {
    const CMDS: &[&str] = &["clone", "fetch", "pull", "push", "ls-remote"];
    env::args()
        .skip(1)
        .position(|arg| CMDS.iter().position(|&tcmd| tcmd == arg).is_some())
        .is_some()
}

fn use_interactive_shell() -> bool {
    // check for explicit environment variable setting
    if let Ok(interactive_flag) = env::var("WSLGIT_USE_INTERACTIVE_SHELL") {
        if interactive_flag == "false" || interactive_flag == "0" {
            return false;
        } else if interactive_flag == "smart" {
            return git_command_needs_interactive_shell();
        } else {
            return true;
        }
    }
    // check for advanced usage indicated by BASH_ENV and WSLENV contains BASH_ENV
    else if env::var("BASH_ENV").is_ok() {
        if let Ok(wslenv) = env::var("WSLENV") {
            lazy_static! {
                // BASH_ENV can be first or after another variable.
                // It can be followed by flags, another variable or be last.
                static ref BASH_ENV_RE: Regex = Regex::new(r"(?-u)(^|:)BASH_ENV(/|:|$)")
                    .expect("Failed to compile BASH_ENV regex");
            }
            if BASH_ENV_RE.is_match(wslenv.as_bytes()) {
                return false;
            }
        }
    }
    // default
    git_command_needs_interactive_shell()
}

/// Find the working directory by starting from the current directory and applying
/// any paths from `-C` or `--work-tree` arguments.
///
/// `--git-dir` is ignored, it is assumed that both the work-tree and git-dir
/// are on the same file system/same wsl distribution.
///
/// Returns the working directory as a String.
fn get_working_directory(current_dir: PathBuf, args: &[String]) -> String {
    let mut working_dir = current_dir;
    let mut work_tree = env::var("GIT_WORK_TREE").unwrap_or("".to_string());

    let mut skip_next = false;
    let mut next_is_path = false;

    for arg in args {
        if skip_next {
            skip_next = false;
        } else if next_is_path {
            next_is_path = false;
            // let path = PathBuf::from(arg);
            working_dir.push(arg);
        } else if arg == "-c" {
            // `-c` expects a second argument, so skip next argument.
            skip_next = true;
        } else if arg == "-C" {
            // `-C` expects a second argument that is a path
            next_is_path = true;
        } else if arg.starts_with("--work-tree=") {
            work_tree = arg[12..].to_string();
        } else if !arg.starts_with("-") {
            // First argument that doesn't start with '-' (or '--') is the git
            // command; clone, commit etc.
            break;
        }
    }

    // Finally apply the path from any "--work-tree" argument on the current working dir
    if work_tree.len() > 0 {
        working_dir.push(work_tree);
    }

    return working_dir.to_str().unwrap().to_string();
}

/// Try to find the WSL distribution name from the provided `path`.
///
/// An UNC prefix consists of \\server\share\, which is then followed by a path.
/// When accessing a WSL filesystem using the `\\wsl$\dist\` UNC prefix then the
/// distribution name can be extracted from the second component of the UNC
/// prefix.
///
/// Returns the distribution name or None.
fn get_wsl_dist_name(path: &str) -> Option<String> {
    const UNC_SERVER_WSL: &str = "\\\\wsl$\\";
    const UNC_SERVER_WSL_LOCALHOST: &str = "\\\\wsl.localhost\\";

    let unc_path_without_server: Option<&str> = if path.starts_with(UNC_SERVER_WSL) {
        Some(&path[UNC_SERVER_WSL.len()..])
    } else if path.starts_with(UNC_SERVER_WSL_LOCALHOST) {
        Some(&path[UNC_SERVER_WSL_LOCALHOST.len()..])
    } else {
        None
    };

    let wsl_dist_name = match unc_path_without_server {
        Some(p) => {
            // the string p starts with the UNC 'share', which is the wsl dist name
            let (dist_name, _) = p.split_once('\\').unwrap();
            Some(dist_name.to_string())
        }
        None => {
            if let Ok(default_dist) = env::var("WSLGIT_DEFAULT_DIST") {
                Some(default_dist)
            } else {
                // Use wsl default dist
                None
            }
        }
    };

    return wsl_dist_name;
}

fn enable_logging() -> bool {
    if let Ok(enable_log_flag) = env::var("WSLGIT_ENABLE_LOGGING") {
        if enable_log_flag == "true" || enable_log_flag == "1" {
            return true;
        }
    }
    false
}

fn log_arguments(out_args: &[String]) {
    log(LogEvent::Arguments {
        input_count: env::args().skip(1).count(),
        forwarded_count: out_args.len(),
    });
}

fn write_log_record<F>(open_log: F, message: &str)
where
    F: FnOnce() -> io::Result<Box<dyn Write>>,
{
    if let Ok(mut log_file) = open_log() {
        let mut record = Vec::with_capacity(message.len().saturating_add(1));
        record.extend_from_slice(message.as_bytes());
        record.push(b'\n');
        let _ = log_file.write_all(&record);
    }
}

struct NamedMutexGuard {
    handle: *mut c_void,
}

impl Drop for NamedMutexGuard {
    fn drop(&mut self) {
        unsafe {
            release_mutex(self.handle);
            close_handle(self.handle);
        }
    }
}

fn try_acquire_named_mutex(name: &str) -> Option<NamedMutexGuard> {
    let wide_name = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe { create_mutex_w(std::ptr::null_mut(), 0, wide_name.as_ptr()) };
    if handle.is_null() {
        return None;
    }

    let wait_status = unsafe { wait_for_single_object(handle, 0) };
    if wait_status == WAIT_OBJECT_0 || wait_status == WAIT_ABANDONED {
        Some(NamedMutexGuard { handle })
    } else {
        unsafe {
            close_handle(handle);
        }
        None
    }
}

fn write_bounded_log(logfile: &Path, message: &str, max_bytes: u64) {
    let record_bytes = message.as_bytes().len().saturating_add(1) as u64;
    if record_bytes > max_bytes {
        return;
    }

    let mut rotated_path = logfile.as_os_str().to_os_string();
    rotated_path.push(".1");
    let rotated_path = PathBuf::from(rotated_path);

    match std::fs::metadata(logfile) {
        Ok(metadata) if metadata.len() > max_bytes => {
            match std::fs::remove_file(&rotated_path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return,
            }
            if std::fs::remove_file(logfile).is_err() {
                return;
            }
        }
        Ok(metadata) if metadata.len().saturating_add(record_bytes) > max_bytes => {
            match std::fs::remove_file(&rotated_path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => return,
            }
            if std::fs::rename(logfile, rotated_path).is_err() {
                return;
            }
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(_) => return,
    }

    write_log_record(
        || {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(logfile)
                .map(|file| Box::new(file) as Box<dyn Write>)
        },
        message,
    );
}

fn log(event: LogEvent) {
    let logfile = match env::current_exe()
        .ok()
        .and_then(|exe_path| exe_path.parent().map(|parent| parent.join("wslgit.log")))
    {
        Some(logfile) => logfile,
        None => return,
    };

    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let record = format!(
        "timestamp_ms={} pid={} {}",
        timestamp_ms,
        std::process::id(),
        event
    );

    let _log_guard = match try_acquire_named_mutex(LOG_MUTEX_NAME) {
        Some(guard) => guard,
        None => return,
    };
    write_bounded_log(&logfile, &record, MAX_LOG_BYTES);
}

fn log_completion(target: LogTarget, status: &ExitStatus, started_at: &Instant) {
    if enable_logging() {
        log(LogEvent::Complete {
            target,
            exit_status: status.code(),
            elapsed_ms: started_at.elapsed().as_millis(),
        });
    }
}

fn log_process_error(
    target: LogTarget,
    operation: LogOperation,
    error: &io::Error,
    started_at: &Instant,
) {
    if enable_logging() {
        log(LogEvent::ProcessError {
            target,
            operation,
            error_kind: error.kind(),
            elapsed_ms: started_at.elapsed().as_millis(),
        });
    }
}

fn main() {
    let started_at = Instant::now();

    if enable_logging() {
        log(LogEvent::Start);
    }

    let have_terminal = {
        let stdin = io::stdin();
        stdin.is_terminal()
    };

    let mut cmd_args = Vec::new();
    let mut git_args: Vec<String> = vec![String::from("git")];
    git_args.extend(env::args().skip(1).map(format_argument));

    let git_cmd: String = git_args.join(" ");

    let curr_dir = env::current_dir().unwrap();
    // Assumes that the first element in args is the executable
    let args: Vec<String> = env::args().skip(1).collect();
    let working_directory = get_working_directory(curr_dir, &args);
    match get_wsl_dist_name(&working_directory) {
        Some(wsl_dist) => {
            cmd_args.push("--distribution".to_string());
            cmd_args.push(wsl_dist.to_string());
        }
        None => {
            if enable_logging() {
                log(LogEvent::NoDistribution);
            }
            if let Ok(windows_git) = env::var("WSLGIT_WINDOWS_GIT") {
                let status;
                let mut windows_git_proc_setup = Command::new(windows_git.clone());

                windows_git_proc_setup.args(&args);
                if !have_terminal {
                    windows_git_proc_setup.creation_flags(CREATE_NO_WINDOW);
                }

                if enable_logging() {
                    log(LogEvent::Dispatch {
                        target: LogTarget::WindowsGit,
                    });
                    log_arguments(&args);
                }

                status = windows_git_proc_setup.status().unwrap_or_else(|error| {
                    log_process_error(
                        LogTarget::WindowsGit,
                        LogOperation::Run,
                        &error,
                        &started_at,
                    );
                    panic!(
                        "Failed to execute command '{}' {:?}: {}",
                        &windows_git, args, error
                    )
                });

                log_completion(LogTarget::WindowsGit, &status, &started_at);

                // forward any exit code
                if let Some(exit_code) = status.code() {
                    std::process::exit(exit_code);
                }
            }
        }
    }

    // build the command arguments that are passed to wsl.exe
    cmd_args.push("-e".to_string());
    cmd_args.push(BASH_EXECUTABLE.to_string());
    if use_interactive_shell() {
        cmd_args.push("-ic".to_string());
    } else {
        cmd_args.push("-c".to_string());
    }
    cmd_args.push(git_cmd.clone());

    if enable_logging() {
        log(LogEvent::Dispatch {
            target: LogTarget::Wsl,
        });
        log_arguments(&cmd_args);
    }

    wsl::share_val("WSLGIT", "1", false);

    // setup the git subprocess launched inside WSL
    let mut git_proc_setup = Command::new("wsl");
    git_proc_setup.args(&cmd_args);
    if !have_terminal {
        git_proc_setup.creation_flags(CREATE_NO_WINDOW);
    }

    let status;

    // add git commands that must use translate_path_to_win
    const TRANSLATED_CMDS: &[&str] = &["rev-parse", "remote", "init"];

    let translate_output = env::args()
        .skip(1)
        .position(|arg| {
            TRANSLATED_CMDS
                .iter()
                .position(|&tcmd| tcmd == arg)
                .is_some()
        })
        .is_some();

    if translate_output {
        // run the subprocess and capture its output
        let git_proc = git_proc_setup
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| {
                log_process_error(LogTarget::Wsl, LogOperation::Start, &error, &started_at);
                panic!("Failed to execute command '{}': {}", &git_cmd, error)
            });
        let output = git_proc.wait_with_output().unwrap_or_else(|error| {
            log_process_error(LogTarget::Wsl, LogOperation::Wait, &error, &started_at);
            panic!("Failed to wait for git call '{}': {}", &git_cmd, error)
        });
        status = output.status;
        let output_bytes = output.stdout;
        let mut stdout = io::stdout();
        stdout
            .write_all(&translate_path_to_win(&output_bytes))
            .expect("Failed to write git output");
        stdout.flush().expect("Failed to flush output");
    } else {
        // run the subprocess without capturing its output
        // the output of the subprocess is passed through unchanged
        status = git_proc_setup.status().unwrap_or_else(|error| {
            log_process_error(LogTarget::Wsl, LogOperation::Run, &error, &started_at);
            panic!("Failed to execute command '{}': {}", &git_cmd, error)
        });
    }

    log_completion(LogTarget::Wsl, &status, &started_at);

    // forward any exit code
    if let Some(exit_code) = status.code() {
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_open_failures_are_ignored() {
        write_log_record(
            || Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
            "event=test",
        );
    }

    #[test]
    fn log_write_failures_are_ignored() {
        struct FailingWriter;

        impl Write for FailingWriter {
            fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::WriteZero, "write failed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        write_log_record(|| Ok(Box::new(FailingWriter)), "event=test");
    }

    #[test]
    fn log_record_is_written_with_one_write_call() {
        use std::sync::{Arc, Mutex};

        struct RecordingWriter {
            writes: Arc<Mutex<Vec<Vec<u8>>>>,
        }

        impl Write for RecordingWriter {
            fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
                self.writes.lock().unwrap().push(buffer.to_vec());
                Ok(buffer.len())
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let writes = Arc::new(Mutex::new(Vec::new()));
        let writer_writes = Arc::clone(&writes);
        write_log_record(
            move || {
                Ok(Box::new(RecordingWriter {
                    writes: writer_writes,
                }))
            },
            "event=test",
        );

        assert_eq!(*writes.lock().unwrap(), vec![b"event=test\n".to_vec()]);
    }

    #[test]
    fn log_mutex_is_nonblocking_between_threads() {
        let mutex_name = format!(
            r"Local\wslgit-log-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let guard = try_acquire_named_mutex(&mutex_name).expect("failed to acquire test mutex");

        let contended_name = mutex_name.clone();
        let contended =
            std::thread::spawn(move || try_acquire_named_mutex(&contended_name).is_none())
                .join()
                .unwrap();
        assert!(contended);

        drop(guard);
        let acquired = std::thread::spawn(move || try_acquire_named_mutex(&mutex_name).is_some())
            .join()
            .unwrap();
        assert!(acquired);
    }

    #[test]
    fn process_error_event_contains_only_controlled_metadata() {
        assert_eq!(
            LogEvent::ProcessError {
                target: LogTarget::Wsl,
                operation: LogOperation::Start,
                error_kind: io::ErrorKind::NotFound,
                elapsed_ms: 12,
            }
            .to_string(),
            "event=process_error target=wsl operation=start error_kind=NotFound elapsed_ms=12"
        );
    }

    #[test]
    fn log_rotates_before_exceeding_the_size_limit() {
        let test_dir = env::temp_dir().join(format!(
            "wslgit-log-rotation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&test_dir).unwrap();
        let log_path = test_dir.join("wslgit.log");
        std::fs::write(&log_path, b"12345678").unwrap();

        write_bounded_log(&log_path, "abcd", 10);

        assert_eq!(std::fs::read(&log_path).unwrap(), b"abcd\n");
        assert_eq!(
            std::fs::read(test_dir.join("wslgit.log.1")).unwrap(),
            b"12345678"
        );
        assert!(std::fs::metadata(&log_path).unwrap().len() <= 10);
        std::fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn log_discards_an_existing_file_that_already_exceeds_the_limit() {
        let test_dir = env::temp_dir().join(format!(
            "wslgit-log-oversize-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&test_dir).unwrap();
        let log_path = test_dir.join("wslgit.log");
        std::fs::write(&log_path, b"123456789012").unwrap();

        write_bounded_log(&log_path, "abcd", 10);

        assert_eq!(std::fs::read(&log_path).unwrap(), b"abcd\n");
        let rotated_log_path = test_dir.join("wslgit.log.1");
        assert!(!rotated_log_path.exists());
        std::fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn use_interactive_shell_test() {
        // default
        env::remove_var("WSLGIT_USE_INTERACTIVE_SHELL");
        env::remove_var("BASH_ENV");
        env::remove_var("WSLENV");

        // It is not possible to change env::args, so the arguments that are matched
        // in git_command_needs_interactive_shell() are the arguments to cargo,
        // which does not match any of the git commands that needs interactive shell.
        let default_value = false;

        assert_eq!(use_interactive_shell(), default_value);

        // disable using WSLGIT_USE_INTERACTIVE_SHELL set to 'false' or '0'
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "false");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "0");
        assert_eq!(use_interactive_shell(), false);

        // enable using WSLGIT_USE_INTERACTIVE_SHELL set to anything but 'false' and '0'
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "true");
        assert_eq!(use_interactive_shell(), true);
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "1");
        assert_eq!(use_interactive_shell(), true);

        env::remove_var("WSLGIT_USE_INTERACTIVE_SHELL");

        // just having BASH_ENV is not enough
        env::set_var("BASH_ENV", "something");
        assert_eq!(use_interactive_shell(), default_value);

        // BASH_ENV must also be in WSLENV
        env::set_var("WSLENV", "BASH_ENV");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "BASH_ENV/up");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "BASH_ENV:TMP");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "BASH_ENV/up:TMP");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "TMP:BASH_ENV");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "TMP:BASH_ENV/up");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "TMP:BASH_ENV:TMP");
        assert_eq!(use_interactive_shell(), false);
        env::set_var("WSLENV", "TMP:BASH_ENV/up:TMP");
        assert_eq!(use_interactive_shell(), false);

        // WSLGIT_USE_INTERACTIVE_SHELL overrides BASH_ENV
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "true");
        assert_eq!(use_interactive_shell(), true);
        env::remove_var("WSLGIT_USE_INTERACTIVE_SHELL");

        env::set_var("WSLENV", "NOT_BASH_ENV/up");
        assert_eq!(use_interactive_shell(), default_value);
    }

    #[test]
    fn escape_characters() {
        assert_eq!(
            super::escape_characters("ab\ncdef".to_string()),
            "ab$\'\n\'cdef"
        );
        assert_eq!(
            super::escape_characters("ab\ncd ef".to_string()),
            "ab$\'\n\'cd ef"
        );
        assert_eq!(
            super::escape_characters("--pretty=format:%H±.%aN±.%aE±.%at±.%cN±.%cE±.%ct±.%P±.%B<!--RevisionMessageEnd-->".to_string()),
            "--pretty=format:%H±.%aN±.%aE±.%at±.%cN±.%cE±.%ct±.%P±.%B\\<\\!--RevisionMessageEnd--\\>"
        );
        // Long arguments with newlines...
        assert_eq!(
            super::escape_characters("--ab\ncdef".to_string()),
            "--ab$\'\n\'cdef"
        );
        assert_eq!(
            super::escape_characters("--ab\ncd ef".to_string()),
            "--ab$\'\n\'cd ef"
        );
        assert_eq!(
            super::escape_characters("ab\"cd ef\"".to_string()),
            "ab\\\"cd ef\\\""
        );
        // Test dollar sign escaping
        assert_eq!(super::escape_characters("$id.tsx".to_string()), "\\$id.tsx");
        assert_eq!(
            super::escape_characters("file$with$multiple$dollars.txt".to_string()),
            "file\\$with\\$multiple\\$dollars.txt"
        );
        // Test embedded path translation is not escaped
        assert_eq!(
            super::escape_characters(
                "commit --file $(wslpath '\\wsl.localhost\\test\\$commit.txt $".to_string()
            ),
            "commit --file $(wslpath '\\wsl.localhost\\test\\\\$commit.txt \\$"
        );
        // Test combination of dollar signs and newlines
        assert_eq!(
            super::escape_characters("$id\nvalue".to_string()),
            "\\$id$\'\n\'value"
        );
        assert_eq!(
            super::escape_characters("before$var\nafter$another".to_string()),
            "before\\$var$\'\n\'after\\$another"
        );
    }

    #[test]
    fn quote_argument_with_invalid_character() {
        assert_eq!(quote_argument("abc def".to_string()), "\"abc def\"");
        assert_eq!(quote_argument("abc(def".to_string()), "\"abc(def\"");
        assert_eq!(quote_argument("abc)def".to_string()), "\"abc)def\"");
        assert_eq!(quote_argument("abc|def".to_string()), "\"abc|def\"");
        assert_eq!(
            quote_argument("\\\"abc def\\\"".to_string()),
            "\"\\\"abc def\\\"\""
        );
        assert_eq!(
            quote_argument("user.(name|email)".to_string()),
            "\"user.(name|email)\""
        );
    }

    #[test]
    fn quote_long_argument_with_invalid_character() {
        assert_eq!(quote_argument("--abc def".to_string()), "\"--abc def\"");
        assert_eq!(quote_argument("--abc=def".to_string()), "--abc=def");
        assert_eq!(quote_argument("--abc=d ef".to_string()), "\"--abc=d ef\"");
        assert_eq!(quote_argument("--abc=d(ef".to_string()), "\"--abc=d(ef\"");
        assert_eq!(quote_argument("--abc=d)ef".to_string()), "\"--abc=d)ef\"");
        assert_eq!(quote_argument("--abc=d|ef".to_string()), "\"--abc=d|ef\"");
        assert_eq!(
            quote_argument("--pretty=format:a(b|c)d".to_string()),
            "\"--pretty=format:a(b|c)d\""
        );
        assert_eq!(
            quote_argument("--pretty=format:a (b | c) d".to_string()),
            "\"--pretty=format:a (b | c) d\""
        );
        // Long arguments with invalid characters in argument name
        assert_eq!(quote_argument("--abc(def".to_string()), "\"--abc(def\"");
        assert_eq!(quote_argument("--abc)def".to_string()), "\"--abc)def\"");
        assert_eq!(quote_argument("--abc|def".to_string()), "\"--abc|def\"");
    }

    #[test]
    fn quote_empty_argument() {
        assert_eq!(quote_argument("".to_string()), "\"\"");
    }

    #[test]
    fn win_to_unix_path_trans() {
        assert_eq!(
            translate_path_to_unix("D:\\test\\file.txt".to_string()),
            "$(wslpath 'D:\\test\\file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("D:/test/file.txt".to_string()),
            "$(wslpath 'D:/test/file.txt')"
        );
        assert_eq!(
            translate_path_to_unix(" D:\\test\\file.txt".to_string()),
            " $(wslpath 'D:\\test\\file.txt')"
        );
        assert_eq!(
            translate_path_to_unix(" D:/test/file.txt".to_string()),
            " $(wslpath 'D:/test/file.txt')"
        );
        assert_eq!(
            translate_path_to_unix(":main:D:\\test\\file.txt".to_string()),
            ":main:$(wslpath 'D:\\test\\file.txt')"
        );
        assert_eq!(
            translate_path_to_unix(":main:D:/test/file.txt".to_string()),
            ":main:$(wslpath 'D:/test/file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("1,1:D:\\test\\file.txt".to_string()),
            "1,1:$(wslpath 'D:\\test\\file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("1,1:D:/test/file.txt".to_string()),
            "1,1:$(wslpath 'D:/test/file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("C:\\Users\\test user\\my file.txt".to_string()),
            "$(wslpath 'C:\\Users\\test user\\my file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("C:/Users/test user/my file.txt".to_string()),
            "$(wslpath 'C:/Users/test user/my file.txt')"
        );
        assert_eq!(
            translate_path_to_unix("\\\\path\\to\\file.txt".to_string()),
            "$(wslpath '\\\\path\\to\\file.txt')"
        );
        // $ git commit --file="//wsl$/Ubuntu-20.04/home/"
        assert_eq!(
            translate_path_to_unix("\\\\wsl$\\Ubuntu-20.04\\home".to_string()),
            "$(wslpath '\\\\wsl$\\Ubuntu-20.04\\home')"
        );
        assert_eq!(
            translate_path_to_unix("//wsl$/Ubuntu-20.04/home".to_string()),
            "$(wslpath '//wsl$/Ubuntu-20.04/home')"
        );
    }

    #[test]
    fn unix_to_win_path_trans() {
        let check_wslpath = Command::new("wsl")
            .arg("-e")
            .arg(BASH_EXECUTABLE)
            .arg("-c")
            .arg("wslpath C:\\")
            .output();
        let prefix_bytes = translate_path_to_win(b"/");
        let prefix = std::str::from_utf8(&prefix_bytes).unwrap();
        if check_wslpath.is_err()
            || !check_wslpath.expect("bash output").status.success()
            || prefix == ""
        {
            // Skip test if `wslpath` is not available (e.g. in CI).
            // Either bash was not found, or running `wslpath` returned an error
            // code or an empty string.
            print!("SKIPPING TEST ... ");
            return;
        }
        // Since Windows 10 2004 `wslpath` can only translate existing
        // unix paths to windows paths, so we need to test real filenames.
        // (see https://github.com/microsoft/WSL/issues/4908)
        Command::new("wsl")
            .arg("-e")
            .arg(BASH_EXECUTABLE)
            .arg("-c")
            .arg("touch '/tmp/wslgit test file'")
            .output()
            .expect("creating tmp test file");
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(b"/tmp/wslgit test file")).unwrap(),
            format!("{}tmp\\wslgit test file", prefix)
        );
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(
                b"origin  /tmp/wslgit test file (fetch)"
            ))
            .unwrap(),
            format!("origin  {}tmp\\wslgit test file (fetch)", prefix)
        );
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(b"mirror  /tmp/wslgit test file (fetch)\nmirror  /tmp/wslgit test file (push)\n")).unwrap(),
            format!("mirror  {0}tmp\\wslgit test file (fetch)\nmirror  {0}tmp\\wslgit test file (push)\n", prefix)
        );
        Command::new("wsl")
            .arg("-e")
            .arg(BASH_EXECUTABLE)
            .arg("-c")
            .arg("rm '/tmp/wslgit test file'")
            .output()
            .expect("deleting tmp test file");
    }

    #[test]
    fn relative_path_translation() {
        unsafe {
            DOUBLE_DASH_FOUND = false;
        }

        assert_eq!(
            translate_path_to_unix("src\\main.rs".to_string()),
            "src/main.rs"
        );
        assert_eq!(
            translate_path_to_unix("src/main.rs".to_string()),
            "src/main.rs"
        );
        assert_eq!(
            translate_path_to_unix(".\\src\\main.rs".to_string()),
            "./src/main.rs"
        );
        assert_eq!(
            translate_path_to_unix("./src/main.rs".to_string()),
            "./src/main.rs"
        );
        assert_eq!(
            translate_path_to_unix("..\\wslgit\\src\\main.rs".to_string()),
            "../wslgit/src/main.rs"
        );
        assert_eq!(
            translate_path_to_unix("../wslgit/src/main.rs".to_string()),
            "../wslgit/src/main.rs"
        );

        assert_eq!(
            translate_path_to_unix("prefix:..\\wslgit\\src\\main.rs:postfix".to_string()),
            "prefix:../wslgit/src/main.rs:postfix"
        );

        assert_eq!(
            translate_path_to_unix("^remote\\..*".to_string()),
            "^remote\\..*"
        );

        assert_eq!(
            translate_path_to_unix("\"prefix:..\\wslgit\\src\\main.rs\"".to_string()),
            "\"prefix:../wslgit/src/main.rs\""
        );
    }

    #[test]
    fn relative_path_after_double_dash() {
        unsafe {
            DOUBLE_DASH_FOUND = false;
        }
        assert_eq!(format_argument("--".to_string()), "--");
        assert_eq!(unsafe { DOUBLE_DASH_FOUND }, true);

        unsafe {
            DOUBLE_DASH_FOUND = false;
        }
        assert_eq!(format_argument("-".to_string()), "-");
        assert_eq!(unsafe { DOUBLE_DASH_FOUND }, false);

        unsafe {
            DOUBLE_DASH_FOUND = false;
        }
        assert_eq!(
            format_argument("path\\to\\nonexisting\\file.txt".to_string()),
            "path\\to\\nonexisting\\file.txt"
        );

        unsafe {
            DOUBLE_DASH_FOUND = true;
        }
        assert_eq!(
            format_argument("path\\to\\nonexisting\\file.txt".to_string()),
            "path/to/nonexisting/file.txt"
        );
    }

    #[test]
    fn do_not_escape_dollar_in_pathname() {
        assert_eq!(
            format_argument(r##"\\wsl$\Debian\home"##.to_string()),
            r##""$(wslpath '\\wsl$\Debian\home')""##
        );
    }

    #[test]
    fn git_url_translation() {
        // URLs with ssh, git, http[s] or ftp[s] prefix should not be translated
        assert_eq!(
            translate_path_to_unix("ssh://user@host.xz:22/path/to/repo.git/".to_string()),
            "ssh://user@host.xz:22/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("ssh://user@host.xz/path/to/repo.git/".to_string()),
            "ssh://user@host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("ssh://host.xz/path/to/repo.git/".to_string()),
            "ssh://host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("user@host.xz/path/to/repo.git/".to_string()),
            "user@host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("host.xz/path/to/repo.git/".to_string()),
            "host.xz/path/to/repo.git/"
        );

        assert_eq!(
            translate_path_to_unix("git://host.xz/path/to/repo.git/".to_string()),
            "git://host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("http://host.xz/path/to/repo.git/".to_string()),
            "http://host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("https://host.xz/path/to/repo.git/".to_string()),
            "https://host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("ftp://host.xz/path/to/repo.git/".to_string()),
            "ftp://host.xz/path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("ftps://host.xz/path/to/repo.git/".to_string()),
            "ftps://host.xz/path/to/repo.git/"
        );

        assert_eq!(
            translate_path_to_unix("file:///path/to/repo.git/".to_string()),
            "file:///path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("file://C:/path/to/repo.git/".to_string()),
            "file://$(wslpath 'C:/path/to/repo.git/')"
        );
        assert_eq!(
            translate_path_to_unix("file://C:\\path\\to\\repo.git\\".to_string()),
            "file://$(wslpath 'C:\\path\\to\\repo.git\\')"
        );

        assert_eq!(
            translate_path_to_unix("file://path/to/repo.git/".to_string()),
            "file://path/to/repo.git/"
        );
        assert_eq!(
            translate_path_to_unix("file://path\\to\\repo.git\\".to_string()),
            "file://path/to/repo.git/"
        );
    }

    #[test]
    fn arguments_path_translation() {
        assert_eq!(
            translate_path_to_unix("--file=C:\\some\\path.txt".to_owned()),
            "--file=$(wslpath 'C:\\some\\path.txt')"
        );
        assert_eq!(
            translate_path_to_unix("--file=C:/some/path.txt".to_owned()),
            "--file=$(wslpath 'C:/some/path.txt')"
        );

        assert_eq!(
            translate_path_to_unix("-c core.editor=C:\\some\\editor.exe".to_owned()),
            "-c core.editor=$(wslpath 'C:\\some\\editor.exe')"
        );
        assert_eq!(
            translate_path_to_unix("-c core.editor=C:/some/editor.exe".to_owned()),
            "-c core.editor=$(wslpath 'C:/some/editor.exe')"
        );

        assert_eq!(
            translate_path_to_unix(
                "-c \"credential.helper=C:/Program Files/SmartGit/lib/credentials.cmd\"".to_owned()
            ),
            "-c \"credential.helper=$(wslpath 'C:/Program Files/SmartGit/lib/credentials.cmd')\""
        );
    }

    #[test]
    fn get_working_directory_test() {
        env::remove_var("GIT_WORK_TREE");

        let args: Vec<String> = vec![];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\".to_string()
        );
        assert_eq!(
            get_working_directory(PathBuf::from("\\\\wsl$\\dist-name\\repo\\"), &args),
            "\\\\wsl$\\dist-name\\repo\\".to_string()
        );

        let args: Vec<String> = vec!["cmd".into()];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\".to_string()
        );

        let args: Vec<String> = vec!["-c".into(), "arg".into(), "cmd".into()];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\".to_string()
        );

        let args: Vec<String> = vec![
            "-c".into(),
            "arg".into(),
            "-C".into(),
            "relative".into(),
            "cmd".into(),
        ];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\relative".to_string()
        );

        let args: Vec<String> = vec![
            "-c".into(),
            "arg".into(),
            "-C".into(),
            "C:\\absolute".into(),
            "cmd".into(),
        ];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\absolute".to_string()
        );

        let args: Vec<String> = vec![
            "-c".into(),
            "arg".into(),
            "-C".into(),
            "a".into(),
            "-C".into(),
            "b".into(),
            "cmd".into(),
        ];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\a\\b".to_string()
        );

        env::set_var("GIT_WORK_TREE", "b");
        let args: Vec<String> = vec![
            "-c".into(),
            "arg".into(),
            "-C".into(),
            "a".into(),
            "cmd".into(),
        ];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\a\\b".to_string()
        );

        env::set_var("GIT_WORK_TREE", "b");
        let args: Vec<String> = vec![
            "-c".into(),
            "arg".into(),
            "-C".into(),
            "a".into(),
            "--work-tree=c".into(),
            "cmd".into(),
        ];
        assert_eq!(
            get_working_directory(PathBuf::from("C:\\repo\\"), &args),
            "C:\\repo\\a\\c".to_string()
        );
    }

    #[test]
    fn wsl_dist_name() {
        env::remove_var("WSLGIT_DEFAULT_DIST");
        assert_eq!(
            get_wsl_dist_name(&r"\\wsl$\dist-name\a\b\c".to_string()),
            Some("dist-name".to_string())
        );
        assert_eq!(
            get_wsl_dist_name(&r"\\wsl.localhost\dist-name\a\b\c".to_string()),
            Some("dist-name".to_string())
        );
        assert_eq!(
            get_wsl_dist_name(&r"\\server\dist-name\a\b\c".to_string()),
            None
        );
        assert_eq!(get_wsl_dist_name(&r"C:\a\b\c".to_string()), None);
    }

    #[test]
    fn wsl_default_dist_name() {
        env::set_var("WSLGIT_DEFAULT_DIST", "some-dist");
        assert_eq!(
            get_wsl_dist_name(&r"\\wsl$\dist-name\a\b\c".to_string()),
            Some("dist-name".to_string())
        );
        assert_eq!(
            get_wsl_dist_name(&r"\\server\dist-name\a\b\c".to_string()),
            Some("some-dist".to_string())
        );
        assert_eq!(
            get_wsl_dist_name(&r"C:\a\b\c".to_string()),
            Some("some-dist".to_string())
        );
    }
}
