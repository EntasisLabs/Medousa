# Dots

Medousa can reach your OpenAI dot through Slack and show its replies in Chat.
The workshop that owns the conversation must run the Slack adapter. Your dot's
computer can access workshop content separately through a scoped API token.

1. In your dot's ChatGPT profile, add Slack. Add your dot and the Medousa Slack
   app to a dedicated Slack channel. Install the Slack adapter from
   **Settings → Packages** on the connected workshop. Configure the Medousa
   app's Socket Mode, `message.channels` event subscription, bot token, app
   token, and your own Slack member ID in **Settings → Sharing → Channels**.
2. Create a Slack user token for the dot's owner with `chat:write` access. The
   owner must be a member of that channel. Medousa needs this token because a
   dot normally takes direction from its owner. Keep this token private.
3. In **Settings → External Agents → Dots**, choose **Add session**. Enter a
   label, dedicated channel ID (`C…` or `G…`), the dot's member or bot ID
   (`U…`, `W…`, or `B…`), and the owner's user token (`xoxp-…`). These values are visible in
   Slack's channel and member details. Medousa stores the token in the workshop
   credential store, not the transcript.
4. Open **Sessions → Connected agents** and choose the named Dots session on
   desktop or iPhone. Send a harmless test request and confirm that it appears
   in Slack and the dot's reply appears in Medousa. A Slack send receipt alone
   does not prove the dot received or acted on a message.

The dot and Medousa Slack app must remain in the channel for replies to arrive.
Use a separate channel for each dot; the adapter reserves that channel and
records replies only from the configured dot member. If your phone is connected
to a remote workshop, choose that workshop in Medousa; the phone's embedded
Personal workshop has its own conversation list and Slack configuration.

## Give the dot workshop access

Under the saved session, create an API token with **Read** and optionally
**Work** permissions and an expiry of 1–90 days. Give the token to your dot
through its private computer configuration, along with an HTTPS address that
forwards to the workshop. The token is shown once. It cannot administer the
workshop, manage credentials, or read other conversation transcripts. Work
runs under the conversation owner's workshop policy. Replace or revoke the
token in the same settings card.

On the dot's computer, the Medousa CLI can read notes with the scoped token.
Configure the values privately rather than putting them in Slack:

```sh
medousa-cli daemon-agent-api GET /v1/vault/notes
```

Set `MEDOUSA_API_URL` to the workshop's HTTPS gateway and
`MEDOUSA_API_TOKEN` to the token shown in Medousa. For a work request, use
`medousa-cli daemon-agent-api POST /v1/jobs/ask --body-json '{...}'`. The CLI
accepts only GET or POST and uses this token only for the requested route;
the workshop enforces the token's scope. A loopback HTTP URL works when the
CLI runs on the workshop itself.

See [Instinct Agent](instinct-agent.md) for the other scoped routes and HTTPS
gateway requirements. A CLI paired to a workshop has its own credentials and
permissions; the Slack identity does not grant filesystem access.

OpenAI documents [Slack messaging for dots](https://learn.chatgpt.com/docs/dots/channels)
and [computer access](https://learn.chatgpt.com/docs/dots/computers-and-apps)
as separate connections.

## Coordinate durable work

The saved API token can also use the [work participant API](../engine/external-conversations.md#work-participant-adapters).
Read access can inspect your work graph, units, and coordination status. Work
access can record work intent, propose native execution against a visible governed
project chat, and register executor/reviewer handoffs. Native execution retains
its existing approval requirements. The originating chat can close while the
runtime coordinates approved work; this does not enable automatic follow-ups or
turn provider messages into qualified review verdicts.
