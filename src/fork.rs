use std::env;

use wsl;

/// Returns `true` if the process was invoked by `Fork.exe`.
pub fn needs_patching() -> bool {
    env::vars()
        .position(|var| "FORK_PROCESS_ID" == var.0)
        .is_some()
}

/// Patches the argument for Fork's interactive-rebase GUI.
///
/// If the argument is an editor and the editor is `Fork.RI.exe` then replace the
/// argument path with the path to the `Fork.RI` script and pass the path to
/// `Fork.RI.exe` to WSL using the `FORK_RI_EXE_PATH` environment variable.
/// The `Fork.RI` script is executed in WSL and will call `Fork.RI.exe` with
/// the path to `git-rebase-todo` converted to a Windows-path.
pub fn patch_argument(arg: String) -> String {
    lazy_static! {
        // "xxx.editor=xxx\Fork.RI.exe"
        static ref FORK_RI_EXE_PATH_EX: regex::Regex = regex::Regex::new(
            r"(?P<prefix>\.editor=)(?P<fork_ri_exe_path>.*Fork\.RI\.exe)"
        )
        .expect("Failed to compile FORK_RI_EXE_PATH_EX regex");
    }

    match FORK_RI_EXE_PATH_EX.captures(arg.as_str()) {
        Some(caps) => {
            let fork_ri_exe_path = caps.name("fork_ri_exe_path").unwrap().as_str();
            wsl::share_val("FORK_RI_EXE_PATH", fork_ri_exe_path, true);

            let fork_ri_script_path = match env::current_exe() {
                Ok(p) => p
                    .parent()
                    .unwrap()
                    .join("Fork.RI")
                    .to_string_lossy()
                    .into_owned(),
                Err(e) => {
                    eprintln!("Failed to get current exe path: {}", e);
                    panic!();
                }
            };

            let new_editor = format!("${{prefix}}{}", fork_ri_script_path);
            return FORK_RI_EXE_PATH_EX
                .replace_all(arg.as_str(), new_editor.as_str())
                .into_owned();
        }
        None => return arg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoked_by_fork() {
        env::set_var("FORK_PROCESS_ID", "5");
        assert_eq!(true, needs_patching());

        env::remove_var("FORK_PROCESS_ID");
        assert_eq!(false, needs_patching());
    }

    #[test]
    fn patch_argument_for_fork() {
        // The Fork.RI script is located in the same directory as the wslgit executable.
        let fork_ri_script_path: String = match env::current_exe() {
            Ok(p) => p
                .parent()
                .unwrap()
                .join("Fork.RI")
                .to_string_lossy()
                .into_owned(),
            Err(e) => {
                eprintln!("Failed to get current exe path: {}", e);
                panic!();
            }
        };

        env::set_var("FORK_PROCESS_ID", "42");

        assert_eq!(
            patch_argument("core.editor=C:\\one\\Fork.RI.exe".to_owned()),
            format!("core.editor={}", fork_ri_script_path)
        );
        assert!(env::vars()
            .position(|var| "FORK_RI_EXE_PATH" == var.0 && "C:\\one\\Fork.RI.exe" == var.1)
            .is_some());
        env::remove_var("FORK_RI_EXE_PATH");

        assert_eq!(
            patch_argument("sequence.editor=C:\\two\\Fork.RI.exe".to_owned()),
            format!("sequence.editor={}", fork_ri_script_path)
        );
        assert!(env::vars()
            .position(|var| "FORK_RI_EXE_PATH" == var.0 && "C:\\two\\Fork.RI.exe" == var.1)
            .is_some());
        env::remove_var("FORK_RI_EXE_PATH");
    }
}
