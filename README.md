# AIPM — AI Plugin Manager

A production-grade package manager for AI plugin primitives (skills, agents, MCP servers, hooks). Think npm/Cargo, but purpose-built for the AI plugin ecosystem.

AIPM ships as **two Rust binaries**:

- **`aipm`** — consumer CLI: install, validate, doctor
- **`aipm-pack`** — author CLI: scaffold, pack, publish, yank

Both work across .NET, Python, Node.js, and Rust monorepos with no runtime dependency.

## Key Features

- **TOML manifest** (`aipm.toml`) — human-editable, AI-generation safe, no indentation traps
- **Content-addressable global store** — pnpm-inspired deduplication across projects
- **Strict dependency isolation** — only declared dependencies are accessible
- **Deterministic lockfile** (`aipm.lock`) — exact tree structure with integrity hashes
- **Semver dependency resolution** — backtracking solver with version unification
- **Registry model** — publish, install, yank, search, scoped packages
- **Workspace support** — workspace protocol, filtering, catalogs
- **Cross-platform** — Windows junction support, works in any monorepo

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) 1.70+

### Build from Source

```bash
cargo build --workspace
```

### Scaffold a New Plugin

```bash
aipm-pack init my-plugin
```

This generates an `aipm.toml` manifest and a starter project structure.

### Run Tests

```bash
cargo test --workspace
```

## Project Structure

```
crates/
  aipm/         CLI binary (consumer: install, validate, doctor)
  aipm-pack/    CLI binary (author: init, pack, publish, yank)
  libaipm/      Shared library (manifest parsing, validation, store)
specs/          Technical design documents
tests/features/ Cucumber BDD feature files (220+ scenarios)
```

## Contributing

Contributions and suggestions are welcome! Please open an issue or pull request on [GitHub](https://github.com/thelarkinn/aipm).

## License

This project is licensed under the [MIT License](LICENSE).
