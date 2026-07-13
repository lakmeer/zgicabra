#!/usr/bin/env bash
set -uo pipefail

BINARY="target/debug/zgicabra"

# Kill everything this script spawned when it exits (Ctrl+C, etc.)
trap 'kill 0' EXIT INT TERM

# Make sure a binary exists before we start watching it
cargo build

# Watcher 1: rebuild whenever anything under src/ changes.
# Re-armed in a loop since cargo build replaces the binary (breaking entr's watch on old inode).
(
    while true; do
        find src -type f | entr -cd cargo build
    done
) &

# Watcher 2: restart the running binary whenever its mtime changes (i.e. after each rebuild).
# `-r` ensures only one instance ever runs, so only one process holds the Hydra hardware handle.
# Re-armed in a loop for the same inode-replacement reason as above.
while true; do
    ls "$BINARY" | entr -r "$BINARY" 2>&-
done
