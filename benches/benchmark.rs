#![feature(test)]
extern crate test;

#[cfg(test)]
mod bench {
    use std::env;
    use std::process::Command;
    use test::Bencher;

    #[bench]
    fn no_translation(b: &mut Bencher) {
        b.iter(|| {
            Command::new("wslgit")
                .args(&["--version"])
                .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
                .output()
        })
    }

    #[bench]
    fn translate_absolute_argument(b: &mut Bencher) {
        let p = env::current_dir()
            .unwrap()
            .as_path()
            .join("src\\main.rs")
            .as_path()
            .to_string_lossy()
            .into_owned();
        let file_path = p.as_str();

        b.iter(|| {
            Command::new("wslgit")
                .args(&["log", "-n1", "--oneline", "--", file_path])
                .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
                .output()
        })
    }

    #[bench]
    fn translate_relative_argument(b: &mut Bencher) {
        let file_path = "src\\main.rs";

        b.iter(|| {
            Command::new("wslgit")
                .args(&["log", "-n1", "--oneline", "--", file_path])
                .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
                .output()
        })
    }

    #[bench]
    fn translate_output(b: &mut Bencher) {
        b.iter(|| {
            Command::new("wslgit")
                .args(&["rev-parse", "--show-toplevel"])
                .env("WSLGIT_USE_INTERACTIVE_SHELL", "false")
                .output()
        })
    }
}
