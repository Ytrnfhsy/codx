#!/usr/bin/env bash
# Compares two sets of screenshots from tools/screens.sh pixel by pixel:
#   tools/screens-diff.sh BEFORE_DIR AFTER_DIR [DIFF_DIR]
# What depends on the real clock is left out: the chat (its lines fade from the moment
# they arrive) and, in the battle royale, everything but the inventory panel (the hosted
# server runs in real time, so where the fall ends differs). A channel off by more than
# FUZZ per cent counts as a difference; up to NOISE such pixels per scene pass. The
# differences are drawn into DIFF_DIR.
set -u
A=${1:?} B=${2:?} D=${3:-$B/diff}
FUZZ=${FUZZ:-3}
NOISE=${NOISE:-100}
mkdir -p "$D"
CHAT="rectangle 0,415 980,560"
mask() { # scene -> ImageMagick draw commands that black out what is not compared
    case $1 in
        br_inventory) echo "rectangle 0,0 1279,92 rectangle 0,0 238,719 rectangle 1042,0 1279,719 rectangle 0,630 1279,719" ;;
        menu) echo "" ;;
        *) echo "$CHAT" ;;
    esac
}
bad=0
for f in "$A"/*.png; do
    n=$(basename "$f" .png)
    if [ ! -f "$B/$n.png" ]; then echo "$n: missing"; bad=1; continue; fi
    m=$(mask "$n")
    if [ -n "$m" ]; then
        convert "$f" -fill black -draw "$m" "$D/$n.a.png"
        convert "$B/$n.png" -fill black -draw "$m" "$D/$n.b.png"
    else
        cp "$f" "$D/$n.a.png"; cp "$B/$n.png" "$D/$n.b.png"
    fi
    px=$(compare -metric AE -fuzz "$FUZZ%" "$D/$n.a.png" "$D/$n.b.png" "$D/$n.png" 2>&1 >/dev/null)
    rm -f "$D/$n.a.png" "$D/$n.b.png"
    px=${px%%.*}
    if [ "$px" -gt "$NOISE" ]; then
        echo "$n: $px px differ  <-- see $D/$n.png"; bad=1
    else
        echo "$n: ok ($px px)"
    fi
done
exit $bad
