# Repository review providers

The daemon routes hosted-review queries to optional executable packages. For this
API, hosting calls, authentication flow, host URL parsing, pagination, and
native-state mapping belong to the package. The engine does not link these
package implementations. Forge work IDs are independent of hosted-review
identities. GitHub publication from Forge sharing and chat Git uses the package
write contract. Git pushes and local workspace custody stay in Forge. The older
GitLab CLI handoff is preserved until a GitLab package replaces it.

## Discovery and lifecycle

An installed executable named `medousa_review_<id>` (or `.exe`) is discovered in
this order: beside the running daemon, then `{dataDir}/bin`. No provider is
started at daemon boot. `--describe` writes one JSON `ReviewProviderDescriptor`
to stdout and exits without authentication or network access. `--invoke` reads
one `ReviewProviderInvocation` JSON document from stdin, writes one
`ReviewProviderResponse`, and exits. `--mutate` accepts a
`ReviewProviderWriteInvocation`; writes are not accepted by `--invoke`. `--auth`
accepts a private `ReviewProviderAuthInvocation` and returns a private
`ReviewProviderAuthPackageResponse`. These auth IPC types are never HTTP schemas.
Logs belong on stderr. Both directions use
protocol version 1 from `medousa-types::review_provider`.

Discovery admits filesystem work through `ForgeExecutionService`, caps the
catalog at 32 packages, limits each descriptor to 32 KiB and three seconds, and
caches discovery for 30 seconds. Listing providers refreshes the catalog. Review
calls use the existing execution admission and process supervision with a
90-second deadline, an 8 MiB output limit, and process-tree cleanup. Failures and
unsupported protocol versions are reported; no invocation is retried automatically.

## HTTP contract

- `GET /v1/reviews/providers`: installed descriptors and discovery problems.
- `POST /v1/reviews/query`: `ReviewProviderQuery` → `ReviewProviderResponse` (`content.read`).
- `POST /v1/reviews/mutate`: `ReviewProviderMutation` → `ReviewProviderResponse` (`content.write`).
- `POST /v1/reviews/auth`: `ReviewProviderAuthRequest` → `ReviewProviderAuthResponse` (`admin.runtime`).

Provider discovery requires `content.read` on the connected workshop. Query is a
read-only POST to carry typed operation inputs. Its response has
`protocol_version` and `outcome`: either `{status:"success", result:…}` or
`{status:"error", error:{code,message}}`. Inspect the outcome even on HTTP 200.
Transport authentication failures use the usual HTTP errors. See
[the generated contract](../../sdk-contract/openapi.json) for the DTO schemas.

A query supplies `provider_id`, `host`, optional `connection_id`, and `request`.
The tagged request operations are `connection_status`, `resolve_repository`, `inbox`, `list`, `resolve`, `get`,
`files`, `diff`, `checks`, `check_details`, and `discussion`. Packages advertise implemented
operations. Cursors are opaque to callers. Follow returned cursors explicitly;
checks and discussions can span multiple native collections. The write operation
`publish` takes repository, head/base branches, title, body, draft, and
`update_existing`. It looks up an open review with the same head/base first.
It reuses that review, or updates its title/body when requested. It never pushes
Git, merges a review, or automatically retries a failed create request.

`HostedReviewRef` contains provider, host, repository, and opaque native ID.
Callers additionally scope account-sensitive state by connection identity. File
and diff queries carry the displayed head revision; a provider reports
`stale_revision` if it cannot serve that revision. Check queries address the
immutable commit. Inline discussion entries retain stable native IDs, path,
line/side, revision, and parent comment when supplied by the provider.

An explicit integration connection must match provider and host. Only configured
OAuth/API-key slots are read, on demand. Credentials travel through package
stdin, never argv or public responses. Missing explicit credentials are errors;
they do not fall back to another account. With no connection selected, a package
may reuse its existing sign-in. Metadata discovery never reads credentials.

## First package: GitHub

`review-github` installs `medousa_review_github`, built from the independent
`medousa-github` crate. It uses native HTTP for explicitly selected OAuth/API-token connections, without
requiring GitHub CLI. With no connection selected it uses non-interactive `gh api`
and the workshop's existing CLI sign-in. Both paths have bounded output and
timeouts. It supports github.com and explicit GitHub Enterprise hosts. CLI lookup
uses PATH and common macOS installation locations, including Homebrew.

### Native account authorization

