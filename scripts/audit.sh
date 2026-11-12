#!/usr/bin/env bash
# Stress/regression audit for CPU and memory: drives the release binary through
# scenarios that have caused problems before (huge responses, big bodies with undo,
# many drafts, tab switching) and reports RSS, CPU time and file sizes per step.
# It also samples CPU every 2 s and fails if postui looks stuck in a busy loop.
#
# Like scripts/bench.sh it uses a throwaway HOME, a synthetic workspace and a local
# `python3 -m http.server`, so it never touches your data or the network.
#
# Usage: scripts/audit.sh          (macOS/Linux; needs python3, ps, pgrep, script)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release --manifest-path "$ROOT/Cargo.toml" >/dev/null
BIN="$ROOT/target/release/postui"
WORK="$(mktemp -d)"
PORT=18766
SRV=""
cleanup() {
  [ -n "$SRV" ] && kill "$SRV" 2>/dev/null || true
  pkill -f "stty rows 50 cols 200; exec '$BIN'" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

case "$(uname)" in
  Darwin) DATA="$WORK/home/Library/Application Support/postui" ;;
  *)      DATA="$WORK/home/.local/share/postui" ;;
esac
mkdir -p "$DATA"

python3 -I - "$DATA" "$WORK" "$PORT" <<'PY'
import json, os, sys, uuid
data, work, port = sys.argv[1], sys.argv[2], sys.argv[3]
def req(name, url, body=""):
    return {"id": str(uuid.uuid4()), "name": name, "method": "Get", "url": url, "path_params": [],
            "headers": [], "body": body, "body_mode": "Raw", "form_fields": [], "binary_file": "",
            "pre_request_script": "", "post_response_script": ""}
big_body = json.dumps({"rows": [{"i": i, "v": "x" * 40} for i in range(3500)]}, indent=2)  # ~290 KB
cols = [{"id": str(uuid.uuid4()), "name": f"c{c}",
         "requests": [req(f"r{c}-{r}", f"http://x/{r}") for r in range(35)]} for c in range(20)]
small = req("small", f"http://127.0.0.1:{port}/small.json")
big = req("big response", f"http://127.0.0.1:{port}/big.json")
oneline = req("one-line response", f"http://127.0.0.1:{port}/oneline.txt")
bodies = [req(f"big body {i}", f"http://127.0.0.1:{port}/small.json", big_body) for i in range(4)]
cols.append({"id": str(uuid.uuid4()), "name": "audit", "requests": [small, big, oneline] + bodies})
slots = {"9": small["id"], "7": big["id"], "6": oneline["id"]}
slots.update({str(i + 1): b["id"] for i, b in enumerate(bodies)})          # space 1..4
ws = {"collections": cols, "environments": [], "active_environment": None, "recent_requests": [],
      "open_tabs": [small["id"], big["id"], oneline["id"]] + [b["id"] for b in bodies],
      "quick_slots": slots}
json.dump(ws, open(os.path.join(data, "workspace.json"), "w"))
items = [{"id": i, "name": f"item {i}", "tags": ["a", "b"], "y": "lorem ipsum dolor sit amet " * 3}
         for i in range(28000)]
json.dump({"items": items}, open(os.path.join(work, "big.json"), "w"))                 # ~5 MB, one line
open(os.path.join(work, "oneline.txt"), "w").write("z" * 900_000)                    # 900 KB, one line
json.dump({"ok": True}, open(os.path.join(work, "small.json"), "w"))
PY

(cd "$WORK" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) & SRV=$!
disown "$SRV"
LOG="$WORK/log"; CPULOG="$WORK/cpu"; : > "$LOG"; : > "$CPULOG"

