#!/usr/bin/env bash
# Screenshots of the client for comparing before and after a refactor:
#   tools/screens.sh OUT_DIR
# Every scene runs on a fixed clock (--fixed-clock) and a hosted server with a fixed seed,
# with its own empty settings, under a virtual X display if there is none. Compare two
# runs with tools/screens-diff.sh.
set -u
OUT=${1:?usage: tools/screens.sh OUT_DIR}
BIN=${BIN:-target/release/toon-warfare}
mkdir -p "$OUT"
OUT=$(cd "$OUT" && pwd)
WORK=$(mktemp -d)
if [ -z "${DISPLAY:-}" ]; then
    Xvfb :97 -screen 0 1280x720x24 >/dev/null 2>&1 &
    XPID=$!
    export DISPLAY=:97
    sleep 1
fi
shot() { # name after_seconds args...
    local name=$1 after=$2
    shift 2
    local home="$WORK/$name"
    mkdir -p "$home/.config/toon-warfare"
    # the same player, no FPS counter, no master server to ask
    cat >"$home/.config/toon-warfare/settings.json" <<JSON
{"name":"Tester","player_key":1,"party_token":"00000000000000000000000000000000",
 "show_fps":false,"master":"http://127.0.0.1:9"}
JSON
    HOME="$home" timeout 120 "$BIN" --no-sound --no-update --fixed-clock --lang uk \
        --screenshot "$OUT/$name.png" --screenshot-after "$after" "$@" \
        >"$WORK/$name.log" 2>&1 || echo "$name: exit $?" >&2
    [ -f "$OUT/$name.png" ] || { echo "$name: no screenshot (see $WORK/$name.log)" >&2; }
}
shot menu 3
shot tdm_hud 8 --host --map depot --mode tdm --bots 0 --seed 7 --auto --gun uar15
shot sniper_scope 9 --host --map depot --mode tdm --bots 0 --seed 7 --auto --gun awm --ads
shot tdm_fire 9 --host --map depot --mode tdm --bots 0 --seed 7 --auto --gun uar15 --hold-fire --pitch 0.2
shot esc_menu 8 --host --map depot --mode tdm --bots 0 --seed 7 --auto --open-menu
shot br_inventory 8 --host --map island --mode br --bots 0 --seed 7 --auto --open-inventory
[ -n "${XPID:-}" ] && kill "$XPID"
rm -rf "$WORK"
ls "$OUT"
