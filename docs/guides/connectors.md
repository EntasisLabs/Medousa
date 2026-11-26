# Connectors

A connector is a reviewed package: a remote, local, or channel server, how it signs in, which tools start hidden, and a short usage note. Medousa ships a starter catalog. Nothing is written to your MCP config until the server returns its first tool list.

Open **Settings → Packages** (or the MCP servers panel) and choose **Connectors**.

## What you can install

- **Remote MCP** is the default, including on a phone. Google Workspace uses Google's own servers at `*.googleapis.com/mcp/v1`. Notion, Linear, Figma, Atlassian, Stripe, and the other dynamic-registration vendors can sign in without a client id Medousa ships.
- **Bring your own client** when a vendor will not register Medousa for you (GitHub, Slack, Asana, HubSpot, Google). Paste the public client id. A client secret, if the vendor requires one, stays in the keyring.
- **Local command** runs only on a desktop. Phones cannot start a stdio process.
- **Channel** connectors, such as Telegram, store the bot token in the keyring. Starting the adapter process from that record is not wired yet.

## Sign-in

Desktop sign-in opens the system browser and returns through a local address on your computer. A phone uses the `medousa://oauth/callback` redirect with the system sign-in sheet (iOS) or the system browser (Android). Tokens are stored with the same secret service as the rest of Medousa: the iOS Keychain when the phone build is linked that way. Android does not yet have a Keystore backend in the keyring crate, so those tokens use the app's private data directory.

Google clients must be flagged **Use this client for an AI-powered agent** in Google Cloud. Medousa does not include a Google client id or secret. Request offline access so the session can refresh instead of asking you to sign in again every hour.

Sensitive confirmations from the server (for example, sending mail on Google's `/mcp/v1` endpoint) show up as a confirmation in Medousa. You can also answer from a terminal on the computer running the gateway:

```bash
medousa mcp elicitations
medousa mcp elicitation-respond <id> --action accept
```

## Scheduled runs

Scheduled and heartbeat runs still refuse MCP writes. That policy is unchanged.
