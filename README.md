# SQLAero

SQLAero is being rebuilt as a native Rust MySQL client for Python. The first
implementation slice establishes the shared core, Python extension and package
installation path. Connection and query APIs are not yet available.

Development follows [the project instructions](AGENTS.md), the confirmed
[specification](docs/connection-reconstruction-research.md), and the local
[execution plan](tasks/rust-sync-kernel/plan.md). The frozen `experimental` and
`reference` directories are development references, not runtime dependencies or
distribution contents.
