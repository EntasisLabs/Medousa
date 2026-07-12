# Security Policy

## Supported versions

Security fixes target the **latest release** on the default branch and current
stable release channels. Older tags may not receive backports.

## Reporting a vulnerability

**Do not open a public GitHub issue for security reports.**

Email **security@entasislabs.com** with:

- A short description of the issue and impact
- Steps to reproduce (or a proof of concept)
- Affected version / commit if known
- Whether you plan to disclose publicly and on what timeline

We aim to acknowledge reports within **3 business days** and will keep you
updated while we investigate.

## Scope (examples)

In scope:

- Remote code execution, auth bypass, or data exfiltration via the engine HTTP API,
  pairing / Iroh transport, or the Medousa desktop/mobile apps
- Privilege escalation across workshops, profiles, or vault roots
- Secrets leakage (API keys, pairing tokens) via logs or unintended surfaces

Out of scope (unless chained into a real exploit):

- Denial of service against a local-only loopback daemon
- Issues that require physical access or an already-compromised host
- Social engineering of operators

## Safe harbor

We will not pursue legal action against researchers who:

- Make a good-faith effort to avoid privacy violations and destruction of data
- Do not access or modify data that is not their own beyond what is needed to
  demonstrate the issue
- Report findings promptly and keep them private until we confirm a fix or
  mutually agree on disclosure

Thank you for helping keep Medousa users safe.
