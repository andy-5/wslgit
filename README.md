# sshgit

A small Windows executable that transparently forwards git commands to a remote
host over SSH. Drop it in place of `git.exe` and GUI clients like
[Fork](https://fork.dev) will route all their git operations to your dev server
without needing a local git installation.

## How it works

When a git client calls `git <args>`, sshgit:

1. Detects the local repo name by walking up from the current directory to find `.git`, then taking the folder name
2. Runs `git -C <remote-root>/<repo-name> <args>` on the remote machine over SSH and streams the output back

Open different repos in Fork and sshgit automatically maps each one to its counterpart under the configured root on the remote.

## Getting the files (working tree)

sshgit handles the git protocol side; your GUI client also needs to be able to
read the actual working tree files. The recommended approach is to mount the
remote filesystem locally so that the two work together:

### SSHFS mount (recommended)

Install [WinFsp](https://github.com/winfsp/winfsp/releases) and
[SSHFS-Win](https://github.com/winfsp/sshfs-win/releases), then in Windows
Explorer map a network drive to:

```
\\sshfs\<user>@<host>\<path>
```

For example:

```
\\sshfs\jake@dev\home\jake
```

That drive letter (e.g. `Z:\`) gives Fork direct access to the files on the
remote. Open your repo folder from there as normal.

### WSL (if the host is a local WSL distro)

If `dev` is a local WSL instance the filesystem is already accessible via the
UNC path — no extra tooling needed:

```
\\wsl.localhost\Ubuntu\home\jake\myproject
```

### Clone locally

If you prefer a fully local workflow, clone from the remote as usual and point
Fork at the local copy. sshgit is not required in this case.

```bash
git clone jake@dev:/home/jake/myproject
```

## Installation

### Prerequisites

- [Git for Windows](https://git-scm.com/download/win)
- [WinFsp](https://github.com/winfsp/winfsp/releases) + [SSHFS-Win](https://github.com/winfsp/sshfs-win/releases) (to mount the remote filesystem)

### From a release (recommended)

1. Download the latest `sshgit-vX.X.X-windows-x86_64.zip` from the [releases page](../../releases)
2. Extract the zip
3. Run `install.bat`
4. Enter your SSH host and remote root path when prompted:

   ```
   SSH host (e.g. dev or 192.168.1.10): dev
   Root path on remote where repos live (e.g. /home/jake/Development): /home/jake/Development
   ```

5. Point your git GUI at:

   ```
   %LOCALAPPDATA%\sshgit\cmd\git.exe
   ```

   In Fork: **Preferences → Git → Git executable**

### From source

Requires [Rust](https://www.rust-lang.org). Clone the repo and run the same `install.bat` — it will build automatically if no pre-compiled binaries are found.

## Configuration

sshgit is configured via a TOML file at
`~/.config/sshgit/config.toml` (or `%XDG_CONFIG_HOME%/sshgit/config.toml`).
Environment variables override file values.

| Key | Env var | Required | Description |
|-----|---------|----------|-------------|
| `host` | `SSHGIT_HOST` | Yes | SSH host to connect to |
| `root` | `SSHGIT_ROOT` | Yes | Root path on the remote where repos live |
| `port` | — | No | SSH port (default 22) |
| `identity_file` | — | No | Path to SSH private key |

### Example

```toml
host = "dev"
root = "/home/jake/Development"
```

If your host doesn't resolve via normal DNS, add an entry to `~/.ssh/config`:

```
Host dev
    HostName <ip address>
    User <username>
```

## Running tests

```bash
# All tests (single-threaded to avoid env-var races)
cargo test -- --test-threads=1

# Unit tests only
cargo test test -- --test-threads=1

# Integration tests only
cargo test integration -- --test-threads=1
```
