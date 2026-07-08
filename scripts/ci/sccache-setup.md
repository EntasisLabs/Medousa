# Shared compile cache for the self-hosted release fleet (sccache)

The `release.yml` workflow runs on self-hosted runners. Each machine already keeps
a warm `CARGO_TARGET_DIR` (`github.workspace/.cache/cargo-target`) between runs, so
per-machine incremental builds are effectively free. `sccache` adds a *shared*
compile cache across the whole fleet, which is most valuable for:

- the 3 i7 mini-PCs (if they PXE-boot diskless, they do not persist a target dir);
- spreading cache hits from one platform's build to the others.

This is opt-in: nothing changes until you set the repo variable and install sccache.

## 1. Enable it in the workflow

The workflow already wires this up, gated on a repo variable:

```yaml
# .github/workflows/release.yml (env:)
RUSTC_WRAPPER: ${{ vars.MEDOUSA_SCCACHE || '' }}
CARGO_INCREMENTAL: ${{ vars.MEDOUSA_SCCACHE != '' && '0' || '' }}
```

Set the variable to turn it on (Settings -> Secrets and variables -> Actions -> Variables):

```
MEDOUSA_SCCACHE = sccache
```

Leave it unset to keep builds on the warm target dir only (default, always safe).

## 2. Install sccache on every runner

```bash
# Linux / macOS
cargo install sccache --locked
# or: brew install sccache   (macOS)
#     winget install Mozilla.sccache   (Windows)
```

`sccache` must be on the runner service account's PATH (the account the GitHub
Actions runner runs as), not just your interactive shell.

## 3. Pick a shared backend on the home server

Configure the `SCCACHE_*` env on each runner (e.g. in the runner's `.env`, a
`runsvc` environment file, or the machine's environment). Choose ONE backend:

### Option A: Redis on the home server (simple, fast on LAN)

```bash
# On the home server:
docker run -d --name sccache-redis -p 6379:6379 redis:7

# On each runner:
export SCCACHE_REDIS="redis://home-server.lan:6379"
```

### Option B: WebDAV (nginx/caddy on the home server)

```bash
export SCCACHE_WEBDAV_ENDPOINT="http://home-server.lan:8080/sccache"
# optional auth:
export SCCACHE_WEBDAV_USERNAME="medousa"
export SCCACHE_WEBDAV_PASSWORD="..."
```

### Option C: S3-compatible (reuse Cloudflare R2)

```bash
export SCCACHE_BUCKET="medousa-sccache"
export SCCACHE_ENDPOINT="https://<accountid>.r2.cloudflarestorage.com"
export SCCACHE_REGION="auto"
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
```

Redis or WebDAV over the LAN is recommended over R2 for latency.

## 4. Diskless PXE mini-PCs: persist the caches over NFS

If the mini-PCs netboot diskless, mount a persistent NFS export from the home
server so the target dir and/or local sccache survive reboots:

```bash
# /etc/fstab on each diskless node (example)
home-server.lan:/export/medousa-cache  /mnt/medousa-cache  nfs  defaults,noatime  0  0
```

Then point the runner at it. Either:

- set the GitHub Actions runner's work directory onto the NFS mount so
  `CARGO_TARGET_DIR` (under `github.workspace`) persists, or
- keep sccache local-disk-less by using a network backend from step 3 (preferred
  for diskless nodes, since NFS-backed incremental target dirs can be slow).

## 5. Verify

On a runner, after a build:

```bash
sccache --show-stats
```

You should see rising "Cache hits" across runs and across machines sharing the
backend.

## Runner label reference

Registered runners must carry these labels (see the header comment in
`.github/workflows/release.yml`):

| Machine            | Labels                                       |
| ------------------ | -------------------------------------------- |
| Apple Silicon Mac  | `self-hosted, macOS, ARM64`                  |
| Intel Mac          | `self-hosted, macOS, X64`                    |
| Windows            | `self-hosted, Windows, X64` (git-bash on PATH) |
| Linux / mini-PCs   | `self-hosted, Linux, X64`                    |
| Home server        | `self-hosted, Linux, X64, medousa-publish`   |
