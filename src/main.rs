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

fn invalid_character(ch: char) -> bool {
    match ch {
        '(' | ')' | '|' => true,
        _ => false,
    }
}

fn quotes(ch: char) -> bool {
    match ch {
        '\"' | '\'' => true,
        _ => false,
    }
}

fn escape_newline(arg: String) -> String {
    arg.replace("\n", "$'\n'")
}

fn format_argument_for_interactive_shell(arg: String) -> String {
    if arg.contains(quotes) {
        // if argument contains quotes then assume it is correctly quoted.
        return arg;
    } else if arg.contains(invalid_character) || arg.contains(" ") {
        return format!("'{}'", arg);
    } else {
        return arg;
    }
}

fn format_argument_for_non_interactive_shell(argument: String) -> String {
    let mut arg: String = argument;

    // argument can not contain quotes since they will then be inside the outer quoting
    if arg.contains(quotes) {
        arg = arg.replace(quotes, "");
    }
    // If an argument string contains a space character then it is correctly quoted
    // somewhere else, unknown where (when process is spawned? by std::process::Command? by wsl.exe?),
    // and it must not be quoted here because then the extra quotes will be part of the argument string.
    if arg.contains(invalid_character) && !arg.contains(" ") {
        if arg.starts_with("--") {
            // --argname[=value], long argument with an optional value.
            // If a long argument string contains invalid characters but no space, just append a space.
            return format!("{} ", arg);
        } else if arg.starts_with("-") {
            // -X[value], flag with value directly after (no space between).
            // todo Is this correct? Need to find a flag argument that takes a string and test.
            return format!("{} ", arg);
        } else {
            // Any other arguments should just be quoted.
            return format!("'{}'", arg);
        }
    }
    return arg;
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
        cwd_unix,
        String::from("&&"),
        String::from("git"),
    ];
    let git_cmd: String;
    let args: Vec<String> = env::args()
        .skip(1)
        .map(translate_path_to_unix)
        .collect::<Vec<String>>();

    if use_interactive_shell() {
        cmd_args.push("bash".to_string());
        cmd_args.push("-ic".to_string());

        git_args.extend(
            args.into_iter()
                .map(format_argument_for_interactive_shell)
                .map(escape_newline)
                .collect::<Vec<String>>(),
        );

        git_cmd = git_args.join(" ");
        cmd_args.push(git_cmd.clone());
    } else {
        git_args.extend(
            args.into_iter()
                .map(format_argument_for_non_interactive_shell)
                .map(escape_newline)
                .collect::<Vec<String>>(),
        );
        git_cmd = git_args.join(" ");
        cmd_args = git_args;
    }

    // setup stdin/stdout
    let stdin_mode = if env::args().last().unwrap() == "--version" {
        // For some reason, the git subprocess seems to hang, waiting for
        // input, when VS Code 1.17.2 tries to detect if `git --version` works
        // on Windows 10 1709 (specifically, in `findSpecificGit` in the
        // VS Code source file `extensions/git/src/git.ts`).
        // To workaround this, we only pass stdin to the git subprocess
        // for all other commands, but not for the initial `--version` check.
        // Stdin is needed for example when commiting, where the commit
        // message is passed on stdin.
        Stdio::null()
    } else {
        Stdio::inherit()
    };

    // setup the git subprocess launched inside WSL
    let mut git_proc_setup = Command::new("wsl");
    git_proc_setup.args(&cmd_args).stdin(stdin_mode);
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
    fn escape_newline_test() {
        assert_eq!(escape_newline("ab\ncdef".to_string()), "ab$\'\n\'cdef");
        assert_eq!(escape_newline("ab\ncd ef".to_string()), "ab$\'\n\'cd ef");
        // Long arguments with newlines...
        assert_eq!(escape_newline("--ab\ncdef".to_string()), "--ab$\'\n\'cdef");
        assert_eq!(
            escape_newline("--ab\ncd ef".to_string()),
            "--ab$\'\n\'cd ef"
        );
    }

    #[test]
    fn format_argument_for_interactive_shell_test() {
        assert_eq!(
            format_argument_for_interactive_shell("abcdef".to_string()),
            "abcdef"
        );
        assert_eq!(
            format_argument_for_interactive_shell("abc def".to_string()),
            "'abc def'"
        );
        assert_eq!(
            format_argument_for_interactive_shell("abc(def".to_string()),
            "'abc(def'"
        );
        assert_eq!(
            format_argument_for_interactive_shell("abc)def".to_string()),
            "'abc)def'"
        );
        assert_eq!(
            format_argument_for_interactive_shell("abc|def".to_string()),
            "'abc|def'"
        );
        assert_eq!(
            format_argument_for_interactive_shell("user.(name|email)".to_string()),
            "'user.(name|email)'"
        );
        assert_eq!(
            format_argument_for_interactive_shell("ab\"(c|d)\'ef".to_string()),
            "ab\"(c|d)\'ef"
        );
    }

    #[test]
    fn format_argument_for_non_interactive_shell_with_invalid_character() {
        assert_eq!(
            format_argument_for_non_interactive_shell("abc def".to_string()),
            "abc def"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("abc(def".to_string()),
            "'abc(def'"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("abc)def".to_string()),
            "'abc)def'"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("abc|def".to_string()),
            "'abc|def'"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("user.(name|email)".to_string()),
            "'user.(name|email)'"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("ab\"(c|d)\'ef".to_string()),
            "'ab(c|d)ef'"
        );
    }

    #[test]
    fn format_argument_for_non_interactive_shell_with_invalid_character_in_long_argument() {
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc def".to_string()),
            "--abc def"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=def".to_string()),
            "--abc=def"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=\"def\'".to_string()),
            "--abc=def"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=d ef".to_string()),
            "--abc=d ef"
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=d(ef".to_string()),
            "--abc=d(ef "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=d)ef".to_string()),
            "--abc=d)ef "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc=d|ef".to_string()),
            "--abc=d|ef "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--pretty=format:a(b|c)d".to_string()),
            "--pretty=format:a(b|c)d "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--pretty=format:a (b | c) d".to_string()),
            "--pretty=format:a (b | c) d"
        );
        // Long arguments with invalid characters in argument name
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc(def".to_string()),
            "--abc(def "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc)def".to_string()),
            "--abc)def "
        );
        assert_eq!(
            format_argument_for_non_interactive_shell("--abc|def".to_string()),
            "--abc|def "
        );
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
