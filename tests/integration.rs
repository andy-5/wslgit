#[macro_use]
extern crate assert_cmd;
extern crate predicates;

#[cfg(test)]
mod integration {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::env;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;

    static LOG_TEST_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn simple_argument() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .arg("--version")
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains("git version"));
    }

    #[test]
    fn diagnostic_log_excludes_arguments_and_working_directory() {
        let _log_guard = LOG_TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let sentinel = "WSLGIT_SECRET_SENTINEL_7F3A9C";
        let binary = cargo_bin!(env!("CARGO_PKG_NAME"));
        let log_path = binary.parent().unwrap().join("wslgit.log");
        let rotated_log_path = binary.parent().unwrap().join("wslgit.log.1");
        let test_dir = env::temp_dir().join(sentinel);

        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);
        std::fs::create_dir_all(&test_dir).unwrap();

        Command::new(&binary)
            .args(&["-c", &format!("wslgit.test={}", sentinel), "--version"])
            .current_dir(&test_dir)
            .env("WSLGIT_ENABLE_LOGGING", "1")
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains("git version"));

        let log_contents = std::fs::read_to_string(&log_path).unwrap();
        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);
        std::fs::remove_dir_all(test_dir).unwrap();

        assert!(!log_contents.contains(sentinel), "{}", log_contents);
    }

    #[test]
    fn diagnostic_log_records_completion_metadata() {
        let _log_guard = LOG_TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let binary = cargo_bin!(env!("CARGO_PKG_NAME"));
        let log_path = binary.parent().unwrap().join("wslgit.log");
        let rotated_log_path = binary.parent().unwrap().join("wslgit.log.1");

        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);

        Command::new(&binary)
            .arg("--version")
            .env("WSLGIT_ENABLE_LOGGING", "true")
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success();

        let log_contents = std::fs::read_to_string(&log_path).unwrap();
        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);

        let records = log_contents.lines().collect::<Vec<_>>();
        assert!(!records.is_empty());

        let process_ids = records
            .iter()
            .map(|record| {
                let timestamp_ms = record
                    .split_whitespace()
                    .find_map(|field| field.strip_prefix("timestamp_ms="))
                    .expect("missing record timestamp")
                    .parse::<u128>()
                    .expect("record timestamp is not numeric");
                assert!(timestamp_ms > 0, "{}", record);

                record
                    .split_whitespace()
                    .find_map(|field| field.strip_prefix("pid="))
                    .expect("missing process ID")
                    .parse::<u32>()
                    .expect("process ID is not numeric")
            })
            .collect::<Vec<_>>();
        assert!(
            process_ids.windows(2).all(|pair| pair[0] == pair[1]),
            "{:?}",
            process_ids
        );

        let completion = records
            .iter()
            .find(|line| line.contains("event=complete "))
            .expect("missing completion record");
        let dispatch = records
            .iter()
            .find(|line| line.contains("event=dispatch target=wsl"))
            .expect("missing WSL dispatch record");
        assert!(dispatch.contains("pid="), "{}", dispatch);
        assert!(completion.contains("target=wsl"), "{}", completion);
        assert!(completion.contains("exit_status=0"), "{}", completion);
        let elapsed_ms = completion
            .split("elapsed_ms=")
            .nth(1)
            .expect("missing elapsed duration")
            .parse::<u128>()
            .expect("elapsed duration is not numeric");
        assert!(elapsed_ms < 60_000, "{}", completion);
    }

    #[test]
    fn diagnostic_log_rotation_survives_concurrent_processes() {
        const TEST_MAX_LOG_BYTES: usize = 1024 * 1024;

        let _log_guard = LOG_TEST_MUTEX
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let binary = cargo_bin!(env!("CARGO_PKG_NAME"));
        let log_path = binary.parent().unwrap().join("wslgit.log");
        let rotated_log_path = binary.parent().unwrap().join("wslgit.log.1");

        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);

        let history_marker = b"pre-rotation-history\n";
        let mut full_log = history_marker.to_vec();
        full_log.resize(TEST_MAX_LOG_BYTES, b'x');
        std::fs::write(&log_path, full_log).unwrap();

        let mut children = Vec::new();
        for _ in 0..8 {
            children.push(
                Command::new(&binary)
                    .arg("--version")
                    .env("WSLGIT_ENABLE_LOGGING", "1")
                    .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
        }
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }

        let active_log = std::fs::read_to_string(&log_path).unwrap();
        let rotated_log = std::fs::read(&rotated_log_path).unwrap();
        let active_size = std::fs::metadata(&log_path).unwrap().len();
        let rotated_size = std::fs::metadata(&rotated_log_path).unwrap().len();

        let _ = std::fs::remove_file(&log_path);
        let _ = std::fs::remove_file(&rotated_log_path);

        assert!(active_size <= TEST_MAX_LOG_BYTES as u64);
        assert!(rotated_size <= TEST_MAX_LOG_BYTES as u64);
        assert!(rotated_log.starts_with(history_marker));
        assert!(!active_log.is_empty());
        for record in active_log.lines() {
            assert!(record.starts_with("timestamp_ms="), "{}", record);
            assert!(record.contains(" pid="), "{}", record);
            assert!(record.contains(" event="), "{}", record);
        }
    }

    #[test]
    fn argument_with_invalid_characters() {
        // https://github.com/andy-5/wslgit/issues/54
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["config", "--get-regex", "user.(name|email)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains("user.name"))
            .stdout(predicate::str::contains("user.email"));
    }

    #[test]
    fn quote_characters_in_argument() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:\"(X|Y)\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("\"(X|Y)\"");
    }

    #[test]
    fn quote_characters_and_spaces() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:\"( X | Y )\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("\"( X | Y )\"");
    }

    #[test]
    fn argument_with_newline() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:ab\ncd"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("ab\ncd");
    }

    #[test]
    fn short_argument_with_parameter_after_space() {
        // This is really stupid, hopefully first line of Cargo.toml won't change.
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "-L 1,1:Cargo.toml"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/Cargo.toml b/Cargo.toml",
            ))
            .stdout(predicate::str::contains("@@ -0,0 +1,1 @@"));
    }

    #[test]
    fn long_argument_with_invalid_characters_and_spaces() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:<!--RevisionMessageEnd-->"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("<!--RevisionMessageEnd-->");

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:a ( b | c )"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("a ( b | c )");

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&[
                "for-each-ref",
                "refs/tags",
                "--format=%(refname) %(objectname)",
            ])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "refs/tags/v0.1.0 c313ea9f9667e346ace079b47dc0d9f991fb5ab7",
            ))
            .stdout(predicate::str::contains(
                "refs/tags/v0.2.0 43e0817f6c711abbcc5fe20bf7656fd26193fc0f",
            ));
    }

    #[test]
    fn long_argument_with_invalid_characters_no_spaces() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--pretty=format:a(b|c)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("a(b|c)");

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&[
                "for-each-ref",
                "refs/tags",
                "--format=%(refname)%(objectname)",
            ])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "refs/tags/v0.1.0c313ea9f9667e346ace079b47dc0d9f991fb5ab7",
            ))
            .stdout(predicate::str::contains(
                "refs/tags/v0.2.043e0817f6c711abbcc5fe20bf7656fd26193fc0f",
            ));
    }

    #[test]
    fn long_argument() {
        // https://github.com/andy-5/wslgit/issues/46
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&[
                "log",
                "-n1",
                "--format=%x3c%x2ff%x3e%n%x3cr%x3e 01234%n%x3ca%x3e abcd",
            ])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("</f>\n<r> 01234\n<a> abcd\n");
    }

    #[test]
    fn translate_arguments() {
        let src_main_rel = "src\\main.rs";
        let p = env::current_dir()
            .unwrap()
            .as_path()
            .join(src_main_rel)
            .as_path()
            .to_string_lossy()
            .into_owned();
        let src_main_abs = p.as_str();

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--oneline", "--", src_main_rel])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "--oneline", "--", src_main_abs])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["config", "--get-regexp", "^remote\\..*"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "-L", format!("1,1:{}", src_main_rel).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("@@ -0,0 +1,1 @@"));

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-n1", "-L", format!("1,1:{}", src_main_abs).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("@@ -0,0 +1,1 @@"));

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-L", format!(":main:{}", src_main_rel).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("fn main() {"));

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["log", "-L", format!(":main:{}", src_main_abs).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("fn main() {"));
    }

    #[test]
    fn translate_output() {
        let cwd = format!(
            "{}\n",
            env::current_dir()
                .unwrap()
                .as_path()
                .to_string_lossy()
                .into_owned()
        );

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["rev-parse", "--show-toplevel"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(cwd);
    }

    #[test]
    fn wslgit_environment_variable() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            // Use pretty format to call 'env'
            .args(&["log", "-1", "--pretty=format:$(env)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .env("WSLENV", "")
            .assert()
            .success()
            .stdout(predicate::str::contains("WSLGIT=1"))
            .stdout(predicate::str::contains("WSLENV=WSLGIT"));

        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            // Use pretty format to call 'env'
            .args(&["log", "-1", "--pretty=format:$(env)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .env("WSLENV", "hello")
            .assert()
            .success()
            .stdout(predicate::str::contains("WSLGIT=1"))
            .stdout(predicate::str::contains("WSLENV=hello:WSLGIT"));
    }

    #[test]
    fn shell_environment_variable() {
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            // Use pretty format to call 'printenv SHELL'
            .args(&["log", "-1", "--pretty=format:$(printenv SHELL)"])
            .assert()
            .success()
            .stdout(predicate::str::contains("/bin/bash"));
    }

    #[test]
    fn filename_with_dollar_sign() {
        // Test for files with $ characters like $id.tsx
        Command::new(cargo_bin!(env!("CARGO_PKG_NAME")))
            .args(&["status", "--", "$id.tsx"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success();
    }
}
