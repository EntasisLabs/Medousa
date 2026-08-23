# Give an agent a credential without putting it in chat

When an isolated task needs an API key or token, Medousa can show a **Secure
credential handoff** card below the conversation. Paste the value into that
card—not into the message composer.

## What happens

1. The agent requests a named credential and explains why it needs it.
2. Medousa shows the destination runtime, credential name, reason, and any
   approved HTTPS hosts in a trusted app card.
3. You approve the handoff or choose **Deny**.
4. The workshop daemon receives the value through a dedicated native endpoint.
   The agent receives only an opaque, short-lived, one-use grant.
5. OpenShell or Grapheme uses the credential behind its own capability
   boundary; neither returns the value to the agent.

The value is not added to the user message, assistant transcript, turn stream,
tool result, Grapheme source or state, or durable job payload.

## OpenShell handoff

For an OpenShell request, choose **Store & continue**. The workshop daemon
sends the value directly to its OpenShell gateway, which encrypts provider
credentials in its default credential store. The sandbox sees a placeholder;
the gateway proxy substitutes the real value only for endpoints bound to that
provider and allowed by sandbox network policy.

This path fails closed unless Providers v2 is enabled. Before showing the card,
Medousa verifies that the credential key belongs to the requested provider
profile and that the profile declares at least one destination endpoint.

## Grapheme handoff

For a native Grapheme request, choose **Authorize & continue**. The card lists
the exact HTTPS hosts that may receive the credential. Medousa holds the value
in zeroizing daemon memory for one inline Grapheme run, then drops it. It is not
saved as a Grapheme script, template, or recurring workflow secret.

Inside the run, `secrets.get_secret_handle` exchanges the opaque grant for an
opaque run handle. The script can use that handle to:

- create an HMAC-SHA256 signature with `secrets.sign_request`; or
- make an authenticated HTTPS request with `medousa.authorized_http`.

Authenticated requests are limited to the exact hosts shown on the card,
redirects are disabled, and the credential is attached by Medousa after the
script crosses the host capability boundary. If the card lists no hosts, the
credential can be used for signing but not HTTP.

## Before you approve

Check these details on the card:

- the reason matches the task you asked for;
- the environment key is the one the service expects, such as
  `GITHUB_TOKEN`;
- the destination runtime and provider match the service;
- for Grapheme, every approved HTTPS host is one you expect the service to use.

If anything looks wrong, choose **Deny** and ask Medousa to explain or retry.
The request also expires if you leave it unanswered.

## Local and remote workshops

Credential authority follows the workshop daemon. With a local workshop, the
handoff occurs on this computer. With a paired or remote workshop, the value is
sent over the authenticated Medousa connection to that workshop—not stored in a
second local copy.

OpenShell providers remain on their workshop gateway so later authorized work
can use them. Grapheme handoffs are ephemeral and expire after one run. The
agent never receives either value. OpenShell provider listing, rotation, and
removal currently use the workshop's management tools.

## If no secure card appears

Do not paste the credential into chat as a fallback. Secure handoff requires a
trusted Medousa app surface. OpenShell requests additionally require a healthy
OpenShell package, gateway, and `providers_v2_enabled`; install or repair it
from **Settings → Packages**. A Grapheme request must be an inline, one-off run
and must name its approved HTTPS hosts before the card appears.
