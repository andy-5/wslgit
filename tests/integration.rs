#[macro_use]
extern crate assert_cli;

#[cfg(test)]
mod integration {
    #[test]
    fn simple_argument() {
        assert_cmd!(wslgit "--version")
            .succeeds()
            .stdout()
            .contains("git version")
            .unwrap();
    }

    #[test]
    fn argument_with_invalid_characters() {
        // https://github.com/andy-5/wslgit/issues/54
        assert_cmd!(wslgit config "--get-regex" "user.(name|email)")
            .succeeds()
            .stdout()
            .contains("user.name")
            .stdout()
            .contains("user.email")
            .unwrap();
    }

    #[test]
    fn quoted_argument_with_invalid_character() {
        assert_cmd!(wslgit log "-n1" "--pretty=\"format:(X|Y)\"")
            .succeeds()
            .stdout()
            .is("(X|Y)")
            .unwrap();
    }

    #[test]
    fn strangely_quoted_argument() {
        assert_cmd!(wslgit log "-n1" "--pr\"etty=format:(X|Y)\"")
            .succeeds()
            .stdout()
            .is("(X|Y)")
            .unwrap();
    }

    #[test]
    fn quoted_argument_with_invalid_character_and_spaces() {
        assert_cmd!(wslgit log "-n1" "--pretty=\"format:( X | Y )\"")
            .succeeds()
            .stdout()
            .is("( X | Y )")
            .unwrap();
    }

    #[test]
    fn argument_with_newline() {
        // https://github.com/andy-5/wslgit/issues/73
        assert_cmd!(wslgit log "-n1" "--pretty=format:ab\ncd")
            .succeeds()
            .stdout()
            .is("ab\ncd")
            .unwrap();
    }

    #[test]
    fn short_argument_with_parameter_after_space() {
        // This is really stupid, hopefully first line of Cargo.toml won't change.
        assert_cmd!(wslgit log "-n1" "-L 1,1:Cargo.toml" "--" "Cargo.toml")
            .succeeds()
            .stdout()
            .contains("diff --git a/Cargo.toml b/Cargo.toml")
            .stdout()
            .contains("@@ -0,0 +1,1 @@")
            .unwrap();
    }

    #[test]
    fn long_argument_with_invalid_characters_and_spaces() {
        assert_cmd!(wslgit log "-n1" "--pretty=format:a ( b | c )")
            .succeeds()
            .stdout()
            .is("a ( b | c )")
            .unwrap();
        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname) %(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0 c313ea9f9667e346ace079b47dc0d9f991fb5ab7\nrefs/tags/v0.2.0 43e0817f6c711abbcc5fe20bf7656fd26193fc0f")
            .unwrap();
    }

    #[test]
    fn long_argument_with_invalid_characters_no_spaces() {
        assert_cmd!(wslgit log "-n1" "--pretty=format:a(b|c)")
            .succeeds()
            .stdout()
            .is("a(b|c)")
            .unwrap();
        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname)%(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0c313ea9f9667e346ace079b47dc0d9f991fb5ab7\nrefs/tags/v0.2.043e0817f6c711abbcc5fe20bf7656fd26193fc0f\n")
            .unwrap();
    }

    #[test]
    fn long_argument() {
        // https://github.com/andy-5/wslgit/issues/46
        assert_cmd!(wslgit log "-n1" "--format=%x3c%x2ff%x3e%n%x3cr%x3e 01234%n%x3ca%x3e abcd")
            .succeeds()
            .stdout()
            .is("</f>\n<r> 01234\n<a> abcd")
            .unwrap();
    }
}
