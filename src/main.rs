use std::borrow::Cow;
use std::env;

use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

#[macro_use]
extern crate lazy_static;
extern crate regex;
use regex::bytes::Regex;

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

fn translate_path_to_unix(argument: String) -> String {
    // An absolute or UNC path must:
    // 1. Be at the beginning of the string, or after a whitespace, colon, or equal-sign.
    // 2. Begin with <drive-letter>:\, <drive-letter>:/ or \\
    // 3. Not contain the characters: <>:|?' or newline.
    lazy_static! {
        static ref ABS_WINPATH_RE: Regex = Regex::new(
            r"(?-u)(?P<pre>^|[[:space:]]|:|=)(?P<path>([A-Z]:[\\/]|\\\\)([^<>:|?'\n]*[\\/]?)*)"
        )
        .expect("Failed to compile ABS_WINPATH_RE regex.");
    }

    let argument = &ABS_WINPATH_RE
        .replace_all(argument.as_bytes(), &b"${pre}$(wslpath '${path}')"[..])
        .into_owned();

    // Relative paths that needs to have their slashes changed must:
    // 1. Be at the beginning of the string, or after a whitespace, colon, or equal-sign.
    // 2. Begin with a string of valid characters (except \)...
    // 3. Followed by one \
    // 4. And then any number of valid characters (including \).
    lazy_static! {
        static ref REL_WINPATH_RE: Regex = Regex::new(
            r"(?-u)^(?P<before>[^\\]+([[:space:]]|:|=))?(?P<path>([^<>:|?'\n\\]+)\\([^<>:|?'\n]*))(?P<after>.*)"
        )
        .expect("Failed to compile REL_WINPATH_RE regex.");
    }

    {
        if REL_WINPATH_RE.is_match(argument) {
            let caps = REL_WINPATH_RE.captures(argument).unwrap();
            let path_cap = caps.name("path").unwrap();
            let path = std::str::from_utf8(&path_cap.as_bytes()).unwrap();

            // Make sure that it really is a relative path and not for example a regex...
            if Path::new(path).exists() {
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
    }

    std::str::from_utf8(&argument).unwrap().to_string()
}

// Translate absolute unix paths to windows paths by mapping what looks like a mounted drive ('/mnt/x') to a drive letter ('x:/').
// The path must either be the start of a line or start with a whitespace, and
// the path must be the end of a line, end with a / or end with a whitespace.
fn translate_path_to_win(line: &[u8]) -> Cow<[u8]> {
    let wslpath_re: Regex = Regex::new(
        format!(
            r"(?m-u)(^|(?P<pre>[[:space:]])){}(?P<drive>[A-Za-z])($|/|(?P<post>[[:space:]]))",
            mount_root()
        )
        .as_str(),
    )
    .expect("Failed to compile WSLPATH regex");

    wslpath_re.replace_all(line, &b"${pre}${drive}:/${post}"[..])
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
    } else if arg.contains(invalid_characters) || arg.is_empty() {
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
    fn use_interactive_shell_test() {
        // default
        env::remove_var("WSLGIT_USE_INTERACTIVE_SHELL");
        env::remove_var("BASH_ENV");
        env::remove_var("WSLENV");
        assert_eq!(use_interactive_shell(), true);

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
        assert_eq!(use_interactive_shell(), true);

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

        env::set_var("WSLENV", "NOT_BASH_ENV/up");
        assert_eq!(use_interactive_shell(), true);

        // WSLGIT_USE_INTERACTIVE_SHELL overrides BASH_ENV
        env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "true");
        assert_eq!(use_interactive_shell(), true);
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
    fn format_empty_argument() {
        assert_eq!(format_argument("".to_string()), "\"\"");
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
    }

    #[test]
    fn unix_to_win_path_trans() {
        env::remove_var("WSLGIT_MOUNT_ROOT");
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
        assert_eq!(
            &*translate_path_to_win(b"/mnt/c  /mnt/c/ /mnt/c/d /mnt/c/d/"),
            b"c:/  c:/ c:/d c:/d/"
        );

        env::set_var("WSLGIT_MOUNT_ROOT", "/abc/");
        assert_eq!(
            &*translate_path_to_win(b"/abc/d/some path/a file.md"),
            b"d:/some path/a file.md"
        );
        assert_eq!(
            &*translate_path_to_win(b"origin  /abc/c/path/ (fetch)"),
            b"origin  c:/path/ (fetch)"
        );
        let multiline = b"mirror  /abc/c/other/ (fetch)\nmirror  /abc/c/other/ (push)\n";
        let multiline_result = b"mirror  c:/other/ (fetch)\nmirror  c:/other/ (push)\n";
        assert_eq!(
            &*translate_path_to_win(&multiline[..]),
            &multiline_result[..]
        );
        assert_eq!(
            &*translate_path_to_win(b"/abc/c  /abc/c/ /abc/c/d /abc/c/d/"),
            b"c:/  c:/ c:/d c:/d/"
        );

        env::set_var("WSLGIT_MOUNT_ROOT", "/");
        assert_eq!(
            &*translate_path_to_win(b"/d/some path/a file.md"),
            b"d:/some path/a file.md"
        );
        assert_eq!(
            &*translate_path_to_win(b"origin  /c/path/ (fetch)"),
            b"origin  c:/path/ (fetch)"
        );
        let multiline = b"mirror  /c/other/ (fetch)\nmirror  /c/other/ (push)\n";
        let multiline_result = b"mirror  c:/other/ (fetch)\nmirror  c:/other/ (push)\n";
        assert_eq!(
            &*translate_path_to_win(&multiline[..]),
            &multiline_result[..]
        );
        assert_eq!(
            &*translate_path_to_win(b"/c  /c/ /c/d /c/d/"),
            b"c:/  c:/ c:/d c:/d/"
        );
    }

    #[test]
    fn no_path_translation() {
        env::remove_var("WSLGIT_MOUNT_ROOT");
        assert_eq!(
            &*translate_path_to_win(b"/mnt/other/file.sh /mnt/ab"),
            b"/mnt/other/file.sh /mnt/ab"
        );

        env::set_var("WSLGIT_MOUNT_ROOT", "/abc/");
        assert_eq!(
            &*translate_path_to_win(b"/abc/other/file.sh /abc/ab"),
            b"/abc/other/file.sh /abc/ab"
        );

        env::set_var("WSLGIT_MOUNT_ROOT", "/");
        assert_eq!(
            &*translate_path_to_win(b"/other/file.sh /ab"),
            b"/other/file.sh /ab"
        );
    }

    #[test]
    fn relative_path_translation() {
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
    }
}
