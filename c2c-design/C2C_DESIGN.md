# Claude2Claude (C2C) — Design Document

## Overview

C2C is a lightweight message-passing system that allows Claude Code instances running on different machines to communicate with each other. One instance can ask a question, and another instance — with full access to its local system — can answer it.

The system is deliberately simple: a stateless MCP server backed by SQLite, acting as a shared postbox. Messages go in, messages get collected. There are no sessions, no presence tracking, no heartbeats.

## Architecture

```
┌─────────────────────┐         ┌─────────────────────┐
│  Linux Machine      │         │  Windows Machine     │
│                     │         │                      │
│  Claude Code ───────┼────┐    │  Claude Code ────────┤
│  (MCP client)       │    │    │  (MCP client)        │
│                     │    │    │                      │
└─────────────────────┘    │    └──────────┬───────────┘
                           │               │
                           ▼               ▼
                    ┌─────────────────────────┐
                    │  C2C MCP Server          │
                    │  (Rust binary on Linux)  │
                    │                          │
                    │  SQLite message store    │
                    │  Listens on network      │
                    └─────────────────────────┘
```

- The C2C server is a single Rust binary running on the Linux machine.
- It implements the MCP protocol (JSON-RPC over stdio for local, SSE/HTTP for remote).
- Both Claude Code instances connect to it as an MCP server.
- The Linux instance connects locally (stdio or localhost).
- The Windows instance connects over the network (HTTP/SSE to the Linux box's IP).

## Peer Identity

- Peers self-declare their name on each tool call (a `from` parameter).
- No registration, no authentication (for now).
- The `/c2c` slash command on each Claude Code client handles setting up the peer name automatically — the user doesn't type it each time.
- The server doesn't track "online" status. A peer exists only in the context of messages it has sent or received.

## MCP Tool Definitions

The server exposes five tools:

### `send_message`

Send a message to another peer.

**Parameters:**
| Name    | Type   | Required | Description                         |
|---------|--------|----------|-------------------------------------|
| `from`  | string | yes      | Sender's peer name                  |
| `to`    | string | yes      | Recipient's peer name               |
| `body`  | string | yes      | The message content (natural language, freeform) |

**Returns:**
```json
{
  "message_id": "uuid-string",
  "status": "pending",
  "created_at": "2026-03-21T14:30:00Z"
}
```

### `check_messages`

Check for messages addressed to you.

**Parameters:**
| Name     | Type   | Required | Description                              |
|----------|--------|----------|------------------------------------------|
| `for`    | string | yes      | Your peer name                           |
| `status` | string | no       | Filter: `"pending"`, `"answered"`, `"all"`. Default: `"pending"` |

**Returns:**
```json
{
  "messages": [
    {
      "message_id": "uuid-string",
      "from": "linux-box",
      "to": "windows-box",
      "body": "What are the network adapter details including MAC addresses?",
      "status": "pending",
      "created_at": "2026-03-21T14:30:00Z"
    }
  ]
}
```

### `reply_to_message`

Reply to a specific message.

**Parameters:**
| Name         | Type   | Required | Description                    |
|--------------|--------|----------|--------------------------------|
| `from`       | string | yes      | Your peer name                 |
| `message_id` | string | yes      | The message being replied to   |
| `body`       | string | yes      | The reply content              |

**Returns:**
```json
{
  "message_id": "uuid-of-original",
  "status": "answered",
  "reply_body": "The machine has two NICs: ...",
  "replied_at": "2026-03-21T14:35:00Z"
}
```

### `list_messages`

Browse message history. Useful for context or reviewing past exchanges.

**Parameters:**
| Name    | Type    | Required | Description                                     |
|---------|---------|----------|-------------------------------------------------|
| `from`  | string  | no       | Filter by sender                                |
| `to`    | string  | no       | Filter by recipient                             |
| `limit` | integer | no       | Max messages to return. Default: 20             |

**Returns:**
Array of message objects (same shape as `check_messages` results, plus any replies).

### `purge_messages`

Clean up old messages.

**Parameters:**
| Name         | Type    | Required | Description                                        |
|--------------|---------|----------|----------------------------------------------------|
| `older_than` | integer | no       | Delete messages older than N hours. Default: 72     |
| `status`     | string  | no       | Only purge messages with this status. Default: all  |

**Returns:**
```json
{
  "deleted_count": 14
}
```

## Message Schema (SQLite)

```sql
CREATE TABLE messages (
    id          TEXT PRIMARY KEY,   -- UUID
    from_peer   TEXT NOT NULL,
    to_peer     TEXT NOT NULL,
    body        TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending',  -- 'pending' | 'answered'
    reply_body  TEXT,
    created_at  TEXT NOT NULL,      -- ISO 8601
    replied_at  TEXT                -- ISO 8601, NULL until answered
);

CREATE INDEX idx_to_status ON messages(to_peer, status);
CREATE INDEX idx_created ON messages(created_at);
```

That's it. One table.

## Message Lifecycle

1. **Created** — `send_message` inserts a row with `status = 'pending'`.
2. **Answered** — `reply_to_message` sets `status = 'answered'`, fills `reply_body` and `replied_at`.
3. **Purged** — `purge_messages` deletes rows by age and/or status.

Messages have no TTL enforced automatically. The `purge_messages` tool is called manually or could be run on a cron if desired.

## Transport

The MCP server must support two transport modes:

1. **Stdio** — for the local Claude Code instance on the Linux box. Claude Code spawns the binary and communicates over stdin/stdout. This is the standard MCP local transport.

2. **HTTP + SSE** — for remote Claude Code instances (the Windows box). The server listens on a configurable port. Claude Code on Windows is configured with the server's URL.

Both transports speak the same MCP JSON-RPC protocol.

## Configuration

### Server config (`c2c-server.toml`)

```toml
[server]
listen_addr = "0.0.0.0"
listen_port = 9229
db_path = "/var/lib/c2c/messages.db"

# Optional shared secret for basic auth
# token = "some-shared-secret"
```

### Claude Code MCP config (Linux — local stdio)

```json
{
  "mcpServers": {
    "c2c": {
      "command": "/usr/local/bin/c2c-server",
      "args": ["--mode", "stdio"]
    }
  }
}
```

### Claude Code MCP config (Windows — remote HTTP)

```json
{
  "mcpServers": {
    "c2c": {
      "url": "http://linux-box:9229/mcp"
    }
  }
}
```

## The `/c2c` Slash Command

This is a Claude Code slash command (client-side) that:

1. Sets the peer name for the session (from hostname, config, or user input).
2. Reminds Claude Code what tools are available and how to use them.
3. Optionally checks for pending messages immediately.

The slash command is **not** part of the server — it's a Claude Code custom command on each machine. The design doc doesn't specify its implementation beyond the interface contract: after `/c2c` runs, Claude Code knows its peer name and uses `from` consistently.

## Deployment

### Build

```bash
cargo build --release
```

Produces a single binary: `c2c-server`.

### Install

```bash
sudo cp target/release/c2c-server /usr/local/bin/
sudo mkdir -p /var/lib/c2c
```

### Systemd service (`/etc/systemd/system/c2c.service`)

```ini
[Unit]
Description=C2C MCP Message Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/c2c-server --mode http --config /etc/c2c/c2c-server.toml
Restart=on-failure
User=c2c

[Install]
WantedBy=multi-user.target
```

### Networking

The Windows machine needs to reach the Linux box on the configured port. Options:

- Direct LAN access (if both on same network)
- Tailscale (if across networks — recommended)
- WireGuard or other VPN

No TLS in v1 — rely on the tunnel/VPN for encryption if crossing networks.

## Typical Workflow

**On the Linux box:**
```
You:    /c2c
CC:     Connected as "linux-box". No pending messages.

You:    Ask windows-box what its network adapter details are,
        including MAC addresses and driver versions.
CC:     [calls send_message(from="linux-box", to="windows-box", body="...")]
        Message sent. ID: abc-123.
```

**On the Windows box (whenever you get to it):**
```
You:    /c2c
CC:     Connected as "windows-box". Checking messages...
        1 pending message from linux-box:
        "What are the network adapter details including MAC
         addresses and driver versions?"
        
        Let me look that up...
        [runs Get-NetAdapter, Get-NetAdapterAdvancedProperty, etc.]
        
        [calls reply_to_message(from="windows-box", message_id="abc-123", body="...")]
        Reply sent.
```

**Back on the Linux box:**
```
You:    Check for c2c replies
CC:     [calls check_messages(for="linux-box", status="answered")]
        Reply from windows-box:
        "The machine has two NICs: 
         1. Intel I219-V - MAC: AA:BB:CC:DD:EE:FF - Driver: 12.19.1.37
         2. Realtek RTL8125 - MAC: 11:22:33:44:55:66 - Driver: 10.53.0321.2022"
```

## Future Considerations

These are explicitly **not in v1** but worth noting:

- **Authentication** — shared token or mTLS for multi-user environments.
- **File attachments** — allow sending files alongside messages (base64 in body for now if needed).
- **Broadcast** — send a message to all peers, not just one.
- **Auto-polling daemon** — a watcher on the worker side that picks up and answers messages without human intervention. This is where it gets really interesting but also where you need to be careful about unattended Claude Code sessions.
- **Message threading** — follow-up questions on the same topic.
- **Encryption at rest** — encrypt the SQLite DB or message bodies.
- **Web UI** — a simple dashboard to see message status without being in a Claude Code session.
