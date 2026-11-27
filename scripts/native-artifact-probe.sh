#!/bin/sh
# Physical mouse diagnostic for the native artifact view. Never stops the app.
set -eu
umask 077
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
BASE="$ROOT/.tmp/artifact-probes"
CURRENT="$BASE/current"
BIN="$ROOT/../.cache/cargo-target/release/medousa-native"

session() {
    [ -f "$CURRENT" ] || { echo 'No probe session. Run start first.' >&2; exit 1; }
    DIR=$(cat "$CURRENT")
    case "$DIR" in "$BASE"/*) ;; *) echo 'Invalid probe session path.' >&2; exit 1 ;; esac
    [ -d "$DIR" ] || { echo 'Probe directory is missing.' >&2; exit 1; }
}
case "${1:-help}" in
    build)
        cd "$ROOT"
        # Match the fixed binary path below even when Cargo env overrides exist.
        cargo build --manifest-path apps/medousa-native/Cargo.toml --release \
            --target-dir "$ROOT/../.cache/cargo-target"
        ;;
    start)
        [ "$(uname -s)" = Darwin ] || { echo 'This probe requires macOS.' >&2; exit 1; }
        if pgrep -x medousa-native >/dev/null; then
            echo 'Quit the existing Medousa native app normally, then run start again.' >&2
            echo 'The probe will not close your app or open a competing workspace.' >&2
            exit 1
        fi
        [ -x "$BIN" ] || { echo 'Run build first.' >&2; exit 1; }
        mkdir -p "$BASE"
        DIR=$(mktemp -d "$BASE/$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX")
        printf '%s\n' "$DIR" > "$CURRENT.tmp.$$"
        mv "$CURRENT.tmp.$$" "$CURRENT"
        printf 'Probe directory: %s\n' "$DIR"
        echo 'Open your artifact and scroll normally, then try fast jumps/reversals.'
        echo 'After a flash, press Cmd+Shift+M in the artifact window to mark it.'
        echo 'When finished, run this script with stop in another terminal before quitting.'
        echo 'This terminal remains attached; app output goes to console.log.'
        cd "$ROOT"
        exec env RNDR_ARTIFACT_DIAGNOSTICS="$DIR" "$BIN" > "$DIR/console.log" 2>&1
        ;;
    mark)
        session
        touch "$DIR/diagnostic.marker"
        printf 'Requested marker in %s\n' "$DIR"
        ;;
    stop)
        session
        touch "$DIR/diagnostic.stop"
        # A flushed summary is the retrieval barrier; process exit can lose the
        # buffered tail. Never kill the app just to stop diagnostic collection.
        tries=0
        while [ "$tries" -lt 5 ]; do
            if grep -q '"kind":"summary"' "$DIR"/artifact-*.jsonl 2>/dev/null; then
                printf 'Logging stopped and summary flushed. App keeps running. Logs: %s\n' "$DIR"
                exit 0
            fi
            tries=$((tries + 1))
            sleep 1
        done
        printf 'Stop requested, but no flushed summary found. Check logs: %s\n' "$DIR" >&2
        echo 'Do not treat missing or incomplete telemetry as a clean result.' >&2
        exit 1
        ;;
    status)
        session
        printf 'Probe directory: %s\n' "$DIR"
        ls -lh "$DIR"
        echo 'Open an artifact to initialize its diagnostic log. No JSONL means not yet initialized or unavailable.'
        ;;
    *)
        echo 'Usage: scripts/native-artifact-probe.sh build|start|mark|stop|status'
        echo 'Build first, quit the existing native app normally, then start.'
        echo 'Logging is opt-in, bounded, and does not change scrolling.'
        ;;
esac
