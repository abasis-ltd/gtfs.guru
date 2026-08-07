# GTFS Guru Profile

[![Crates.io](https://img.shields.io/crates/v/gtfs-guru-profile.svg)](https://crates.io/crates/gtfs-guru-profile)

Deterministic, model-friendly facts derived from a parsed GTFS feed, for
[GTFS Guru](https://github.com/abasis-ltd/gtfs.guru). The types in this
crate deliberately contain no generated prose and make no provider-specific
LLM calls — the CLI, MCP server, web API, and any hosted product can share
the same computed facts and explanations without risking different
calculations between them.

## Installation

```bash
cargo add gtfs-guru-profile
```

## License

Apache-2.0
