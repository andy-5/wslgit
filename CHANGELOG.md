# WSLGit Changelog

## [1.2.0] - unreleased

nothing yet


## [1.1.0] - 2022-03-13

### Changed

- Use full path to bash (#119)

### Fixed

- Fix test errors


## [1.0.1] - 2020-08-25

### Fixed

- Fix install script for paths with spaces (#110).


## [1.0.0] - 2020-08-22

### Added

- Add install script to create binaries and directory structure
  similar to Git for Windows. This enables tools to auto-detect Git,
  if the created directory is added to the Windows `Path`.
- Add proxy to call `Fork.RI` from WSL.

### Changed

- Add `ls-remote` to commands that use an interactive bash shell (#101).
- Treat file arguments after ` -- ` as relative paths (#102).
- Include version number in logging output (#105).
- Invoke `wsl` without default shell (#107).

### Fixed

- Fix translation of URLs that start with a transport protocol (#103).


## [0.9.0] - 2020-01-10

### Added

- New `WSLGIT` environment variable that is set to `1` by `wslgit` and
  shared to the WSL environment.

### Changed

- Use `wslpath` to translate paths between Windows and Linux (#12, #71).
- New default `smart` for `WSLGIT_USE_INTERACTIVE_SHELL` - only uses an
  interactive bash shell for `clone`, `fetch`, `pull` and `push`.

### Removed

- Remove `WSLGIT_MOUNT_ROOT` environment variable, this is handled by `wslpath` now.


## [0.8.0] - 2019-10-11

### Added

- New environment variable `WSLGIT_MOUNT_ROOT` to configure the
    WSL mount root (#78). 

### Fixed

- Improve shell escaping of invalid characters (#27, #54, #73),
    fixed by #74 and #76.
- Support flags for `BASH_ENV` in `WSLENV` environment variable (#56),
    fixed by #78.
- Fixed translating to windows path from the root of a mounted drive (#80).
- Support empty command line arguments (#84).

### Changed

- Format code using `rustfmt`.
- Unify interactive/non-interactive configurations, both use `bash -c` now.
- Expand tests and add integration tests (#76).
- `WSLGIT_USE_INTERACTIVE_SHELL` now has higher priority than a
    `BASH_ENV`/`WSLENV` configuration (#78).


## [0.7.0] - 2019-01-24

### Added

- Support for relative paths as arguments.
- Translate paths in long form arguments, e.g. `--file=C:\some\path`

### Fixed

- Support git commands in any argument position when deciding wether to
  translate paths in the output of the command.
- Fix incorrectly quoted arguments with spaces in non-interactive setup.

### Changed

- To support manually mounted network drives, the working directory inside WSL
  is now explicitly changed to the current working directory of `wslgit`
  in Windows.


## [0.6.0] - 2018-04-24

### Added

- Allow running bash in non-interactive mode (#16, #23).

### Fixed

- Unix paths inside file contents are not being erroneously translated anymore (#19).
- Do not assume valid UTF-8 output from git (#29).
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
[0.7.0]: https://github.com/andy-5/wslgit/releases/tag/v0.7.0
[0.8.0]: https://github.com/andy-5/wslgit/releases/tag/v0.8.0
[0.9.0]: https://github.com/andy-5/wslgit/releases/tag/v0.9.0
[1.0.0]: https://github.com/andy-5/wslgit/releases/tag/v1.0.0
[1.0.1]: https://github.com/andy-5/wslgit/releases/tag/v1.0.1
[1.0.1]: https://github.com/andy-5/wslgit/releases/tag/v1.1.0