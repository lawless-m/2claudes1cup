# C2C Design Package — Contents

## Files

| File | Description |
|------|-------------|
| `C2C_DESIGN.md` | The full design document. **Start here.** |
| `CONTENTS.md` | This file. |

## What is this?

Design specification for **Claude2Claude (C2C)** — a stateless MCP server that lets Claude Code instances on different machines exchange messages. One instance asks a question, another answers it, using a shared SQLite-backed postbox running on the Linux server.

## For Claude Code

This is a Rust project. The design doc contains:

- Architecture overview and diagrams
- All five MCP tool definitions with parameter schemas and return types
- SQLite schema
- Transport requirements (stdio for local, HTTP+SSE for remote)
- Config file format
- Systemd service definition
- Example workflow showing the full send/check/reply cycle

Start by reading `C2C_DESIGN.md` end to end, then begin with the Cargo project structure.
