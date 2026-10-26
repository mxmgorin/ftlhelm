#!/bin/sh
# Runs a command with ftlhelm preloaded, the ftlhelm.ini and ftlhelm.log next to this script.
#
#   Steam launch options:  /path/to/ftlhelm/ftlhelm.sh %command%
#   Without Steam:         /path/to/ftlhelm/ftlhelm.sh "/path/to/FTL Faster Than Light/FTL"
#
# Keeps the preloads already set (the Steam overlay) after ftlhelm.so. FTLHELM_LAYOUT and
# FTLHELM_LOG, when already set, name another layout and log.
here=$(dirname "$(readlink -f "$0")")
export LD_PRELOAD="$here/ftlhelm.so${LD_PRELOAD:+:$LD_PRELOAD}"
export FTLHELM_LAYOUT="${FTLHELM_LAYOUT:-$here/ftlhelm.ini}"
export FTLHELM_LOG="${FTLHELM_LOG:-$here/ftlhelm.log}"
exec "$@"
