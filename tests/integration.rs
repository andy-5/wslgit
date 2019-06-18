#[macro_use]
extern crate assert_cli;

#[cfg(test)]
mod integration {
    fn use_interactive_env() {
        ::std::env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "true");
    }

    fn use_non_interactive_env() {
        ::std::env::set_var("WSLGIT_USE_INTERACTIVE_SHELL", "false");
    }

    fn simple_argument_test() {
        assert_cmd!(wslgit "--version")
            .succeeds()
            .stdout()
            .contains("git version")
            .unwrap();
    }

    #[test]
    fn simple_argument_interactive() {
        use_interactive_env();
        simple_argument_test();
    }

    #[test]
    fn simple_argument_non_interactive() {
        use_non_interactive_env();
        simple_argument_test();
    }

    fn argument_with_invalid_characters_test() {
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
    fn argument_with_invalid_characters_interactive() {
        use_interactive_env();
        argument_with_invalid_characters_test();
    }

    #[test]
    fn argument_with_invalid_characters_non_interactive() {
        use_non_interactive_env();
        argument_with_invalid_characters_test();
    }

    fn quoted_argument_with_invalid_character_test() {
        assert_cmd!(wslgit log "-n1" "--pretty=\"format:(X|Y)\"")
            .succeeds()
            .stdout()
            .is("(X|Y)")
            .unwrap();
    }

    #[test]
    fn quoted_argument_with_invalid_character_interactive() {
        use_interactive_env();
        quoted_argument_with_invalid_character_test();
    }

    #[test]
    fn quoted_argument_with_invalid_character_non_interactive() {
        use_non_interactive_env();
        quoted_argument_with_invalid_character_test();
    }

    fn quoted_argument_with_invalid_character_and_space_test() {
        assert_cmd!(wslgit log "-n1" "--pretty=\"format:( X | Y )\"")
            .succeeds()
            .stdout()
            .is("( X | Y )")
            .unwrap();
    }

    #[test]
    fn quoted_argument_with_invalid_character_and_space_interactive() {
        use_interactive_env();
        quoted_argument_with_invalid_character_and_space_test();
    }

    #[test]
    fn quoted_argument_with_invalid_character_and_space_non_interactive() {
        use_non_interactive_env();
        quoted_argument_with_invalid_character_and_space_test();
    }

    fn argument_with_newline_test() {
        // https://github.com/andy-5/wslgit/issues/73
        assert_cmd!(wslgit log "-n1" "--pretty=format:XX\nYY")
            .succeeds()
            .stdout()
            .is("XX\nYY")
            .unwrap();
    }

    #[test]
    fn argument_with_newline_interactive() {
        use_interactive_env();
        argument_with_newline_test();
    }

    #[test]
    fn argument_with_newline_non_interactive() {
        use_non_interactive_env();
        argument_with_newline_test();
    }

    fn short_argument_with_parameter_after_space_test() {
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
    fn short_argument_with_parameter_after_space_interactive() {
        use_interactive_env();
        short_argument_with_parameter_after_space_test();
    }

    #[test]
    fn short_argument_with_parameter_after_space_non_interactive() {
        use_non_interactive_env();
        short_argument_with_parameter_after_space_test();
    }

    fn long_argument_with_invalid_characters_and_spaces_test() {
        assert_cmd!(wslgit log "-n1" "--pretty=format:a ( b | c )")
            .succeeds()
            .stdout()
            .is("a ( b | c )")
            .unwrap();

        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname) %(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0 c313ea9f9667e346ace079b47dc0d9f991fb5ab7\nrefs/tags/v0.2.0 43e0817f6c711abbcc5fe20bf7656fd26193fc0f")
            .unwrap();

        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname) %(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0 c313ea9f9667e346ace079b47dc0d9f991fb5ab7\nrefs/tags/v0.2.0 43e0817f6c711abbcc5fe20bf7656fd26193fc0f")
            .unwrap();
    }

    #[test]
    fn long_argument_with_invalid_characters_and_spaces_interactive() {
        use_interactive_env();
        long_argument_with_invalid_characters_and_spaces_test();
    }

    #[test]
    fn long_argument_with_invalid_characters_and_spaces_non_interactive() {
        use_non_interactive_env();
        long_argument_with_invalid_characters_and_spaces_test();
    }

    fn long_argument_with_invalid_characters_no_spaces_test() {
        assert_cmd!(wslgit log "-n1" "--pretty=format:a(b|c)")
            .succeeds()
            .stdout()
            .is("a(b|c)")
            .unwrap();
    }

    #[test]
    fn long_argument_with_invalid_characters_no_spaces_interactive() {
        use_interactive_env();
        long_argument_with_invalid_characters_no_spaces_test();

        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname)%(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0c313ea9f9667e346ace079b47dc0d9f991fb5ab7\nrefs/tags/v0.2.043e0817f6c711abbcc5fe20bf7656fd26193fc0f\n")
            .unwrap();
    }

    #[test]
    fn long_argument_with_invalid_characters_no_spaces_non_interactive() {
        use_non_interactive_env();
        long_argument_with_invalid_characters_no_spaces_test();

        assert_cmd!(wslgit "for-each-ref" "refs/tags" "--format=%(refname)%(objectname)")
            .succeeds()
            .stdout().contains("refs/tags/v0.1.0c313ea9f9667e346ace079b47dc0d9f991fb5ab7 \nrefs/tags/v0.2.043e0817f6c711abbcc5fe20bf7656fd26193fc0f \n")
            .unwrap();
    }

    fn long_argument_test() {
        // https://github.com/andy-5/wslgit/issues/46
        use_interactive_env();
        assert_cmd!(wslgit log "-n1" "--format=%x3c%x2ff%x3e%n%x3cr%x3e 01234%n%x3ca%x3e abcd")
            .succeeds()
            .stdout()
            .is("</f>\n<r> 01234\n<a> abcd")
            .unwrap();

        use_non_interactive_env();
        assert_cmd!(wslgit log "-n1" "--format=%x3c%x2ff%x3e%n%x3cr%x3e 01234%n%x3ca%x3e abcd")
            .succeeds()
            .stdout()
            .is("</f>\n<r> 01234\n<a> abcd")
            .unwrap();
    }

    #[test]
    fn long_argument_interactive() {
        use_interactive_env();
        long_argument_test();
    }

    #[test]
    fn long_argument_non_interactive() {
        use_non_interactive_env();
        long_argument_test();
    }
}
