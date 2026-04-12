#![feature(test)]
extern crate test;

#[cfg(test)]
mod bench {
    use std::process::Command;
    use test::Bencher;

    #[bench]
    fn no_config_error_path(b: &mut Bencher) {
        b.iter(|| {
            Command::new(env!("CARGO_BIN_EXE_sshgit"))
                .env("XDG_CONFIG_HOME", "/dev/null/no-such-dir")
                .env_remove("SSHGIT_HOST")
                .env_remove("SSHGIT_ROOT")
                .arg("status")
                .output()
        })
    }
}
