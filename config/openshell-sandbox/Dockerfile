# Medousa BYOC OpenShell sandbox image (Sprint B6).
#
# Build (requires Docker/Podman and network to ghcr.io):
#   podman build -t medousa-openshell-sandbox:local -f config/openshell-sandbox/Dockerfile .
#
# Use with OpenShell:
#   openshell sandbox create --from medousa-openshell-sandbox:local --policy ~/.config/medousa/openshell-policies/research-readonly.yaml
#
# Extend this image to install Grapheme CLI, medousa binaries, and imported skill assets.

FROM ghcr.io/nvidia/openshell-community/sandboxes/base:latest

USER root

RUN mkdir -p /sandbox/workspace /etc/medousa/policy \
    && chown -R sandbox:sandbox /sandbox/workspace

# Optional: copy repo policy templates into the image for offline sandboxes.
COPY config/openshell-policies/ /etc/medousa/policy/

USER sandbox
WORKDIR /sandbox

# Default child command is supplied by openshell.sandbox.run / cognition_openshell_sandbox_run.
