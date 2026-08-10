# GTFS Guru MCP

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru-mcp.svg)](https://crates.io/crates/gtfs-guru-mcp)

A read-only, validation-focused [MCP](https://modelcontextprotocol.io) server
for [GTFS Guru](https://github.com/abasis-ltd/gtfs.guru), letting an LLM
client validate GTFS feeds and inspect the resulting notices, profile facts,
and explanations directly.

## Installation

```bash
cargo install gtfs-guru-mcp
```

## Usage

```bash
gtfs-guru-mcp --transport stdio --allow-dir /path/to/gtfs/feeds
```

Point your MCP client (Claude Desktop, Claude Code, etc.) at the resulting
stdio server, or run `--transport http` with `--bind` for HTTP access
(requires a bearer token). Run `gtfs-guru-mcp --help` for the full option
list.

## License

Apache-2.0
