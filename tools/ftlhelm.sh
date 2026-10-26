#!/bin/sh
# Runs a command with ftlhelm preloaded and the ftlhelm.ini next to this script.
#
#   Steam launch options:  /path/to/ftlhelm/ftlhelm.sh %command%
#   Without Steam:         /path/to/ftlhelm/ftlhelm.sh "/path/to/FTL Faster Than Light/FTL"
#
# Keeps the preloads already set (the Steam overlay) after ftlhelm.so. FTLHELM_LAYOUT, when
# already set, names another layout.
here=$(dirname "$(readlink -f "$0")")
export LD_PRELOAD="$here/ftlhelm.so${LD_PRELOAD:+:$LD_PRELOAD}"
export FTLHELM_LAYOUT="${FTLHELM_LAYOUT:-$here/ftlhelm.ini}"
exec "$@"
