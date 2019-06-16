use std::borrow::Cow;
use std::env;

use std::io::{self, Write};
use std::path::{Component, Path, Prefix, PrefixComponent};
use std::process::{Command, Stdio};

#[macro_use]
extern crate lazy_static;
extern crate regex;
use regex::bytes::Regex;

fn get_drive_letter(pc: &PrefixComponent) -> Option<String> {
    let drive_byte = match pc.kind() {
        Prefix::VerbatimDisk(d) => Some(d),
        Prefix::Disk(d) => Some(d),
        _ => None,
    };
    drive_byte.map(|drive_letter| {
        String::from_utf8(vec![drive_letter])
            .expect(&format!("Invalid drive letter: {}", drive_letter))
            .to_lowercase()
    })
}

fn mount_root() -> String {
    match env::var("WSLGIT_MOUNT_ROOT") {
        Ok(val) => {
            if val.ends_with("/") {
                return val;
            } else {
                return format!("{}/", val);
            }
        }
        Err(_e) => return "/mnt/".to_string(),
    }
}

fn get_prefix_for_drive(drive: &str) -> String {
    // todo - lookup mount points
    format!("/mnt/{}", drive)
}

fn translate_path_to_unix(argument: String) -> String {
    {
        let (argname, arg) = if argument.starts_with("--") && argument.contains('=') {
            let parts: Vec<&str> = argument.splitn(2, '=').collect();
            (format!("{}=", parts[0]), parts[1])
        } else {
            ("".to_owned(), argument.as_ref())
        };
        let win_path = Path::new(arg);
        if win_path.is_absolute() || win_path.exists() {
            let wsl_path: String = win_path.components().fold(String::new(), |mut acc, c| {
                match c {
                    Component::Prefix(prefix_comp) => {
                        let d = get_drive_letter(&prefix_comp)
                            .expect(&format!("Cannot handle path {:?}", win_path));
                        acc.push_str(&get_prefix_for_drive(&d));
                    }
                    Component::RootDir => {}
                    _ => {
                        let d = c
                            .as_os_str()
                            .to_str()
                            .expect(&format!("Cannot represent path {:?}", win_path))
                            .to_owned();
                        if !acc.is_empty() && !acc.ends_with('/') {
                            acc.push('/');
                        }
                        acc.push_str(&d);
                    }
                };
                acc
            });
            return format!("{}{}", &argname, &wsl_path);
        }
    }
    argument
}

fn translate_path_to_win(line: &[u8]) -> Cow<[u8]> {
    lazy_static! {
        static ref WSLPATH_RE: Regex = Regex::new(r"(?m-u)/mnt/(?P<drive>[A-Za-z])(?P<path>/\S*)")
            .expect("Failed to compile WSLPATH regex");
    }
    WSLPATH_RE.replace_all(line, &b"${drive}:${path}"[..])
}

fn escape_newline(arg: String) -> String {
    arg.replace("\n", "$'\n'")
}

fn quote_characters(ch: char) -> bool {
    match ch {
        '\"' | '\'' => true,
        _ => false,
    }
}

fn invalid_characters(ch: char) -> bool {
    match ch {
        ' ' | '(' | ')' | '|' => true,
        _ => false,
    }
}

fn format_argument(arg: String) -> String {
    if arg.contains(quote_characters) {
        // if argument contains quotes then assume it is correctly quoted.
        return arg;
    } else if arg.contains(invalid_characters) {
        return format!("\"{}\"", arg);
    } else {
        return arg;
    }
}

fn use_interactive_shell() -> bool {
    // check for explicit environment variable setting
    if let Ok(interactive_flag) = env::var("WSLGIT_USE_INTERACTIVE_SHELL") {
        if interactive_flag == "false" || interactive_flag == "0" {
            return false;
        }
    }
    // check for advanced usage indicated by BASH_ENV and WSLENV=BASH_ENV
    else if env::var("BASH_ENV").is_ok() {
        if let Ok(wslenv) = env::var("WSLENV") {
            if wslenv
                .split(':')
                .position(|r| r.eq_ignore_ascii_case("BASH_ENV"))
                .is_some()
            {
                return false;
            }
        }
    }
    true
}

