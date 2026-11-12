#!/usr/bin/env bash
# Measure postui's memory (RSS) and CPU through a scripted session.
#
# Runs the release binary in a pseudo-terminal (via `script`) against a throwaway
# HOME with a synthetic workspace (700 requests in 20 collections), sends keys, and
# samples the process with `ps`. A local `python3 -m http.server` serves a ~5 MB
# JSON file so "send" doesn't touch the network.
#
# Usage: scripts/bench.sh          (macOS/Linux; needs python3, ps, script)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release --manifest-path "$ROOT/Cargo.toml" >/dev/null
BIN="$ROOT/target/release/postui"
WORK="$(mktemp -d)"
trap 'kill "${SRV:-}" 2>/dev/null || true; rm -rf "$WORK"' EXIT
PORT=18765

case "$(uname)" in
  Darwin) DATA="$WORK/home/Library/Application Support/postui" ;;
  *)      DATA="$WORK/home/.local/share/postui" ;;
esac
mkdir -p "$DATA"

python3 -I - "$DATA" "$WORK" "$PORT" <<'PY'
import json, os, sys, uuid
data, work, port = sys.argv[1], sys.argv[2], sys.argv[3]
def req(name, url):
    return {"id": str(uuid.uuid4()), "name": name, "method": "Get", "url": url, "path_params": [],
            "headers": [{"key": "Accept", "value": "application/json", "enabled": True}],
            "body": json.dumps({"example": True, "n": 1}, indent=2), "body_mode": "Raw",
            "form_fields": [], "binary_file": "", "pre_request_script": "", "post_response_script": ""}
cols = [{"id": str(uuid.uuid4()), "name": f"collection {c}",
         "requests": [req(f"request {c}-{r}", f"{{{{baseUrl}}}}/c{c}/r{r}") for r in range(35)]}
        for c in range(20)]
bench = req("bench big json", f"http://127.0.0.1:{port}/big.json")
cols.append({"id": str(uuid.uuid4()), "name": "bench", "requests": [bench]})
tabs = [r["id"] for r in cols[0]["requests"][:5]] + [bench["id"]]
ws = {"collections": cols, "environments": [], "active_environment": None,
      "recent_requests": [], "open_tabs": tabs, "quick_slots": {"9": bench["id"]}}
json.dump(ws, open(os.path.join(data, "workspace.json"), "w"))
items = [{"id": i, "name": f"item {i}", "tags": ["a", "b", "c"],
          "nested": {"x": i * 1.5, "y": "lorem ipsum dolor sit amet " * 3}} for i in range(28000)]
json.dump({"items": items}, open(os.path.join(work, "big.json"), "w"))
PY

(cd "$WORK" && exec python3 -m http.server "$PORT" --bind 127.0.0.1 >/dev/null 2>&1) & SRV=$!
LOG="$WORK/samples.txt"; : > "$LOG"

driver() {
  sleep 0.5
  local pid
  pid=$(pgrep -n -f "$BIN")
  cpu() { ps -o time= -p "$pid" | awk -F'[:.]' '{printf "%.2f", ($1*60+$2)+("0."$3)}'; }
  rss() { ps -o rss= -p "$pid" | awk '{printf "%.1f", $1/1024}'; }
  sample() { printf "%-34s RSS %6s MB   CPU time %5ss\n" "$1" "$(rss)" "$(cpu)" >> "$LOG"; }
  idle() {
    local c0 c1; c0=$(cpu); sleep 10; c1=$(cpu); sample "$1"
    awk -v a="$c0" -v b="$c1" 'BEGIN{printf "  -> %.2f%% of one core while idle\n", (b-a)*10}' >> "$LOG"
  }
  k() { printf "$1"; sleep "${2:-0.3}"; }
  sleep 2.5;                       sample "startup (701 requests)"
  idle "idle 10 s"
  k ' ' 0.2; k 'f' 0.5; k 'request' 0.5; k '\033' 0.4; k '\033' 0.4; sample "collection browser + fuzzy search"
  for _ in $(seq 1 20); do k 'L' 0.15; done; sleep 1; sample "switch tabs x20 (Shift+L)"
  k ' ' 0.2; k '9' 1;              sample "open bench request (space 9)"
  k '\033[15~' 4;                  sample "send -> ~5 MB JSON response"
  k '\033[15~' 4;                  sample "send again"
  k ' ' 0.2; k 'r' 0.5; k 'G' 0.8; k 'g' 0.1; k 'g' 0.8; k '/item 27999\r' 1.5; k '\033' 0.4; k '\033' 0.4
  sample "scroll + search the response"
  idle "idle 10 s after"
  k 'q' 1
}

RUN="stty rows 50 cols 200; exec '$BIN'"
in_pty() {  # BSD (macOS) and util-linux `script` take the command differently
  if [ "$(uname)" = Darwin ]; then script -q /dev/null sh -c "$RUN"; else script -q -c "$RUN" /dev/null; fi
}
HOME="$WORK/home" XDG_DATA_HOME= driver | HOME="$WORK/home" XDG_DATA_HOME= in_pty >/dev/null 2>&1
printf "postui %s on %s\n\n" "$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo '?')" "$(uname -sm)"
cat "$LOG"
