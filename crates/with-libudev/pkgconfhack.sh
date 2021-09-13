#!/usr/bin/env bash
#
# Hack to try to gain some visibility into what the `pkg-config` crate is doing
#
# If you set `PKG_CONFIG` env var to the path to this script, then the Rust
# `pkg-config` crate will invoke this script instead of the `pkg-config` executable,
# and then you can look at the resulting log file to see what env vars and args are
# being used.  This is very handy when debugging or trying different combinations of 
# env vars to find the right magic incantation.
SCRIPTPATH="$( cd -- "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"

echo "Invoked with args: ${@}" >> "$SCRIPTPATH/pkgconfig.log"
echo "Invoked with env: \n" >> "$SCRIPTPATH/pkgconfig.log"
env >> "$SCRIPTPATH/pkgconfig.log"

pkg-config "$@" >> "$SCRIPTPATH/pkgconfig.log"

pkg-config "$@"
