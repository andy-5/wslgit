#[cfg(test)]
mod integration {
    use assert_cmd::Command;
    use predicates::prelude::*;

    /// Returns a Command for the sshgit binary with the environment isolated
    /// from any real config file or env vars that might be set on the machine.
    #[allow(deprecated)]
    fn sshgit() -> Command {
        let mut cmd = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
        // Point XDG_CONFIG_HOME somewhere that has no config file
        cmd.env("XDG_CONFIG_HOME", "/dev/null/no-such-dir");
        cmd.env_remove("SSHGIT_HOST");
        cmd.env_remove("SSHGIT_ROOT");
        cmd
    }

    #[test]
    fn fails_without_host_configured() {
        sshgit()
            .arg("status")
            .assert()
            .failure()
            .stderr(predicate::str::contains("host"));
    }

    #[test]
    fn fails_without_root_configured() {
        sshgit()
            .env("SSHGIT_HOST", "user@example.invalid")
            .arg("status")
            .assert()
            .failure()
            .stderr(predicate::str::contains("root"));
    }
}
