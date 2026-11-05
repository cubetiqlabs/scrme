#!/usr/bin/env bash
# Memory / thread / CPU snapshot of scrme and its WebKit helper processes (macOS).
#
#   scripts/measure.sh [path/to/scrme] [captures]
#
# Launches the app, then reports: idle, after N `--screen` captures (+ CPU time per
# capture), and with Preferences open. Build first: cargo build --release --manifest-path src-tauri/Cargo.toml
#
# Note: WebKit helper processes can't be attributed to an app without root, so helpers are
# "processes that appeared after launch". Quit other web-view apps first for clean numbers
# (the idle section is reliable: right after launch nothing else is starting up).
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-src-tauri/target/release/scrme}"
N="${2:-10}"
[ -x "$BIN" ] || { echo "No binary at $BIN (build it first)" >&2; exit 1; }
pkill -x scrme 2>/dev/null || true; sleep 1

snapshot() { ps -axo pid= | tr -d ' ' | sort; }
BEFORE=$(mktemp); snapshot >"$BEFORE"
"$BIN" >/dev/null 2>&1 &
sleep 10
APP=$(pgrep -x scrme | head -n1)
[ -n "$APP" ] || { echo "scrme did not start" >&2; exit 1; }

# pid list = the app + every process that appeared with it and is a WebKit helper
pids() {
  echo "$APP"
  comm -13 "$BEFORE" <(snapshot) | while read -r p; do
    case "$(ps -p "$p" -o comm= 2>/dev/null)" in *com.apple.WebKit.*) echo "$p" ;; esac
  done
}
cpu_secs() { ps -p "$1" -o time= | awk -F'[:.]' '{ printf "%.2f", $1*60 + $2 + $3/100 }'; }

report() {
  local label="$1" total=0
  echo "── $label"
  printf '  %-34s %10s %8s %6s\n' process footprint threads cpu%
  for p in $(pids); do
    local name fp mb th cpu
    name=$(ps -p "$p" -o comm= | sed 's|.*/||' | cut -c1-34)
    fp=$(footprint -p "$p" 2>/dev/null | awk '/phys_footprint:/ {print $2, $3; exit}')
    mb=$(echo "$fp" | awk '{ if ($2 == "KB") printf "%.1f", $1/1024; else printf "%.1f", $1 }')
    th=$(( $(ps -M -p "$p" | wc -l) - 1 ))
    cpu=$(ps -p "$p" -o pcpu= | tr -d ' ')
    printf '  %-34s %8s MB %8s %6s\n' "$name" "$mb" "$th" "$cpu"
    total=$(echo "$total + $mb" | bc)
  done
  echo "  total footprint: $total MB, processes: $(pids | wc -l)"
}

report "idle"

C0=$(cpu_secs "$APP")
for _ in $(seq "$N"); do "$BIN" --screen; sleep 1.2; done
sleep 2
C1=$(cpu_secs "$APP")
report "after $N captures (--screen)"
echo "  app CPU per capture: $(echo "($C1 - $C0) / $N" | bc -l | awk '{printf "%.3f", $1}') s"

"$BIN" --prefs; sleep 4
report "Preferences open"

pkill -x scrme || true
rm -f "$BEFORE"
