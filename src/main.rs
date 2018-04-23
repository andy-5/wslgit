use std::env;
use std::process::{Command, Stdio};
use std::io::{self, Write};
use std::borrow::Cow;

#[macro_use] extern crate lazy_static;
extern crate regex;
use regex::bytes::Regex;


fn translate_path_to_unix(arg: String) -> String {
    if let Some(index) = arg.find(":\\") {
        if index != 1 {
            // Not a path
            return arg;
        }
        let mut path_chars = arg.chars();
        if let Some(drive) = path_chars.next() {
            let mut wsl_path = String::from("/mnt/");
            wsl_path.push_str(&drive.to_lowercase().collect::<String>());
            path_chars.next();
            wsl_path.push_str(&path_chars.map(|c|
                    match c {
                        '\\' => '/',
                        _ => c,
                    }
                ).collect::<String>());
            return wsl_path;
        }
    }
    arg
}

fn translate_path_to_win(line: &[u8]) -> Cow<[u8]> {
    lazy_static! {
        static ref WSLPATH_RE: Regex = 
            Regex::new(r"(?m-u)/mnt/(?P<drive>[A-Za-z])(?P<path>/\S*)")
                .expect("Failed to compile WSLPATH regex");
    }
    WSLPATH_RE.replace_all(line, &b"${drive}:${path}"[..])
}

fn shell_escape(arg: String) -> String {
    // ToDo: This really only handles arguments with spaces and newlines.
    // More complete shell escaping is required for the general case.
    if arg.contains(" ") {
        return vec![
            String::from("\""),
            arg,
            String::from("\"")].join("");
    }
    arg.replace("\n", "$'\n'")
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
            if wslenv.split(':').position(|r| r.eq_ignore_ascii_case("BASH_ENV")).is_some() {
                return false;
            }
        }
    }
    true
}


fn main() {
    let mut cmd_args = Vec::new();
    let mut git_args: Vec<String> = vec![String::from("git")];
    let git_cmd: String;

    // process git command arguments   
    git_args.extend(env::args().skip(1)
        .map(translate_path_to_unix)
        .map(shell_escape));
    git_cmd = git_args.join(" ");

    if use_interactive_shell() {
        cmd_args.push("bash".to_string());
        cmd_args.push("-ic".to_string());
        cmd_args.push(git_cmd.clone());
    }
    else {
        cmd_args.clone_from(&git_args);
    }

    // setup stdin/stdout
    let stdin_mode = if git_cmd.ends_with("--version") {
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
    git_proc_setup.args(&cmd_args)
        .stdin(stdin_mode);
    let status;

    // add git commands that must use translate_path_to_win
    const TRANSLATED_CMDS: &[&str] = &["rev-parse", "remote"];

    let have_args = git_args.len() > 1;
    let translate_output = if have_args {
       TRANSLATED_CMDS.iter().position(|&r| r == git_args[1]).is_some()
    } else {
        false
    };

    if translate_output {
        // run the subprocess and capture its output
        let git_proc = git_proc_setup.stdout(Stdio::piped())
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
    }
    else {
        // run the subprocess without capturing its output
        // the output of the subprocess is passed through unchanged
        status = git_proc_setup.status()
            .expect(&format!("Failed to execute command '{}'", &git_cmd));
    }

    // forward any exit code
    if let Some(exit_code) = status.code() {
        std::process::exit(exit_code);
    }
}


#[test]
fn win_to_unix_path_trans() {
    assert_eq!(
        translate_path_to_unix("d:\\test\\file.txt".to_string()),
        "/mnt/d/test/file.txt");
    assert_eq!(
        translate_path_to_unix("C:\\Users\\test\\a space.txt".to_string()),
        "/mnt/c/Users/test/a space.txt");
}

#[test]
fn unix_to_win_path_trans() {
    assert_eq!(
        &*translate_path_to_win(b"/mnt/d/some path/a file.md"),
        b"d:/some path/a file.md");
    assert_eq!(
        &*translate_path_to_win(b"origin  /mnt/c/path/ (fetch)"),
        b"origin  c:/path/ (fetch)");
    let multiline = b"mirror  /mnt/c/other/ (fetch)\nmirror  /mnt/c/other/ (push)\n";
    let multiline_result = b"mirror  c:/other/ (fetch)\nmirror  c:/other/ (push)\n";
    assert_eq!(
        &*translate_path_to_win(&multiline[..]),
        &multiline_result[..]);
}

#[test]
fn no_path_translation() {
    assert_eq!(
        &*translate_path_to_win(b"/mnt/other/file.sh"),
        b"/mnt/other/file.sh");
}

