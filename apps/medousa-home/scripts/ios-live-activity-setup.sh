#!/usr/bin/env bash
# Deprecated alias — use ios-prepare.sh (runs automatically via npm run tauri:ios:dev).
exec "$(dirname "$0")/ios-prepare.sh"
