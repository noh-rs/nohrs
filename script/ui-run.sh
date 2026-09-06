#!/usr/bin/env bash
# Agent UI-verification helper for the Nohrs GUI on Linux (software Vulkan).
# See docs/agent-ui-verification.md for the full workflow and rationale.
#
# Subcommands:
#   setup            Create unversioned dev symlinks so the GUI links without root.
#   display          Print the X display to use (existing $DISPLAY, or a running Xvnc/Xvfb).
#   launch           (Re)launch the already-built binary, wait until it renders. Prints WINDOW id.
#   shot <out.png>   Capture the whole screen to a PNG (launch first if not running).
#   win              Print the nohrs window id (located by PID; WM_NAME is unset).
#   stop             Kill the running nohrs instance.
#
# Notes:
#   - Build separately first: RUSTFLAGS="-L $HOME/.local/devlibs" cargo build -p nohrs
#   - Drive input yourself with xdotool against the window id from `launch`/`win`.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$ROOT/target/debug/nohrs"
LOG="${NOHRS_LOG:-/tmp/nohrs.log}"
DEVLIBS="$HOME/.local/devlibs"
LIBDIR="${LIBDIR:-/usr/lib/x86_64-linux-gnu}"
TMP="${TMPDIR:-/tmp}"
WINDOW_POLLS=120       # x 0.5s: how long to wait for the X window to be created
REDRAW_ATTEMPTS=20     # x ~2.5s: how long to keep nudging before giving up on a frame
UNIFORM_EXIT=3         # xwd2png.py --fail-if-uniform: captured frame is one flat color

pause() {
  # The 'sleep' binary is blocked in some agent shells; this is not.
  perl -e 'select(undef, undef, undef, $ARGV[0])' "$1"
}

setup() {
  mkdir -p "$DEVLIBS"
  # gpui links these; create unversioned symlinks to the runtime .so (no dev packages / root needed).
  local pairs=(
    "libxcb.so.1:libxcb.so" "libxkbcommon.so.0:libxkbcommon.so"
    "libxkbcommon-x11.so.0:libxkbcommon-x11.so" "libwayland-client.so.0:libwayland-client.so"
    "libwayland-cursor.so.0:libwayland-cursor.so" "libwayland-egl.so.1:libwayland-egl.so"
    "libvulkan.so.1:libvulkan.so"
  )
  for p in "${pairs[@]}"; do
    ln -sf "$LIBDIR/${p%%:*}" "$DEVLIBS/${p##*:}"
  done
  echo "devlibs ready at $DEVLIBS (build with: RUSTFLAGS=\"-L $DEVLIBS\" cargo build -p nohrs)"
}

resolve_display() {
  if [ -n "${DISPLAY:-}" ]; then echo "$DISPLAY"; return 0; fi
  local d
  d=$(ps -eo args 2>/dev/null | grep -oE '(Xvnc|Xvfb) :[0-9]+' | grep -oE ':[0-9]+' | head -1)
  if [ -n "$d" ]; then echo "$d"; return 0; fi
  echo "no display: set \$DISPLAY, or start one e.g. 'Xvfb :99 -screen 0 1280x800x24 &'" >&2
  return 1
}

win_id() {
  local pid; pid=$(pgrep -x nohrs | head -1) || return 1
  [ -n "$pid" ] || return 1
  DISPLAY="$(resolve_display)" xdotool search --pid "$pid" 2>/dev/null | head -1
}

# Force a redraw. Under a bare X server (no window manager) nothing ever damages the
# window after it is mapped, so gpui presents no further frame and the surface stays
# black indefinitely -- waiting does not help. A one-pixel resize and back does.
nudge() {
  local disp="$1" window="$2" geometry width height
  geometry="$(DISPLAY="$disp" xdotool getwindowgeometry --shell "$window")" || return 1
  width="$(sed -n 's/^WIDTH=//p' <<< "$geometry")"
  height="$(sed -n 's/^HEIGHT=//p' <<< "$geometry")"
  case "$width$height" in
    *[!0-9]* | "") echo "could not read geometry of window $window" >&2; return 1 ;;
  esac
  DISPLAY="$disp" xdotool windowsize "$window" "$((width - 1))" "$((height - 1))" || return 1
  pause 1
  DISPLAY="$disp" xdotool windowsize "$window" "$width" "$height" || return 1
}

