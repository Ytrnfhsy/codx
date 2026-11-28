#!/usr/bin/env bash
# Measures the client on this computer: every scene with the Low and the High preset,
# SECS seconds each (after a 5 s warm-up), one Markdown report per run in OUT_DIR.
#   tools/profile-client.sh [OUT_DIR] [SECS]
# The game must be closed. Your saved settings are not changed (--video is for one run).
set -u
OUT=${1:-profile}
SECS=${2:-30}
BIN=${BIN:-target/release/toon-warfare}
mkdir -p "$OUT"
COMMON="--no-sound --no-update --no-vsync --profile $SECS"
GAME="--host --seed 7 --auto"
scene() { # name args...
    local name=$1
    shift
    for v in low high; do
        echo "== $name ($v)"
        "$BIN" $COMMON --video $v --profile-out "$OUT/$name-$v.md" "$@"
    done
}
scene menu
scene tdm $GAME --map depot --mode tdm --bots 9 --gun uar15
scene br-town $GAME --map island --mode br --bots 31 --team spec \
    --spec-at -180,9.6,-80 --look-at -60,9,-80
scene br-field $GAME --map island --mode br --bots 31 --team spec \
    --spec-at 60,11.2,-700 --look-at -120,9,-80
scene br-drop $GAME --map island --mode br --bots 31 --br-jump
scene scope8x $GAME --map site --mode tdm --bots 9 --gun awm --ads
echo "Reports: $OUT/"
