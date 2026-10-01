# Instinct Agent

Instinct can chat through WhatsApp and call your workshop through a short-lived
Node.js client over Urspace, or ordinary HTTPS requests. The workshop owns its
files, tools, and long-running work.

## Connect WhatsApp

1. Open **Settings → External Agents → Instinct Agent → Add session**.
2. Link the workshop's WhatsApp adapter if it is not already connected.
3. Enter a session name and Instinct's international phone number, including
   `+` and the country code, then choose **Connect**.
4. Open **Sessions → Connected agents** and choose the named Instinct session
   on desktop or mobile.

Replies belong to this workshop's transcript. When WhatsApp supplies a linked
ID instead of a phone number, the adapter uses WhatsApp's phone mapping to route
the reply. A transport acknowledgement alone does not prove Instinct received
the message; verify a round trip in your session.

## Give Instinct API access

Under the saved session, select permissions and an expiry of 1–90 days, then
choose **Create API token**. Store the token in Instinct's private credential
configuration. Medousa shows it once and stores only its hash.

- **Read:** list, read, and search workshop notes; list tags and calendar events;
  read the capability catalog.
- **Work:** start background work and read workshop job results and reports.
  Work runs under the conversation owner's profile and existing workshop policy.

These grants apply to the workshop, not to a single note or project. They do
not grant administrative access, credential management, direct shell API calls,
or access to other conversation transcripts. Work can use the tools permitted
by the workshop; it is not a read-only permission.

**Replace API token** immediately invalidates the previous token. **Revoke API
token**, expiry, or removing the session prevents further requests. Revocation
does not cancel work already admitted by the workshop.

## Node 22 through Urspace

The [JavaScript example](../../examples/instinct-urspace/client.mjs) uses the
Urspace SDK to carry the same API requests over Iroh. On the workshop machine,
configure an Urspace host that proxies the daemon API and preserves the
`Authorization` header. A Medousa Iroh ticket is not an Urspace invitation; the
protocols are different.

In Instinct's Node 22 environment:

```sh
cd examples/instinct-urspace
npm ci --ignore-scripts
node client.mjs GET /v1/capabilities
node client.mjs POST /v1/jobs/ask < request.json
```

Set `URSPACE_INVITE_URL` and `MEDOUSA_API_TOKEN` through private environment
configuration, never command arguments. The first connection saves a private
resume grant and key in `~/.config/medousa-instinct/session.json` (mode `0600`).
Later invocations resume that identity; they do not need the original invite.
Override the path with `MEDOUSA_URSPACE_SESSION_FILE` for another workshop.
Failed resumes do not silently create a new identity. Requests have a 45-second
deadline and no automatic retries or LAN fallback.

The example pins `@urspace/client` 0.5.0 and works around its web-initializer vs
bundler-loader packaging mismatch by loading the SDK's own WASM and glue.
Node 22 initialization and request construction are tested; a live Urspace host
and invitation are still needed to verify the full connection.

## HTTPS address (curl alternative)

Use an HTTPS gateway that forwards API methods, paths, query strings, request
bodies, and the `Authorization` header to the workshop. Disable caching of API
responses and do not log credentials. Keep the daemon's administrative routes
out of the gateway's public route allowlist.

An Iroh ticket alone is not an HTTPS URL. Urspace's current invite links load
a browser client that opens Iroh using JavaScript/WASM; its Cloudflare Worker
does not proxy app requests. Plain `curl` cannot use that browser bootstrap.
Using Iroh here requires a server-side HTTPS-to-Iroh gateway, or a conventional
HTTPS tunnel to the workshop. See [Urspace's edge documentation](https://github.com/EntasisLabs/urspace/blob/main/deploy/cloudflare-worker/README.md).

## Curl examples

Set `MEDOUSA_API_URL` to that gateway's HTTPS base URL and `MEDOUSA_API_TOKEN`
through Instinct's private environment. A read request:

```sh
curl --fail-with-body --max-time 20 \
  -H "Authorization: Bearer $MEDOUSA_API_TOKEN" \
  "$MEDOUSA_API_URL/v1/vault/notes"
```

Start work with a short request; the daemon continues after curl exits:

```sh
curl --fail-with-body --max-time 20 \
  -H "Authorization: Bearer $MEDOUSA_API_TOKEN" \
  -H 'Content-Type: application/json' \
  --data '{"prompt":"Summarize the notes for my current project."}' \
  "$MEDOUSA_API_URL/v1/jobs/ask"
```

Save the returned `job_id`, then check it in a later invocation:

```sh
curl --fail-with-body --max-time 20 \
  -H "Authorization: Bearer $MEDOUSA_API_TOKEN" \
  "$MEDOUSA_API_URL/v1/jobs/$JOB_ID/result"
```

Do not automatically repeat a work submission after a timeout: it may already
have been accepted. Check the workshop's job history first. A 401 means the
token is invalid, expired, or revoked. A 403 means its scope does not permit the
operation. API access is separate from WhatsApp; the token never needs to be
sent in a WhatsApp message.