capture() {   # capture <out.png> <display> <root|window-id> [xwd2png.py flags...]
  local out="$1" disp="$2" target="$3"; shift 3
  local -a selector
  if [ "$target" = root ]; then selector=(-root); else selector=(-id "$target"); fi
  local dump; dump="$(mktemp "$TMP/ui-shot.XXXXXX.xwd")" || return 1
  DISPLAY="$disp" xwd "${selector[@]}" -silent -out "$dump" \
    || { rm -f "$dump"; echo "xwd failed" >&2; return 1; }
  python3 "$ROOT/script/xwd2png.py" "$dump" "$out" "$@"
  local status=$?
  rm -f "$dump"
  return "$status"
}

# Exit status of a --fail-if-uniform capture of the app's OWN window: 0 = it has
# presented a real frame, UNIFORM_EXIT = still one flat color, anything else = the
# capture itself failed. Probing the root window instead would be wrong on any
# display that has a desktop or other windows on it: those pixels alone make the
# capture non-uniform, so a blank nohrs window would read as rendered.
window_frame_status() {
  local disp="$1" window="$2"
  local probe="$TMP/ui-frame-probe.$$.png"
  capture "$probe" "$disp" "$window" --fail-if-uniform >/dev/null 2>&1
  local status=$?
  rm -f "$probe"
  return "$status"
}

launch() {
  local disp; disp="$(resolve_display)" || return 1
  [ -x "$BIN" ] || { echo "binary not built: $BIN" >&2; return 1; }
  pkill -x nohrs 2>/dev/null   # never use 'pkill -f' here: it matches this script's own path.
  : > "$LOG"
  DISPLAY="$disp" setsid "$BIN" > "$LOG" 2>&1 < /dev/null &

  local window="" attempt seen_process=false
  for attempt in $(seq 1 "$WINDOW_POLLS"); do
    if pgrep -x nohrs >/dev/null; then
      seen_process=true
      window="$(win_id)"
      [ -n "$window" ] && break
    elif [ "$seen_process" = true ]; then
      # Gone after having been seen: a real crash, not the startup race between
      # backgrounding `setsid` and the binary appearing under its own name.
      echo "nohrs exited during startup; tail of $LOG:" >&2
      tail -5 "$LOG" >&2
      return 1
    fi
    pause 0.5
  done
  [ -n "$window" ] || { echo "window id not found; tail of $LOG:" >&2; tail -5 "$LOG" >&2; return 1; }

  # Software (llvmpipe) rendering plus the no-damage problem above: the only reliable
  # readiness signal is the window's own pixels no longer being a single flat color.
  for attempt in $(seq 1 "$REDRAW_ATTEMPTS"); do
    nudge "$disp" "$window" || return 1
    pause 1.5
    if window_frame_status "$disp" "$window"; then
      echo "WINDOW=$window DISPLAY=$disp PID=$(pgrep -x nohrs | head -1)"
      return 0
    fi
  done
  echo "window still blank after $REDRAW_ATTEMPTS redraw attempts; tail of $LOG:" >&2
  tail -5 "$LOG" >&2
  return 1
}

shot() {
  local out="${1:?usage: ui-run.sh shot <out.png>}"
  local disp; disp="$(resolve_display)" || return 1
  pgrep -x nohrs >/dev/null || launch >/dev/null || return 1
  # The PNG is the whole screen, so its pixel coordinates match the ones you drive
  # xdotool with; the blank check below still has to look at the window alone.
  capture "$out" "$disp" root || return 1
  local window; window="$(win_id)"
  if [ -n "$window" ]; then
    window_frame_status "$disp" "$window"
    if [ $? -eq "$UNIFORM_EXIT" ]; then
      echo "warning: the nohrs window in $out is one flat color -- it has not presented a frame." >&2
      echo "         re-run './script/ui-run.sh launch' to force the redraw." >&2
    fi
  fi
}

case "${1:-}" in
  setup)   setup ;;
  display) resolve_display ;;
  launch)  launch ;;
  shot)    shift; shot "$@" ;;
  win)     win_id ;;
  stop)    pkill -x nohrs && echo "stopped" || echo "not running" ;;
  *) echo "usage: $0 {setup|display|launch|shot <out.png>|win|stop}" >&2; exit 2 ;;
esac
