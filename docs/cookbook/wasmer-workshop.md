# Wasmer workshop guest

**Audience:** Portal operators and anyone hosting a Medousa workshop with `wasmer run`.

Portal stays a process supervisor. It starts one Wasmer command and kills that process on release. The program inside that command is **`medousa-workshop`**, built in this repo. Point `MEDOUSA_WASMER_PACKAGE` at the built `.wasm` file (or at the directory that contains `wasmer.toml` after the build script runs).

This guest is a WASIX command module. It is the workshop process Portal acquires. It is not a second copy of the browser Personal workshop, and it does not replace `medousa_daemon`.

| Runtime | What it is | `wasmer run` |
|---------|------------|--------------|
| `medousa_daemon` | The workshop brain: HTTP API, pairing, Iroh mesh, vault, turns | No. Native host process |
| Browser Personal workshop (`medousa_browser`, `wasm32-unknown-unknown`) | Personal workshop in a tab via wasm-bindgen | No. Imports `__wbindgen_placeholder__` |
| Grapheme module | Script the daemon loads in its own Wasmer host | No. The daemon is the host |
| `medousa-workshop` | Foreground guest Portal supervises | Yes |

## Build

Install [Rust](https://rustup.rs/), [Wasmer 7.x](https://docs.wasmer.io/runtime/cli/), and [`cargo-wasix`](https://wasix.org/docs/language-guide/rust/installation/). WASIX is required because the guest opens a TCP socket to the daemon. `wasm32-unknown-unknown` and plain `wasm32-wasip1` do not provide that socket API.

From the repo root:

```bash
bash scripts/build-workshop-wasm.sh
```

That writes:

```text
crates/medousa-workshop/dist/workshop.wasm
```

Copy `workshop.wasm` onto the host that runs Wasmer, for example `/opt/medousa/workshop.wasm`, in place of a sleep stub. The same directory’s `wasmer.toml` names the unpublished package `medousa/workshop`. Publishing that name to the Wasmer registry is optional and is not required for `wasmer run` of the local file.

Check the guest without Wasmer:

```bash
cargo test -p medousa-workshop
bash scripts/ci/smoke-workshop-guest.sh
```

The smoke script runs the crate tests, builds the wasm when `cargo-wasix` is installed, and, when `wasmer` is on `PATH`, runs the local file against a loopback `/health` server. It does not contact wasmer.io (`wasmer run --offline`).

## Run

The guest reads its job from the environment, prints one JSON line to stdout, flushes, and sleeps until the process is killed. It does not read stdin and it does not prompt. There is no double-fork.

```bash
wasmer run --net --no-tty --offline \
  --env MEDOUSA_SANDBOX_ID=sbx-1 \
  --env MEDOUSA_KIND=workshop \
  --env MEDOUSA_CONNECT_URL=http://10.0.0.8:7419 \
  --env MEDOUSA_VCPU_MILLI=1000 \
  --env MEDOUSA_MEMORY_MIB=512 \
  crates/medousa-workshop/dist/workshop.wasm
```

`--net` lets the guest open TCP to the daemon. Port 7419 is not a port-80 HTTP proxy, so `--http-client` is not a substitute. With `--net`, the guest uses the host network stack: a daemon listening on the host’s `127.0.0.1:7419` is reachable at that URL. Without `--net`, Wasmer denies the socket and, on a terminal, may prompt for access; the ready line then reports `connection_failed`. `--no-tty` keeps Wasmer from attaching a terminal. `--offline` resolves a local file without the registry. Leave threads enabled (the Wasmer 7 default). The module’s memory is shared, and `--disable-threads` will not instantiate it.

A successful attach prints one line and keeps running:

```json
{"event":"workshop.ready","package":"medousa-workshop","version":"0.11.0","sandboxId":"sbx-1","kind":"workshop","shape":{"vcpuMilli":1000,"memoryMib":512},"connect":{"mode":"http_health","ok":true,"url":"http://10.0.0.8:7419","httpStatus":200,"apiVersion":"v1","heartbeat":"skipped"},"mesh":{"inGuest":false,"owner":"host_daemon"}}
```

`MEDOUSA_WORKSHOP_ONCE=1` prints that line and exits. Use it for smoke tests. Portal should leave it unset so acquire keeps a process that release can kill.

Exit codes:

| Code | Meaning |
|------|---------|
| 0 | The guest started. With `MEDOUSA_WORKSHOP_ONCE`, it has already exited. Otherwise it stays resident |
| 1 | `MEDOUSA_WORKSHOP_REQUIRE_CONNECT=1` and the connect attempt did not succeed |
| 2 | The Portal environment is invalid (`workshop.error` on stdout) |

A daemon that is down is not a contract error. The ready line has `connect.ok: false` and the process still stays up, so Portal has something to kill. Set `MEDOUSA_WORKSHOP_REQUIRE_CONNECT=1` when acquire should fail closed.

## Portal

Portal shells out to:

```text
wasmer run [--env MEDOUSA_SANDBOX_ID=…] [--env MEDOUSA_CONNECT_URL=…] [--env MEDOUSA_KIND=…] [--env MEDOUSA_VCPU_MILLI=…] [--env MEDOUSA_MEMORY_MIB=…] <MEDOUSA_WASMER_ARGS…> <MEDOUSA_WASMER_PACKAGE>
```

`MEDOUSA_WASMER_ARGS` is split on whitespace and placed before the package. Recommended values for this guest:

```text
MEDOUSA_WASMER_PACKAGE=/opt/medousa/workshop.wasm
MEDOUSA_WASMER_ARGS=--net --no-tty
```

`MEDOUSA_WASMER_PACKAGE` and `MEDOUSA_WASMER_ARGS` are host settings. The guest does not read them.

| Portal env | Guest behavior |
|------------|----------------|
| `MEDOUSA_SANDBOX_ID` | Copied into `sandboxId`. Empty means the field is null |
| `MEDOUSA_KIND` | Must be `workshop` when set. Unset defaults to `workshop` and sets `kindDefaulted: true` |
| `MEDOUSA_CONNECT_URL` | `http://host:port` origin, `https://` origin, or `medousa://` invite. No userinfo, query, fragment, or path |
| `MEDOUSA_VCPU_MILLI` | Positive integer copied into `shape.vcpuMilli`. The guest does not apply a cgroup; Wasmer owns the quota. The resident loop sleeps so it does not spin |
| `MEDOUSA_MEMORY_MIB` | Positive integer copied into `shape.memoryMib`. Same ownership as vCPU |
| `MEDOUSA_SESSION_TOKEN` | Optional. Not part of Portal’s current command. When set, sent only on `GET /pair/heartbeat` |
| `MEDOUSA_WORKSHOP_ONCE` | `1` / `true` exits after the ready line |
| `MEDOUSA_WORKSHOP_REQUIRE_CONNECT` | `1` / `true` exits 1 unless the connect attempt succeeded |

`--net` uses the host network stack. `MEDOUSA_CONNECT_URL` must be an address that host can already route, including the host’s own loopback when the daemon is bound there.

## What connect actually does

`mesh.inGuest` is always `false` and `mesh.owner` is `host_daemon`.

**HTTP origin.** The guest dials TCP and sends:

1. `GET /health` with no `Authorization` header. This is the daemon’s anonymous liveness document, `{"status":"ok","apiVersion":"v1"}`. A bearer is not sent here: the daemon treats a bad `Authorization` value as a failure even on `/health`.
2. When `MEDOUSA_SESSION_TOKEN` is set, `GET /pair/heartbeat` with `Authorization: Bearer <token>`. That is the same authenticated liveness paired portals already call. The guest does not `POST` `mesh_lan_base_url` or `mesh_iroh_ticket`, because this process cannot accept a mesh dial-back.

**HTTPS origin.** Reported as `connect.mode: "https_unsupported"`. The guest does not terminate TLS. Put the daemon’s HTTP listener on the sandbox network, or terminate TLS in front of it on the host.

**`medousa://` invite.** Reported as `connect.mode: "invite_host_side"` and `reason: "iroh_host_side"`. The invite is not copied into the ready line. Completing the Ed25519 ceremony and dialing `medousa-http/1` needs Iroh. Iroh’s wasm target is the browser relay, not a Wasmer socket guest, and WASIX has no QUIC stack that can carry that session. Home and `medousa_daemon` keep that dial.

**Empty URL.** `connect.mode: "absent"`. The process is still a workshop guest; it has not joined a daemon.

Still host-side, on `medousa_daemon` or Home:

- Iroh endpoint, ticket dial, and relay
- mDNS Nearby discovery
- Pairing init / verify (the guest does not mint keys or consume a QR token)
- Vault, sessions, turns, Grapheme modules, Forge, and local inference
- Filesystem authority for the workshop (`workshop.kind === "local"` remains a co-located daemon, not this sandbox)

Shape hints are echoed so a supervisor can see what it requested. They do not resize the Wasmer instance.