The package advertises `device_auth` and `credential_refresh`. Medousa starts
sign-in from **Settings → Accounts**, displays the short user code, and opens
GitHub in the system browser. Home polls automatically at the provider's interval.
Cancel, reopen, and retry are available in Settings. Reviews contains no sign-in
or OAuth configuration controls.

The native bridge pins begin/poll/cancel to the workshop where sign-in started,
even if the active workshop changes. It opens the package's verification address;
the webview cannot submit arbitrary auth requests through the generic dispatcher.
Only public user codes and account metadata reach the webview. The private device
code and OAuth tokens stay with the package and workshop.

The generic auth API accepts `begin {provider_id,host,client_id?}`, `poll {flow_id}`,
and `cancel {flow_id}`. The GitHub package includes Medousa's public OAuth client
ID (`Ov23lisduhkFdBIix9ON`); normal users supply no registration, callback, client
ID, or secret. The package talks directly to GitHub's device endpoints. There is
no hosted Medousa auth service or browser callback listener.

For package maintainers: enable device flow on the GitHub app registration. GitHub's
registration form still asks for a callback URL; a loopback value such as
`http://127.0.0.1/oauth/callback` can be registered, but device flow never calls it.
No server needs to listen there. Enterprise/custom distributions can override the
public client ID through `MEDOUSA_GITHUB_CLIENT_ID` at runtime/build time (or via
the headless SDK request) and configure `MEDOUSA_GITHUB_HOST`; this is outside the
normal Settings experience. Device flow requires no client secret.

The daemon retains private pending state for at most 15 minutes, limits concurrent
flows to 16, and enforces poll intervals, including `slow_down`. Restarting the
workshop discards unfinished login flows. Another account's saved token is never
overwritten by sign-in.

On completion, the daemon saves a new connection's OAuth bundle through
`medousa-secrets` (keychain/keyring), then publishes its metadata and connection
ID. A storage failure retains the completed private bundle for a completion retry,
without redeeming the device code twice. Successful completion is idempotent
for the remaining flow lifetime: retries return the same connection ID from a
bounded metadata cache, with the private token released from the flow. Credentials
never reach Home. Discovery and review account-list metadata do not read keychain;
review packages are not started during daemon boot. Explicit account requests
serialize token load/rotation/save per connection. Expiring tokens refresh on
demand through package IPC; rotated bundles are persisted before use. A failed
rotation save keeps the new bundle in a bounded in-memory retry slot instead of
redeeming the old refresh token again. The public API sees only connection
metadata or sanitized errors.

To build another provider, implement the shared DTO protocol in a separate
`medousa_review_*` executable. Installing it in the workshop's bin directory is
sufficient for discovery; no provider-name match is needed in the daemon or UI.


### Account inbox and hosted writes

`inbox` takes an account-relative `folder` (`authored`, `review_requested`, or
`participating`) and an opaque cursor. It uses the selected connection, never a
username supplied by Home. Inbox rows keep hosted identities and never create
Forge tasks. The GitHub package uses account-relative PR search (up to 1,000
results); Home retains at most 500 metadata rows per current queue.

The separate mutation contract supports `comment` (optionally replying to an
opaque inline comment ID), `submit_review` with typed decision and inline
comments, `edit`, `set_state`, `request_reviewers`, `merge`, `rerun_check`, and
`cancel_check`, alongside `publish`. Packages advertise each operation; the
daemon validates provider/host identity and dispatches the declared operation.
It does not interpret provider routes or permissions. Successful actions return
`updated` with a human-readable confirmation; API rejections remain typed errors.

`submit_review`, `merge`, and CI actions carry the displayed head revision. The
GitHub package verifies the head before writes, includes `commit_id` for submitted
reviews and `sha` for merges, and verifies workflow/check ownership by head SHA.
Inline comments use path, old/new side, and line rather than provider positions.
No automatic write retries occur after ambiguous transport failures. A user's
explicit retry can create duplicate comments; refresh and inspect the discussion
first. Branch protection enforcement remains with the host.

Hosted review metadata includes available action names, requested reviewers,
merge state, and enabled merge methods. These are UI affordances, not an
authorization substitute. `check_details` returns bounded Markdown output and
annotations or job steps; full provider logs/artifacts remain external links.
GitHub-specific permissions, REST/GraphQL calls, search qualifiers, CI resource
IDs, and projections live entirely in `medousa-github`.
