use std::env;
use std::process::{Command, Stdio};

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
    if arg.contains(" ") {
        return vec![
            String::from("\""),
            arg,
            String::from("\"")].join("");
    }
    arg
}

fn main() {
    let mut git_args: Vec<String> = vec![String::from("git")];
    git_args.extend(env::args().skip(1)
        .map(translate_path_to_unix)
        .map(shell_escape));
    let git_cmd = git_args.join(" ");
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
    let git_proc = Command::new("bash")
        .env("BASH_ENV", "~/.bashrc")
        .arg("-c")
        .arg(&git_cmd)
        .stdin(stdin_mode)
        .stdout(Stdio::piped())
        .spawn()
        .expect(&format!("Failed to execute command '{}'", &git_cmd));
    let output = git_proc
        .wait_with_output()
        .expect(&format!("Failed to wait for git call '{}'", &git_cmd));
    let output_str = String::from_utf8_lossy(&output.stdout);
    // add git commands that must skip translate_path_to_win
    // e.g. = &["show", "status, "rev-parse", "for-each-ref"];
    const NO_TRANSLATE: &'static [&'static str] = &["show"];
    if NO_TRANSLATE.iter().position(|&r| r == git_args[1]).is_none() {
        for line in output_str.lines().map(translate_path_to_win) {
            println!("{}", line);
        }
    }
    else {
        print!("{}", output_str);
    }
    if let Some(exit_code) = output.status.code() {
        std::process::exit(exit_code);
    }
}
