#!/bin/sh
# Runs a command with ftlpad preloaded and the ftlpad.ini next to this script.
#
#   Steam launch options:  /path/to/ftlpad/ftlpad.sh %command%
#   Without Steam:         /path/to/ftlpad/ftlpad.sh "/path/to/FTL Faster Than Light/FTL"
#
# Keeps the preloads already set (the Steam overlay) after ftlpad.so. FTLPAD_LAYOUT, when
# already set, names another layout.
here=$(dirname "$(readlink -f "$0")")
export LD_PRELOAD="$here/ftlpad.so${LD_PRELOAD:+:$LD_PRELOAD}"
export FTLPAD_LAYOUT="${FTLPAD_LAYOUT:-$here/ftlpad.ini}"
exec "$@"
