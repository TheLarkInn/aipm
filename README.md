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

This project welcomes contributions and suggestions.  Most contributions require you to agree to a
Contributor License Agreement (CLA) declaring that you have the right to, and actually do, grant us
the rights to use your contribution. For details, visit [Contributor License Agreements](https://cla.opensource.microsoft.com).

When you submit a pull request, a CLA bot will automatically determine whether you need to provide
a CLA and decorate the PR appropriately (e.g., status check, comment). Simply follow the instructions
provided by the bot. You will only need to do this once across all repos using our CLA.

This project has adopted the [Microsoft Open Source Code of Conduct](https://opensource.microsoft.com/codeofconduct/).
For more information see the [Code of Conduct FAQ](https://opensource.microsoft.com/codeofconduct/faq/) or
contact [opencode@microsoft.com](mailto:opencode@microsoft.com) with any additional questions or comments.

## Trademarks

This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft
trademarks or logos is subject to and must follow
[Microsoft's Trademark & Brand Guidelines](https://www.microsoft.com/legal/intellectualproperty/trademarks/usage/general).
Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship.
Any use of third-party trademarks or logos are subject to those third-party's policies.
