# WSLGit

This project provides a small executable that forwards all arguments
to `git` running inside Bash on Windows/Windows Subsystem for Linux (WSL).

The primary reason for this tool is to make the Git plugin in
Visual Studio Code (VSCode) work with the `git` command installed in WSL.
For these two to interoperate, this tool translates paths
between the Windows (`C:\Foo\Bar`) and Linux (`/mnt/c/Foo/Bar`)
representations.

## Download

The latest binary release can be found on the
[releases page](https://github.com/andy-5/wslgit/releases).

You may also need to install the latest
[*Microsoft Visual C++ Redistributable for Visual Studio 2017*](https://aka.ms/vs/15/release/vc_redist.x64.exe).


## Usage in VSCode

To use this inside VSCode, put the `wslgit.exe` executable somewhere on
your computer and set the appropriate path in your VSCode `settings.json`:

```
{
    "git.path": "C:\\CHANGE\\TO\\PATH\\TO\\wslgit.exe"
}
```

Also make sure that you use an SSH key without password to access your
git repositories, or that your SSH key is added to a SSH agent running
within WSL before starting VSCode.
*You cannot enter your passphrase in VSCode!*

If you use a SSH agent, make sure that it does not print any text
(like e.g. *Agent pid 123*) during startup of an interactive bash shell.
If there is any additional output when your bash shell starts, the VSCode
Git plugin cannot correctly parse the output.


## Usage from the command line

Put the directory containing the executable somewhere on your Windows `Path`
environment variable and optionally rename `wslgit.exe` to `git.exe`.
To change the environment variable, type
`Edit environment variables for your account` into Start menu/Windows search
and use that tool to edit `Path`.

You can then just run any git command from a Windows console
by running `wslgit COMMAND` or `git COMMAND` and it uses the Git version
installed in WSL.


## Remarks

Currently, the path translation and shell escaping is very limited,
just enough to make it work in VSCode.

All absolute paths are translated, but relative paths are only
translated if they point to existing files or directories.
Otherwise it would be impossible to detect if an
argument is a relative path or just some other string.
VSCode always uses forward slashes for relative paths, so no
translation is necessary in this case.

Additionally, be careful with special characters interpreted by the shell.
Only spaces and newlines in arguments are currently handled.


## Advanced Usage

### WSLGIT_USE_INTERACTIVE_SHELL
To automatically support the common case where `ssh-agent` or similar tools are 
setup by `.bashrc` in interactive mode then, per default, `wslgit` executes `git` 
inside the WSL environment through `bash` started in interactive mode for some 
commands (`clone`, `fetch`, `pull` and `push`), and `bash` started in non-interactive 
mode for all other commands.

The behavior can be selected by setting an environment variable in Windows 
named `WSLGIT_USE_INTERACTIVE_SHELL` to one of the following values:
* `false` or `0` - Force `wslgit` to **always** start in **_non_-interactive** mode.
* `true`, `1`, or empty value - Force `wslgit` to **always** start in **interactive** mode.
* `smart` (default) - Interactive mode for `clone`, `fetch`, `pull`, `push`, 
non-interactive mode for all other commands. This is the default if the variable is not set.

Alternatively, if `WSLGIT_USE_INTERACTIVE_SHELL` is **not** set but the Windows 
environment variable `BASH_ENV` is set to a bash startup script and the environment 
variable `WSLENV` contains the string `"BASH_ENV"`, then `wslgit` assumes that 
the forced startup script from `BASH_ENV` contains everything you need, and 
therefore also starts bash in non-interactive mode.

This feature is only available in Windows 10 builds 17063 and later.

## Building from source

First, install Rust from https://www.rust-lang.org. Rust on Windows also
requires Visual Studio or the Visual C++ Build Tools for linking.

The final executable can then be build by running

```
cargo build --release
```

inside the root directory of this project. The resulting binary will
be located in `./target/release/`.

Tests **must** be run using one test thread because of race conditions when changing environment variables:
```bash
# Run all tests
cargo test -- --test-threads=1
# Run only unit tests
cargo test test -- --test-threads=1
# Run only integration tests
cargo test integration -- --test-threads=1
# Run benchmarks (requires nightly toolchain!)
cargo +nightly bench
```