fn main() {
    let mut cmd_args = Vec::new();
    let cwd_unix =
        translate_path_to_unix(env::current_dir().unwrap().to_string_lossy().into_owned());
    let mut git_args: Vec<String> = vec![
        String::from("cd"),
        format!("\"{}\"", cwd_unix),
        String::from("&&"),
        String::from("git"),
    ];

    git_args.extend(
        env::args()
            .skip(1)
            .map(translate_path_to_unix)
            .map(format_argument)
            .map(escape_newline),
    );

    let git_cmd: String = git_args.join(" ");

    // build the command arguments that are passed to wsl.exe
    cmd_args.push("bash".to_string());
    if use_interactive_shell() {
        cmd_args.push("-ic".to_string());
    } else {
        cmd_args.push("-c".to_string());
    }
    cmd_args.push(git_cmd.clone());

    // setup the git subprocess launched inside WSL
    let mut git_proc_setup = Command::new("wsl");
    git_proc_setup.args(&cmd_args);
    let status;

    // add git commands that must use translate_path_to_win
    const TRANSLATED_CMDS: &[&str] = &["rev-parse", "remote"];

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
    fn mount_root_test() {
        env::remove_var("WSLGIT_MOUNT_ROOT");
        assert_eq!(mount_root(), "/mnt/");

        env::set_var("WSLGIT_MOUNT_ROOT", "/abc/");
        assert_eq!(mount_root(), "/abc/");

        env::set_var("WSLGIT_MOUNT_ROOT", "/abc");
        assert_eq!(mount_root(), "/abc/");

        env::set_var("WSLGIT_MOUNT_ROOT", "/");
        assert_eq!(mount_root(), "/");
    }

    #[test]
    fn escape_newline() {
        assert_eq!(
            super::escape_newline("ab\ncdef".to_string()),
            "ab$\'\n\'cdef"
        );
        assert_eq!(
            super::escape_newline("ab\ncd ef".to_string()),
            "ab$\'\n\'cd ef"
        );
        // Long arguments with newlines...
        assert_eq!(
            super::escape_newline("--ab\ncdef".to_string()),
            "--ab$\'\n\'cdef"
        );
        assert_eq!(
            super::escape_newline("--ab\ncd ef".to_string()),
            "--ab$\'\n\'cd ef"
        );
    }

    #[test]
    fn format_argument_with_invalid_character() {
        assert_eq!(format_argument("abc def".to_string()), "\"abc def\"");
        assert_eq!(format_argument("abc(def".to_string()), "\"abc(def\"");
        assert_eq!(format_argument("abc)def".to_string()), "\"abc)def\"");
        assert_eq!(format_argument("abc|def".to_string()), "\"abc|def\"");
        assert_eq!(format_argument("\"abc def\"".to_string()), "\"abc def\"");
        assert_eq!(
            format_argument("user.(name|email)".to_string()),
            "\"user.(name|email)\""
        );
    }

    #[test]
    fn format_long_argument_with_invalid_character() {
        assert_eq!(format_argument("--abc def".to_string()), "\"--abc def\"");
        assert_eq!(format_argument("--abc=def".to_string()), "--abc=def");
        assert_eq!(format_argument("--abc=d ef".to_string()), "\"--abc=d ef\"");
        assert_eq!(format_argument("--abc=d(ef".to_string()), "\"--abc=d(ef\"");
        assert_eq!(format_argument("--abc=d)ef".to_string()), "\"--abc=d)ef\"");
        assert_eq!(format_argument("--abc=d|ef".to_string()), "\"--abc=d|ef\"");
        assert_eq!(
            format_argument("--pretty=format:a(b|c)d".to_string()),
            "\"--pretty=format:a(b|c)d\""
        );
        assert_eq!(
            format_argument("--pretty=format:a (b | c) d".to_string()),
            "\"--pretty=format:a (b | c) d\""
        );
        // Long arguments with invalid characters in argument name
        assert_eq!(format_argument("--abc(def".to_string()), "\"--abc(def\"");
        assert_eq!(format_argument("--abc)def".to_string()), "\"--abc)def\"");
        assert_eq!(format_argument("--abc|def".to_string()), "\"--abc|def\"");
    }

    #[test]
    fn win_to_unix_path_trans() {
        assert_eq!(
            translate_path_to_unix("d:\\test\\file.txt".to_string()),
            "/mnt/d/test/file.txt"
        );
        assert_eq!(
            translate_path_to_unix("C:\\Users\\test\\a space.txt".to_string()),
            "/mnt/c/Users/test/a space.txt"
        );
    }

    #[test]
    fn unix_to_win_path_trans() {
        assert_eq!(
            &*translate_path_to_win(b"/mnt/d/some path/a file.md"),
            b"d:/some path/a file.md"
        );
        assert_eq!(
            &*translate_path_to_win(b"origin  /mnt/c/path/ (fetch)"),
            b"origin  c:/path/ (fetch)"
        );
        let multiline = b"mirror  /mnt/c/other/ (fetch)\nmirror  /mnt/c/other/ (push)\n";
        let multiline_result = b"mirror  c:/other/ (fetch)\nmirror  c:/other/ (push)\n";
        assert_eq!(
            &*translate_path_to_win(&multiline[..]),
            &multiline_result[..]
        );
    }

    #[test]
    fn no_path_translation() {
        assert_eq!(
            &*translate_path_to_win(b"/mnt/other/file.sh"),
            b"/mnt/other/file.sh"
        );
    }

    #[test]
    fn relative_path_translation() {
        assert_eq!(
            translate_path_to_unix(".\\src\\main.rs".to_string()),
            "./src/main.rs"
        );
    }

    #[test]
    fn long_argument_path_translation() {
        assert_eq!(
            translate_path_to_unix("--file=C:\\some\\path.txt".to_owned()),
            "--file=/mnt/c/some/path.txt"
        );
    }
}
