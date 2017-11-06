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

Just put the executable somewhere on your `PATH` and optionally rename it
to `git.exe`. You can then just run any git command from a Windows console
by running `wslgit COMMAND` or `git COMMAND` and it uses the Git version
installed in WSL.


## Remarks

Currently, the path translation and shell escaping is very limited,
just enough to make it work in VSCode.

Only absolute paths are translated, because it is hard to detect if an
argument is a relative path or just some other string.
Also, VSCode always uses forward slashes for relative paths, so no
translation is necessary.

Addtionally, be careful with special characters interpreted by the shell.
Only spaces in arguments are currently handled.

