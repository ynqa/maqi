#!/bin/sh
# Recording-only headings; all editing and completion come from the real maqi.
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
maqi_bin=${MAQI_BIN:-"$script_dir/../target/debug/maqi"}

if [ ! -x "$maqi_bin" ]; then
    printf 'Build maqi first, or set MAQI_BIN: %s\n' "$maqi_bin" >&2
    exit 1
fi

case ${1:-} in
    1)
        title='01 / Input editing + session history'
        hint='Ctrl-A/E: move | Up: recall | Down: restore draft | Ctrl-C: cancel'
        ;;
    2)
        title='02 / Static kubectl completion from Usage'
        hint='Tab: complete / open menu | Up/Down: select | Enter: accept | Esc: close'
        ;;
    3)
        title='03 / Multiline input + editing across lines'
        hint='Trailing pipe: continue | Up/Down: move between lines | Enter: submit'
        ;;
    *)
        printf 'Usage: sh tapes/scene.sh {1|2|3}\n' >&2
        exit 1
        ;;
esac

printf '\033[2J\033[H'
printf '\033[1;36mmaqi / current prototype\033[0m\n'
printf 'Input + static completion; commands are echoed, not executed.\n\n'
printf '\033[1;33m%s\033[0m\n' "$title"
printf '\033[37m%s\033[0m\n\n' "$hint"
exec "$maqi_bin"