driver() {
  sleep 0.5
  local pid; pid=$(pgrep -n -f "$BIN")
  cpu() { ps -o time= -p "$pid" | awk -F'[:.]' '{printf "%.2f", ($1*60+$2)+("0."$3)}'; }
  rss() { ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}'; }
  dsz() { [ -f "$DATA/drafts.json" ] && awk -v b="$(wc -c < "$DATA/drafts.json")" 'BEGIN{printf "%.1f MB", b/1048576}' || echo "-"; }
  k() { printf "$1"; sleep "${2:-0.3}"; }
  M=0
  mark() { M=$(cpu); }
  rep() { printf "%-46s RSS %4s MB  CPU %5.2fs  drafts.json %s\n" "$1" "$(rss)" "$(awk -v a="$M" -v b="$(cpu)" 'BEGIN{print b-a}')" "$(dsz)" >> "$LOG"; }
  edit_body() { k ' ' 0.1; k "$1" 0.8; k ' ' 0.1; k 'b' 0.4; for _ in $(seq 1 "$2"); do k 'i' 0.05; k 'x' 0.05; k '\033' 0.15; done; k '\033' 0.3; }

  sleep 2.5; mark;                                                      rep "startup (708 requests)"
  mark; k ' ' 0.2; k '7' 0.6; k '\033[15~' 4;                           rep "5 MB single-line JSON response"
  mark; k ' ' 0.2; k '6' 0.6; k '\033[15~' 3;                           rep "900 KB single-line text response"
  mark; for _ in $(seq 1 20); do k 'j' 0.2; k 'k' 0.2; done; sleep 1.5; rep "40 navigation keys"
  mark; for _ in $(seq 1 5); do k ' ' 0.1; k '9' 0.5; k ' ' 0.1; k '7' 0.8; done; sleep 1.5; rep "10 tab switches incl. the 5 MB one"
  mark; edit_body 1 30;                                                 rep "30 edits in a 290 KB body (undo)"
  mark; edit_body 2 10; edit_body 3 10; edit_body 4 10;                 rep "3 more 290 KB bodies edited (drafts)"
  mark; for _ in $(seq 1 20); do k 'j' 0.2; k 'k' 0.2; done; sleep 1.5; rep "40 navigation keys with 4 big drafts"
  k ' ' 0.1; k '7' 0.6
  mark; for _ in $(seq 1 8); do k '\033[15~' 2.5; done; sleep 2;       rep "8 more 5 MB responses"
  mark; sleep 10;                                                       rep "idle 10 s"
  k 'q' 1
}

# Watchdog: if postui hangs, kill it so the script still ends (and fails).
( sleep 180; pkill -f "stty rows 50 cols 200; exec '$BIN'" && echo "WATCHDOG: postui had to be killed" >> "$LOG" ) & WD=$!
disown "$WD"   # no "Terminated" job notices when we kill these helpers
( while sleep 2; do p=$(pgrep -n -f "$BIN") || continue; ps -o %cpu= -p "$p" >> "$CPULOG" 2>/dev/null; done ) & MON=$!
disown "$MON"

RUN="stty rows 50 cols 200; exec '$BIN'"
in_pty() { if [ "$(uname)" = Darwin ]; then script -q /dev/null sh -c "$RUN"; else script -q -c "$RUN" /dev/null; fi; }
HOME="$WORK/home" XDG_DATA_HOME= driver | HOME="$WORK/home" XDG_DATA_HOME= in_pty >/dev/null 2>&1 || true
kill "$WD" "$MON" 2>/dev/null || true

printf "postui %s on %s\n\n" "$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo '?')" "$(uname -sm)"
cat "$LOG"
echo
for f in workspace.json drafts.json responses.jsonl history.json; do
  [ -f "$DATA/$f" ] && awk -v f="$f" -v b="$(wc -c < "$DATA/$f")" 'BEGIN{printf "  %-16s %6.1f MB\n", f, b/1048576}'
done
busy=$(awk '$1 > 50 {n++} END {print n+0}' "$CPULOG")
echo
echo "CPU samples over 50%: $busy of $(wc -l < "$CPULOG" | tr -d ' ')"
if grep -q WATCHDOG "$LOG" || [ "$busy" -gt 3 ]; then
  echo "FAIL: postui looks stuck or busy"; exit 1
fi
echo "OK"
