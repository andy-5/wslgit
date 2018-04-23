# WSLGit Changelog


## [0.6.0] - unreleased

### Added

- Allow running bash in non-interactive mode (#16, #23).

### Fixed

- Unix paths inside file contents are not being erroneously translated anymore (#19).
- Do not assume valid UTF-8 output from git.
- Fix running `wslgit` without arguments (#26).
- Escape `\n` newlines in arguments to git (#27).

### Changed

- Change to `wsl.exe` to call into the WSL environment.
- Apply path translation only to output of `rev-parse` and `remote`.


## [0.5.0] - 2018-01-11

### Added

- Return exit code from git subprocess.

### Fixed

- Fix superfluous empty `.git` source control providers.


## [0.4.0] - 2017-12-18

### Fixed

- Compatibility with VS Code 1.19, which now requires proper Windows paths
    (with backslashes) and a lowercase drive letter.


## [0.3.0] - 2017-11-08

### Added

- Add proper license (MIT).

### Fixed

- Git waiting for input when called from VS Code to check if `git --version`
    works.


## [0.2.0] - 2017-07-27

### Added

- Properly handle input via stdin (for commit messages).


## [0.1.0] - 2017-07-26

### Added

- Initial version of `wslgit` with basic functionality.


[0.1.0]: #
[0.2.0]: https://github.com/andy-5/wslgit/releases/tag/v0.2.0
[0.3.0]: https://github.com/andy-5/wslgit/releases/tag/v0.3.0
[0.4.0]: https://github.com/andy-5/wslgit/releases/tag/v0.4.0
[0.5.0]: https://github.com/andy-5/wslgit/releases/tag/v0.5.0
[0.6.0]: https://github.com/andy-5/wslgit/releases/tag/v0.6.0
