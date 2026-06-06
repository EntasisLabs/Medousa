# Medousa BYOC OpenShell sandbox image (H6 — grapheme --version in sandbox child).
#
# Build from repo root:
#   podman build -t medousa-openshell-sandbox:local -f config/openshell-sandbox/Dockerfile .
#
# H6 smoke:
#   openshell sandbox create --from medousa-openshell-sandbox:local \
#     --policy ~/.config/medousa/openshell-policies/skill-sandbox.yaml
#   openshell sandbox exec -n <name> -- grapheme --version
#
# H7 smoke (after skill-import echo-skill):
#   medousa openshell-probe echo-skill

FROM rust:1.85-bookworm AS grapheme-builder
RUN cargo install --git https://github.com/entasislabs/grapheme.git grapheme-cli --bin grapheme --locked

FROM ghcr.io/nvidia/openshell-community/sandboxes/base:latest

USER root

RUN mkdir -p /sandbox/workspace /etc/medousa/policy \
    && chown -R sandbox:sandbox /sandbox/workspace

COPY --from=grapheme-builder /usr/local/cargo/bin/grapheme /usr/local/bin/grapheme
COPY config/openshell-policies/ /etc/medousa/policy/

USER sandbox
WORKDIR /sandbox
