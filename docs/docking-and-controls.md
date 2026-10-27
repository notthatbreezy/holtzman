# Docking instructions and binding access

The server-side binding-name allow-list has been removed at the Captain's request.
All keyboard-bound actions in the active Elite preset can be sent. Legacy
`pressable` entries are accepted for compatibility but ignored and no longer
written into new configs. A real keyboard mapping and a supported key remain
technical prerequisites; this is not an extra approval system.

`list_bindings` discovers exact configured action names and key chords. `status`
also includes a compact menu-binding vocabulary when the preset can be read.
`FocusLeftPanel` is an Elite action ID; `UI panel left` and `PanelLeft` are not.
Invalid names are resolved before focusing the game or pressing a key.

A current docking order authorizes the menu steps needed to request clearance;
HOLT should not ask for another approval on the Request Docking button. The
[procedure](../guides/docking.md) covers observed Contacts navigation and grant
verification. It is a built-in MCP guide after updating the server, and is also
served by the local HOLT client as `read_guide(name="docking")` for older servers.

The [draft playbook payload](../examples/playbooks/request-docking.json) is ready
for installation. It deliberately does not claim a successful live docking test.
After the server update, this command on ARCHIE installs it without replacing an
existing agent-edited playbook:

```powershell
.\.local\holt\voice-env\Scripts\python.exe -m client.seed_playbooks
```

## SKIPPY deployment

The observed running server did not advertise playbook tools or binding discovery.
Updating the local client does not replace that remote binary. On SKIPPY, from its
Holtzman checkout:

```powershell
git pull --ff-only
powershell -ExecutionPolicy Bypass -File scripts\update-holtzman-server.ps1
```

Then reconnect HOLT on ARCHIE and run the seed command above. This activates the
removed allow-list, actual binding discovery, and durable MCP playbooks together.
The server updater builds first and leaves the previous service running on build
failure. No credentials need to be copied into chat.

## Checks

An authenticated HTTP regression uses a dry-run keyboard preset to discover and
press both FocusLeftPanel and a keyboard action outside a legacy allow-list.
`python -m client.check_docking` is a small paid GPT-6 Luna workflow check with
simulated observations and simulated controls only; it never controls the game.
The draft remains unverified for live cockpit variations until a real session
confirms them.


After updating, `holtzman --version` and the MCP `status.server_build` field show
the compiled commit (or `unknown` when Git metadata was unavailable at build time).
A checkout's `git log` alone does not identify an already-running installed binary.
The fresh pre-update endpoint advertised 19 tools and none of the three playbook
tools, even though the reported source revision e03ffa6 includes them.
