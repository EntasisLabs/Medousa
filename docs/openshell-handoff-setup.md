# OpenShell handoff setup (Medousa)

> **Audience:** Operators running Sprint B eval — especially **uv CLI-only** installs without Docker.  
> **Related:** [docs/internal/openshell-sprint-b-eval.md](../docs/internal/openshell-sprint-b-eval.md)

---

## What `uv tool install` gives you (and what it doesn't)

| Component | `uv tool install openshell` | Full `install.sh` (RPM/deb) |
|-----------|----------------------------|-----------------------------|
| `openshell` CLI | ✅ | ✅ |
| `openshell-gateway` daemon | ❌ | ✅ |
| `openshell-sandbox` supervisor | ❌ | ✅ |
| systemd user service | ❌ | ✅ auto-start |
| mTLS cert bootstrap | ❌ | ✅ on first start |

**Symptom:** `openshell status` → `Connection refused` on `http://127.0.0.1:8080` (or `:17670`).

**Fix:** Install gateway binaries separately and start the gateway yourself.

---

## Compute driver: Podman (no Docker)

OpenShell sandboxes need a container runtime. On Arch without Docker, use **rootless Podman** (OpenShell's Fedora/RHEL default).

```bash
# Arch
sudo pacman -S podman

# Rootless socket (gateway talks to this)
systemctl --user enable --now podman.socket

# Verify
podman info --format '{{.Host.CgroupsVersion}}'   # want 2
ls -la /run/user/$(id -u)/podman/podman.sock
```

If subuid/subgid are missing:

```bash
grep "$USER" /etc/subuid /etc/subgid
# If empty, Arch usually has these from package install; else:
# sudo usermod --add-subuids 100000-165535 --add-subgids 100000-165535 $USER
# then log out/in
```

---

## Install gateway binaries (Arch / uv path)

Pin version to match CLI (`openshell --version` → e.g. `0.0.57`).

```bash
VERSION=0.0.57
ARCH=x86_64-unknown-linux-gnu   # or aarch64-unknown-linux-gnu on ARM

mkdir -p ~/.local/openshell/bin
cd /tmp

curl -fsSL -o gateway.tgz \
  "https://github.com/NVIDIA/OpenShell/releases/download/v${VERSION}/openshell-gateway-${ARCH}.tar.gz"
curl -fsSL -o sandbox.tgz \
  "https://github.com/NVIDIA/OpenShell/releases/download/v${VERSION}/openshell-sandbox-${ARCH}.tar.gz"

tar -xzf gateway.tgz -C ~/.local/openshell/bin
tar -xzf sandbox.tgz -C ~/.local/openshell/bin
chmod +x ~/.local/openshell/bin/openshell-gateway ~/.local/openshell/bin/openshell-sandbox

export PATH="$HOME/.local/openshell/bin:$PATH"
```

---

## Gateway config (Podman + local dev TLS off)

Create `~/.config/openshell/gateway.toml`:

```toml
# Medousa local eval — Podman, no Docker
grpc_endpoint = "http://127.0.0.1:17670"
bind_address = "0.0.0.0:17670"

compute_drivers = ["podman"]

[openshell.drivers.podman]
socket_path = "/run/user/1000/podman/podman.sock"   # ← replace 1000 with $(id -u)
supervisor_bin = "/home/USER/.local/openshell/bin/openshell-sandbox"  # ← your path
image_pull_policy = "missing"

# Local single-player: simplify TLS for first eval
# (tighten before any shared/network exposure)
```

Generate the file with your uid/path:

```bash
mkdir -p ~/.config/openshell
cat > ~/.config/openshell/gateway.toml <<EOF
grpc_endpoint = "http://127.0.0.1:17670"
bind_address = "0.0.0.0:17670"
compute_drivers = ["podman"]

[openshell.drivers.podman]
socket_path = "/run/user/$(id -u)/podman/podman.sock"
supervisor_bin = "$HOME/.local/openshell/bin/openshell-sandbox"
image_pull_policy = "missing"
EOF
```

For first bring-up, NVIDIA's RPM path uses mTLS on `17670`. If the standalone binary defaults to TLS, either:

1. Follow [gateway config](https://docs.nvidia.com/openshell/latest/reference/gateway-config.html) to disable TLS for **localhost-only** eval, or  
2. Run `openshell-gateway` once and let it generate certs, then register with `--local` (see below).

---

## Start the gateway (uv does not auto-start)

**Foreground (debug):**

```bash
export PATH="$HOME/.local/openshell/bin:$PATH"
openshell-gateway --config ~/.config/openshell/gateway.toml
# or if binary reads env only:
OPENSHELL_CONFIG=~/.config/openshell/gateway.toml openshell-gateway
```

**Background user service (recommended):**

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/openshell-gateway.service <<EOF
[Unit]
Description=OpenShell gateway (Medousa eval)
After=podman.socket
Requires=podman.socket

[Service]
ExecStart=%h/.local/openshell/bin/openshell-gateway
Environment=OPENSHELL_CONFIG=%h/.config/openshell/gateway.toml
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now openshell-gateway
journalctl --user -u openshell-gateway -f
```

Keep running after logout (optional):

```bash
sudo loginctl enable-linger "$USER"
```

---

## Register CLI → gateway

After gateway is listening:

```bash
# If gateway uses plain HTTP on 17670 (eval only):
openshell gateway add http://127.0.0.1:17670 --local --name local

# If gateway uses mTLS (package default):
openshell gateway add https://127.0.0.1:17670 --local --name local

openshell gateway select local
openshell status
openshell doctor check
```

`openshell status` should show server reachable (no `Connection refused`).

---

## First sandbox

```bash
openshell sandbox create --from base
# or
openshell sandbox create -- claude   # needs ANTHROPIC_API_KEY provider
```

Policy demo (after clone):

```bash
git clone --depth 1 https://github.com/NVIDIA/OpenShell.git /tmp/OpenShell
bash /tmp/OpenShell/examples/sandbox-policy-quickstart/demo.sh
```

---

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|--------------|-----|
| `Connection refused` | Gateway not running (or crashed) | Start gateway (below); check it stays up |
| `--tls-cert is required` | Gateway defaults to TLS | Add `--disable-tls` for localhost eval |
| `Gateway 'local' already exists` | Prior `gateway add` succeeded | **Do not re-add.** Just start the gateway. Or `openshell gateway remove local` |
| Gateway exits immediately | Podman not installed / socket missing | Install podman + `podman.socket` **before** starting gateway |
| `passt` 404 on install | Bad mirror (e.g. omarchy) | See **Fix Podman install** below |
| `uv` only, no gateway binary | Expected with uv | Install gateway tarball (above) |
| Podman permission errors | Socket not up / subuid | `podman.socket`, relogin |
| Pull failures | No network to ghcr.io | `podman pull ghcr.io/nvidia/openshell-community/sandboxes/base:latest` |

### Fix Podman install (passt 404)

If `pacman -S podman` fails on `passt` from a stale mirror:

```bash
# Refresh mirrors, install passt first from any working mirror
sudo pacman -Sy
sudo pacman -S passt

# Then podman + socket
sudo pacman -S podman
systemctl --user enable --now podman.socket
podman info   # should work
ls /run/user/$(id -u)/podman/podman.sock
```

If `passt` still 404, install the package file directly from Arch (version may differ — check https://archlinux.org/packages/extra/x86_64/passt/):

```bash
cd /tmp
curl -fsSLO https://geo.mirror.pkgbuild.com/extra/os/x86_64/passt-2026_05_07.1afd4ed-1-x86_64.pkg.tar.zst
sudo pacman -U passt-*.pkg.tar.zst
sudo pacman -S podman
systemctl --user enable --now podman.socket
```

### Start gateway (matches your `local` registration on :8080)

You already registered:

```
local → http://127.0.0.1:8080 (plaintext)
```

**Do not run `gateway add` again.** Start the server on the same port:

```bash
export PATH="$HOME/.local/openshell/bin:$PATH"
export OPENSHELL_DOCKER_SUPERVISOR_BIN="$HOME/.local/openshell/bin/openshell-sandbox"

# Podman path (after podman.socket is active):
openshell-gateway \
  --disable-tls \
  --port 8080 \
  --bind-address 127.0.0.1 \
  --drivers podman

# Leave this terminal open, or use the systemd unit below with the same flags.
```

In another terminal:

```bash
openshell status          # should connect
openshell sandbox create --from base
```

**Important:** The gateway **exits** if the compute driver cannot connect (no podman socket). Installing podman is mandatory for the Podman path.

### Optional: Docker instead of Podman

If `docker ps` works on your machine, you can skip Podman and use:

```bash
openshell-gateway \
  --disable-tls \
  --port 8080 \
  --bind-address 127.0.0.1 \
  --drivers docker
```

Set `OPENSHELL_DOCKER_SUPERVISOR_BIN` to your `openshell-sandbox` path (same as above).

### systemd unit (port 8080, TLS off, Podman)

```bash
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/openshell-gateway.service <<EOF
[Unit]
Description=OpenShell gateway (Medousa eval)
After=podman.socket
Wants=podman.socket

[Service]
Environment=OPENSHELL_DOCKER_SUPERVISOR_BIN=%h/.local/openshell/bin/openshell-sandbox
ExecStart=%h/.local/openshell/bin/openshell-gateway --disable-tls --port 8080 --bind-address 127.0.0.1 --drivers podman
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now openshell-gateway
journalctl --user -u openshell-gateway -f
```

---

## Medousa doctor

`medousa doctor` includes an **OpenShell** section (Sprint B2):

| Field | Meaning |
|-------|---------|
| `openshell_gateway_url` | From `MEDOUSA_OPENSHELL_GATEWAY_URL`, OpenShell `active_gateway` metadata, or `http://127.0.0.1:8080` |
| `tcp_reachable` | TCP connect to gateway host:port |
| `readyz` | HTTP `GET /readyz` or `/healthz` succeeds |
| `cli` | `openshell --version` |
| `openshell_gateway_bin` / `sandbox_bin` | `~/.local/openshell/bin/` or `~/.local/bin/` |
| `podman_socket` / `podman_active` | User Podman socket for `--drivers podman` |
| `openshell_policies_dir` / `policy_count` | `~/.config/medousa/openshell-policies/` |

On first run, doctor seeds starter policies from `config/openshell-policies/` (e.g. `research-readonly.yaml`) if the user dir is empty.

Override gateway URL for Medousa probes:

```bash
export MEDOUSA_OPENSHELL_GATEWAY_URL=http://127.0.0.1:8080
medousa doctor
```

Future: sandbox smoke `create --from base` in CI.

---

## Manuscript `spec.openshell` (Sprint B5)

Worker manuscripts can declare OpenShell handoff defaults:

```yaml
spec:
  openshell:
    enabled: true
    policy_template: research-readonly   # file in ~/.config/medousa/openshell-policies/
    sandbox_from: base                   # or BYOC image ref / Dockerfile path
    allow_scheduled: false               # default — scheduled lane denies openshell tools
  tools:
    allow:
      - cognition_openshell_status
      - cognition_openshell_sandbox_run
```

Example specialty: `.medousa/manuscripts/openshell-researcher.yaml` (extends `base-researcher`).

Validate after seeding policies (`medousa doctor`):

```bash
medousa manuscript-validate openshell-researcher
```

**Cognition tools (worker lane):**

| Tool | Role |
|------|------|
| `cognition_openshell_status` | Gateway/policy probe (read-only) |
| `cognition_openshell_sandbox_run` | Enqueue `openshell.sandbox.run` (create → exec → destroy) |

Spawn a worker with `manuscript_id=openshell-researcher`, then call `cognition_openshell_sandbox_run` with `command` (string or argv array). Policy/sandbox defaults come from the manuscript when `manuscript_id` is set.

---

## BYOC sandbox image (Sprint B6)

Starter Dockerfile: `config/openshell-sandbox/Dockerfile`

```bash
podman build -t medousa-openshell-sandbox:local -f config/openshell-sandbox/Dockerfile .
openshell sandbox create --from medousa-openshell-sandbox:local \
  --policy ~/.config/medousa/openshell-policies/research-readonly.yaml
```

Extend the image to install Grapheme CLI, `medousa` binaries, and `skill-import` assets for H6–H7 validation.

---

## References

- OpenShell installation: https://docs.nvidia.com/openshell/latest/about/installation.html
- Gateway config (Podman): https://docs.nvidia.com/openshell/latest/reference/gateway-config.html
- Sprint B eval: [docs/internal/openshell-sprint-b-eval.md](../docs/internal/openshell-sprint-b-eval.md)
