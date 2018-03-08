use std::env;
use std::process::{Command, Stdio};
use std::io::{self, Write};

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

fn translate_path_to_win(line: &str) -> String {
    if let Some(index) = line.find("/mnt/") {
        if index != 0 {
            // Path somewhere in the middle, don't change
            return String::from(line);
        }
        let mut path_chars = line.chars();
        if let Some(drive) = path_chars.nth(5) {
            if let Some(slash) = path_chars.next() {
                if slash != '/' {
                    // not a windows mount
                    return String::from(line);
                }
                let mut win_path = String::from(
                    drive.to_lowercase().collect::<String>());
                win_path.push_str(":\\");
                win_path.push_str(&path_chars.collect::<String>());
                return win_path.replace("/", "\\");
            }
        }
    }
    String::from(line)
}

fn shell_escape(arg: String) -> String {
    // ToDo: This really only handles arguments with spaces.
    // More complete shell escaping is required for the general case.
    if arg.contains(" ") || arg.contains("\n") {
        return vec![
            String::from("\""),
            arg,
            String::from("\"")].join("");
    }
    arg
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
    if use_interactive_shell() {
        git_args.extend(env::args().skip(1)
            .map(translate_path_to_unix)
            .map(shell_escape));
        git_cmd = git_args.join(" ");
        cmd_args.push("bash".to_string());
        cmd_args.push("-ic".to_string());
        cmd_args.push(git_cmd.clone());
    }
    else {
        git_args.extend(env::args().skip(1)
        .map(translate_path_to_unix));
        git_cmd = git_args.join(" ");
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

    // add git commands that must skip translate_path_to_win
    // e.g. = &["show", "status, "rev-parse", "for-each-ref"];
    const NO_TRANSLATE: &'static [&'static str] = &["show"];

    let have_args = git_args.len() > 1;
    let translate_output = if have_args {
       NO_TRANSLATE.iter().position(|&r| r == git_args[1]).is_none() 
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
        // force with no checking or conversion returned data
        // into a Rust UTF-8 String
        let output_str = unsafe {
            String::from_utf8_unchecked(output.stdout)
        };
        // iterate through lines (LR or CRLF endings) and output
        // each line with paths translated and ending with the
        // native line ending (CRLF)
        for line in output_str.lines().map(translate_path_to_win) {
            println!("{}", line);
        }
        status = output.status;
    }
    else {
        // run the subprocess without capturing its output
        // the output of the subprocess is passed through unchanged
        status = git_proc_setup.status()
            .expect(&format!("Failed to execute command '{}'", &git_cmd));
    }

    // std::process::exit does not call destructors; must manually flush stdout
    io::stdout().flush().unwrap();

    // forward any exit code
    if let Some(exit_code) = status.code() {
        std::process::exit(exit_code);
    }
}
