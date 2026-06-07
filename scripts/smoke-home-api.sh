#!/usr/bin/env bash
set -euo pipefail

DAEMON_URL="${MEDOUSA_DAEMON_URL:-http://127.0.0.1:7419}"
TOKEN="$(uuidgen 2>/dev/null || echo "smoke-$(date +%s)")"
NOTE_PATH="journal/smoke-${TOKEN}.md"

echo "smoke-home-api: daemon=${DAEMON_URL}"

curl -fsS "${DAEMON_URL}/health" >/dev/null

curl -fsS -X POST "${DAEMON_URL}/v1/vault/notes" \
  -H 'content-type: application/json' \
  -d "{\"path\":\"${NOTE_PATH}\",\"content\":\"# Smoke ${TOKEN}\\n\\nmedousa smoke token ${TOKEN}\\n\"}" \
  >/dev/null

curl -fsS "${DAEMON_URL}/v1/vault/notes/${NOTE_PATH}" | grep -q "${TOKEN}"
curl -fsS "${DAEMON_URL}/v1/vault/search?q=${TOKEN}&limit=5" | grep -q '"hits"'
curl -fsS "${DAEMON_URL}/v1/workspace/snapshot" | grep -q 'workspace_revision'
curl -fsS "${DAEMON_URL}/v1/manuscripts?limit=5" | grep -q '"manuscripts"'
curl -fsS "${DAEMON_URL}/v1/capabilities" | grep -q '"capabilities"'

curl -fsS -X DELETE "${DAEMON_URL}/v1/vault/notes/${NOTE_PATH}" >/dev/null

echo "smoke-home-api: ok"
