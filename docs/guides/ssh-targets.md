# Connect a server over SSH

SSH targets let Medousa work with servers without installing Medousa on them.
The connected workshop opens SSH connections; it must be able to reach the
server and authenticate as the account you choose.

## Add a server

1. Open **Settings → Connection → SSH targets → Add server**.
2. Enter a name, hostname or IP address, port, and SSH username.
3. Enter the absolute private-key path **on the workshop**, or leave it blank
   to use that workshop process's SSH agent. Keys remain on the workshop.
   An encrypted key must already be unlocked in its SSH agent; password login
   and password entry in chat are not supported.
4. Choose whether Medousa agents can use the server.
5. Select **Check server identity**. Compare the displayed fingerprints with
   the server's host keys, then select **Trust this server identity** and save.
   Checking retrieves public host keys; it does not authenticate your account.
6. Select **Test** to check authentication. On desktop, **Terminal** opens the
   same saved connection in Medousa's existing terminal view. SSH terminals
   use the saved server access independently of coding projects; they do not
   require an undertaking or a sealed Forge environment.

The workshop needs `ssh` and `ssh-keyscan` available on its PATH. Targets use
explicit addresses and pinned host keys. Existing SSH config aliases,
ProxyCommand/ProxyJump rules, and port forwards are not imported in this version.
If the server's host key changes, the connection fails rather than silently
trusting the replacement. Remove and add the connection after verifying its new
identity. The first version supports up to 64 saved targets per workshop.

## Ask Medousa to work with the server

For example: “Check why the backup failed on Homelab.” Agents discover only SSH
targets enabled for their owning user. No coding project is required. The
runtime resolves the saved target and authentication settings; agents do not
supply new addresses, keys, or access grants.

Remote commands have durable receipts with status, bounded stdout/stderr, exit
code, and an execution ID. The runtime starts commands once for an exact
request key and polls the receipt afterward. Closing the app does not cancel
workshop-owned commands or terminals. Receipts persist after a workshop restart,
but an unfinished operation becomes **unknown**: SSH loss or timeout does not
prove that the remote command stopped. Medousa must inspect remote state before
issuing new changes; it does not automatically rerun the old command.

A terminal is interactive; it does not provide a reliable completion receipt for
each line typed. Tracked commands are preferable for agent work. Remote file
paths in a hosted SSH terminal do not open workshop-local files. SSH sessions do
not provide the Code view, remote file browsing, or remote job recovery across
an SSH disconnect.

Turn off **Agent access** to block new agent operations and terminal input.
Commands already dispatched may finish, and an operator's open terminal remains
usable. Removing the target prevents further connections; it does not undo
remote work or revoke the server account's SSH key. Setup requires workshop
operator access. A paired phone can manage targets through its connected
workshop with that access; opening a terminal view is desktop-only in this version.
