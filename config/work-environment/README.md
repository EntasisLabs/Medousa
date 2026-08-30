# Medousa development environment image

This is the bootstrap image for daemon-owned work environments. It contains no
Medousa daemon or durable state. Derived images install the user's selected
Node, Rust, Python, system, and global toolchains here rather than on the daemon
host.

The runtime executes only immutable references:

```text
ghcr.io/entasislabs/medousa-work-environment@sha256:<digest>
```

The publication workflow builds Linux `amd64` and `arm64` variants and reports
the manifest digest. A local runtime can be pinned for conformance testing:

```bash
MEDOUSA_OCI_RUNTIME=podman \
MEDOUSA_TEST_OCI_IMAGE=ghcr.io/entasislabs/medousa-work-environment \
MEDOUSA_TEST_OCI_DIGEST=<sha256 hex> \
MEDOUSA_TEST_OCI_PLATFORM=linux/arm64 \
cargo test -p medousa --features full-daemon \
  daemon::work_environment_host::tests::oci_lifecycle_exec_and_restart_reconcile \
  -- --ignored --exact
```

Project dependency caches may be attached later as disposable OCI volumes.
Source, checkpoints, artifacts, and image digests remain authoritative outside
the container lifecycle.
