#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."
fork_ri=resources/Fork.RI

uname() {
    printf 'Linux test 6.6.0-microsoft-standard-WSL2 x86_64 GNU/Linux\n'
}

cmd.exe() {
    printf 'unexpected cmd.exe invocation\n' >&2
    return 99
}

chmod() {
    return 0
}

wslpath() {
    if [[ ${MOCK_WSLPATH_FAIL:-0} == 1 ]]; then
        return 23
    fi
    if [[ $# -ne 3 || $1 != -w || $2 != -- || $3 != '/tmp/rebase todo' ]]; then
        printf 'unexpected wslpath arguments: %q\n' "$*" >&2
        return 22
    fi
    printf 'C:\\repo\\git-rebase-todo\n'
}

fork_ri_editor() {
    printf 'editor:%s\n' "$1"
}

export -f uname cmd.exe chmod wslpath fork_ri_editor

set +e
output=$(
    WSL_INTEROP=/run/WSL/1_interop \
        FORK_RI_EXE_PATH=fork_ri_editor \
        bash "$fork_ri" '/tmp/rebase todo' 2>&1
)
status=$?
set -e

if [[ $status -ne 0 ]]; then
    printf 'Fork.RI exited with %d:\n%s\n' "$status" "$output" >&2
    exit 1
fi
if [[ $output != 'editor:C:\repo\git-rebase-todo' ]]; then
    printf 'Unexpected Fork.RI output:\n%s\n' "$output" >&2
    exit 1
fi

set +e
output=$(
    WSL_INTEROP=/run/WSL/1_interop \
        FORK_RI_EXE_PATH=fork_ri_editor \
        MOCK_WSLPATH_FAIL=1 \
        bash "$fork_ri" '/tmp/rebase todo' 2>&1
)
status=$?
set -e

if [[ $status -ne 23 ]]; then
    printf 'Expected wslpath status 23, got %d:\n%s\n' "$status" "$output" >&2
    exit 1
fi
if [[ $output == *editor:* ]]; then
    printf 'Editor ran after wslpath failed:\n%s\n' "$output" >&2
    exit 1
fi
