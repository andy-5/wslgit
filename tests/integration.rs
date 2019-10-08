extern crate assert_cmd;
extern crate predicates;

#[cfg(test)]
mod integration {
    use assert_cmd::prelude::*;
    use predicates::prelude::*;
    use std::env;
    use std::process::Command;

    #[test]
    fn simple_argument() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .arg("--version")
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains("git version"));
    }

    #[test]
    fn argument_with_invalid_characters() {
        // https://github.com/andy-5/wslgit/issues/54
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["config", "--get-regex", "user.(name|email)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains("user.name"))
            .stdout(predicate::str::contains("user.email"));
    }

    #[test]
    fn quoted_argument_with_invalid_character() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pretty=\"format:(X|Y)\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("(X|Y)");
    }

    #[test]
    fn strangely_quoted_argument() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pr\"etty=format:(X|Y)\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("(X|Y)");
    }

    #[test]
    fn quoted_argument_with_invalid_character_and_spaces() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pretty=\"format:( X | Y )\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("( X | Y )");
    }

    #[test]
    fn argument_with_newline() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pretty=format:ab\ncd"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("ab\ncd");
    }

    #[test]
    fn short_argument_with_parameter_after_space() {
        // This is really stupid, hopefully first line of Cargo.toml won't change.
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "-L 1,1:Cargo.toml", "--", "Cargo.toml"])
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
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pretty=format:a ( b | c )"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("a ( b | c )");

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
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
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--pretty=format:a(b|c)"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout("a(b|c)");

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
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
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
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

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--oneline", "--", src_main_rel])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "--oneline", "--", src_main_abs])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["config", "--get-regexp", "^remote\\..*"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::is_empty().not());

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "-L", format!("1,1:{}", src_main_rel).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("@@ -0,0 +1,1 @@"));

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-n1", "-L", format!("1,1:{}", src_main_abs).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("@@ -0,0 +1,1 @@"));

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["log", "-L", format!(":main:{}", src_main_rel).as_str()])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(predicate::str::contains(
                "diff --git a/src/main.rs b/src/main.rs",
            ))
            .stdout(predicate::str::contains("fn main() {"));

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
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

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            .args(&["rev-parse", "--show-toplevel"])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .assert()
            .success()
            .stdout(cwd);
    }

    #[test]
    fn wslgit_environment_variable() {
        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            // Use pretty format to call 'env'
            .args(&["log", "-1", "--pretty=format:\"$(env)\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .env("WSLENV", "")
            .assert()
            .success()
            .stdout(predicate::str::contains("WSLGIT=1"))
            .stdout(predicate::str::contains("WSLENV=WSLGIT"));

        Command::cargo_bin(env!("CARGO_PKG_NAME"))
            .unwrap()
            // Use pretty format to call 'env'
            .args(&["log", "-1", "--pretty=format:\"$(env)\""])
            .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
            .env("WSLENV", "hello")
            .assert()
            .success()
            .stdout(predicate::str::contains("WSLGIT=1"))
            .stdout(predicate::str::contains("WSLENV=hello:WSLGIT"));
    }
}
