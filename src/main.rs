use std::env;

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

#[macro_use]
extern crate lazy_static;
extern crate regex;
use regex::bytes::Regex;

mod fork;
mod wsl;

const VERSION: &'static str = env!("CARGO_PKG_VERSION");

static mut DOUBLE_DASH_FOUND: bool = false;

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
    lazy_static! {
        static ref WSLPATH_RE: Regex =
            Regex::new(r"(?m)(?P<pre>^|[[:space:]])(?P<path>/([^<>:|?'*\n]*/?)*)")
                .expect("Failed to compile WSLPATH_RE regex");
    }

    if WSLPATH_RE.is_match(line) {
        // Use wslpath to convert the path to a windows path.
        let line = WSLPATH_RE
            .replace_all(
                line,
                &b"${pre}$(wslpath -w '${path}')"[..],
            )
            .into_owned();
        let line = std::str::from_utf8(&line).unwrap();

        let echo_cmd = format!("echo -n \"{}\"", line);
        let output = Command::new("wsl")
            .arg("-e")
            .arg("bash")
            .arg("-c")
            .arg(&echo_cmd)
            .output()
            .expect("failed to execute echo_cmd");
        if enable_logging() {
            log(format!(
                "{:?} -> {} -> {:?}",
                line,
                echo_cmd,
                std::str::from_utf8(&output.stdout).unwrap()
            ));
        }
        return output.stdout;
    }
    line.to_vec()
}

fn escape_characters(arg: String) -> String {
    arg.replace("\n", "$'\n'").replace("\"", "\\\"")
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

fn enable_logging() -> bool {
    if let Ok(enable_log_flag) = env::var("WSLGIT_ENABLE_LOGGING") {
        if enable_log_flag == "true" || enable_log_flag == "1" {
            return true;
        }
    }
    false
}

fn log_arguments(out_args: &Vec<String>) {
    let in_args = env::args().collect::<Vec<String>>();
    log(format!("{:?} -> {:?}", in_args, out_args));
}

fn log(message: String) {
    let logfile = match env::current_exe() {
        Ok(exe_path) => exe_path
            .parent()
            .unwrap()
            .join("wslgit.log")
            .to_string_lossy()
            .into_owned(),
        Err(e) => {
            eprintln!("Failed to get current exe path: {}", e);
            Path::new("wslgit.log").to_string_lossy().into_owned()
        }
    };

    let f = OpenOptions::new()
        .append(true)
        .create(true)
        .open(logfile)
        .unwrap();
    write!(&f, "{}\n", message).unwrap();
}

fn main() {
    let mut cmd_args = Vec::new();
    let mut git_args: Vec<String> = vec![String::from("git")];

    git_args.extend(env::args().skip(1).map(format_argument));

    let git_cmd: String = git_args.join(" ");

    // build the command arguments that are passed to wsl.exe
    cmd_args.push("-e".to_string());
    cmd_args.push("bash".to_string());
    if use_interactive_shell() {
        cmd_args.push("-ic".to_string());
    } else {
        cmd_args.push("-c".to_string());
    }
    cmd_args.push(git_cmd.clone());

    if enable_logging() {
        log(format!("wslgit version {}", VERSION));
        log_arguments(&cmd_args);
    }

    wsl::share_val("WSLGIT", "1", false);

    // setup the git subprocess launched inside WSL
    let mut git_proc_setup = Command::new("wsl");
    git_proc_setup.args(&cmd_args);

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
            .expect(&format!("Failed to execute command '{}'", &git_cmd));
        let output = git_proc
            .wait_with_output()
            .expect(&format!("Failed to wait for git call '{}'", &git_cmd));
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
        status = git_proc_setup
            .status()
            .expect(&format!("Failed to execute command '{}'", &git_cmd));
    }

    // forward any exit code
    if let Some(exit_code) = status.code() {
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .arg("bash")
            .arg("-c")
            .arg("wslpath C:\\")
            .output();
        if check_wslpath.is_err() || !check_wslpath.expect("bash output").status.success() {
            // Skip test if `wslpath` is not available (e.g. in CI)
            // Either bash was not found, or running `wslpath` returned an error code
            print!("SKIPPING TEST ... ");
            return;
        }
        // Since Windows 10 2004 `wslpath` can only translate existing
        // unix paths to windows paths, so we need to test real filenames.
        // (see https://github.com/microsoft/WSL/issues/4908)
        Command::new("wsl")
            .arg("-e")
            .arg("bash")
            .arg("-c")
            .arg("touch '/tmp/wslgit test file'")
            .output()
            .expect("creating tmp test file");
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(b"/tmp/wslgit test file")).unwrap(),
            "\\\\wsl$\\Ubuntu-20.04\\tmp\\wslgit test file"
        );
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(
                b"origin  /tmp/wslgit test file (fetch)"
            ))
            .unwrap(),
            "origin  \\\\wsl$\\Ubuntu-20.04\\tmp\\wslgit test file (fetch)"
        );
        assert_eq!(
            std::str::from_utf8(&translate_path_to_win(b"mirror  /tmp/wslgit test file (fetch)\nmirror  /tmp/wslgit test file (push)\n")).unwrap(),
            "mirror  \\\\wsl$\\Ubuntu-20.04\\tmp\\wslgit test file (fetch)\nmirror  \\\\wsl$\\Ubuntu-20.04\\tmp\\wslgit test file (push)\n"
        );
        Command::new("wsl")
            .arg("-e")
            .arg("bash")
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
}
