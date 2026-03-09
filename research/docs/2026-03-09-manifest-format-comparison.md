---
date: 2026-03-09 11:06:05 PDT
researcher: Claude Opus 4.6
git_commit: 9ed90fe83636e78e067b21f37d6fee72492dc0d7
branch: main
repository: aipm
topic: "Manifest Format Comparison: TOML vs JSON vs JSONC vs YAML"
tags: [research, manifest, toml, json, yaml, format-comparison]
status: complete
last_updated: 2026-03-09
last_updated_by: Claude Opus 4.6
---

# Manifest Format Comparison: TOML vs JSON vs JSONC vs YAML

## Research Question

Is TOML the best file format for AIPM's manifest? What are the arguments for JSON, JSONC, and YAML? What data exists on developer preferences?

## Summary

TOML is well-validated for `aipm.toml` but the decision is not clear-cut. The data shows format choice depends on the audience (humans vs machines vs AI agents) and the integration surface. AIPM's unique challenge is that its **entire target ecosystem (Claude Code, MCP, Agency) uses JSON** while its **implementation language (Rust) natively favors TOML**.

---

## 1. Ecosystem Adoption Data

### Package Managers

| Tool | Format | Why |
|---|---|---|
| npm | JSON (package.json) | Universal parsing, zero learning curve |
| Cargo (Rust) | TOML (Cargo.toml) | Human-editable, typed values, comments |
| Poetry/pip (Python) | TOML (pyproject.toml) | PEP 518: explicitly chose over JSON and YAML |
| Go | Custom (go.mod) | Bespoke format; rejected all standards |
| pnpm | YAML + JSON | Workspace config in YAML, packages in JSON |
| Deno | JSON/JSONC (deno.json) | Simplicity, VS Code compat, JSON Schema |
| Bun | JSON (package.json) | npm compatibility |

### AI Tooling (AIPM's integration surface)

| Tool | Format | Notes |
|---|---|---|
| Claude Code plugins | JSON (plugin.json) | Strict JSON, no comments |
| MCP servers | JSON (.mcp.json) | Machine-friendly |
| Agency | JSON (.mcp.json) | Claude Code compat |
| Claude Code hooks | JSON (hooks.json) | Machine-friendly |
| Agent Skills | YAML frontmatter + markdown | Human-readable |
| Claude Code agents | YAML frontmatter + markdown | Human-readable |
| VS Code | JSONC (settings.json) | Microsoft's JSONC consumer |
| GitHub Actions | YAML | DevOps standard |

**Key observation**: AIPM's entire integration surface uses JSON or YAML. No tool in the AI ecosystem uses TOML.

---

## 2. Developer Sentiment Data

### Python PEP 518 (Most Rigorous Documented Rationale)

Python's packaging authority formally evaluated all formats and chose TOML:

> "human-usable (unlike JSON), flexible enough (unlike configparser), stems from a standard, and not overly complex (unlike YAML)."

Rejections:
- **JSON**: "the syntax does not lend itself to easy editing by a human being"
- **YAML**: 86-page spec, "not safe by default" (arbitrary code execution in parsers)

### Rust/Cargo Community

Cargo chose TOML at inception. Per PEP 518: "The Rust community... have been quite happy with their choice of TOML."

### Hacker News Consensus (multiple threads)

- TOML supporters: "If you're creating a file maintained by humans, use TOML"
- TOML critics: "TOML quickly breaks down with lots of nested arrays of objects"
- YAML detractors: "YAML is awful and needs to die"
- JSON pragmatists: universally understood but "not human-friendly"
- **General consensus**: No single winner. TOML for human config, JSON for machine interchange, YAML only where mandated.

---

## 3. Technical Tradeoffs

### TOML

| Pros | Cons |
|------|------|
| Comments (`#`) | Deep nesting becomes verbose (3+ levels) |
| Explicit typing (strings, ints, dates) | `[[array.of.tables]]` syntax confuses newcomers |
| No indentation sensitivity | Inline tables can't span lines (v1.0) |
| Cargo/pyproject.toml precedent | Less universally known than JSON |
| Taplo LSP for IDE support | Not used by any AI tooling ecosystem |

### JSON

| Pros | Cons |
|------|------|
| Universal: stdlib parser in every language | No comments |
| Zero learning curve | No trailing commas |
| JSON Schema is mature & widely adopted | Verbose (closing braces, quoted keys) |
| Best IDE support (SchemaStore) | Poor multiline strings |
| What Claude Code/MCP/Agency already use | Not human-friendly for editing |

### JSONC (JSON with Comments)

| Pros | Cons |
|------|------|
| Comments (`//`, `/* */`) solve JSON's biggest complaint | Non-standard; `JSON.parse()` rejects it |
| Trailing commas allowed | Fragmented specs (jsonc.org vs JSONC Spec) |
| VS Code, tsconfig already use it | Parser availability narrower than JSON |
| Microsoft maintains node-jsonc-parser | Only 2/5 languages have mature parsers |

### YAML

| Pros | Cons |
|------|------|
| Most human-readable for complex nesting | **Norway problem**: `NO` → `false`, `3.10` → `3.1` |
| Comments (`#`) | **Security**: deserialization CVEs (2026-24009 is recent) |
| Anchors/aliases for DRY config | Indentation-sensitive: silent semantic changes |
| Dominant in DevOps/CI | 86-page spec; parser inconsistencies (1.1 vs 1.2) |

---

## 4. The AI Agent Angle

### LLM Reading Comprehension (Improving Agents benchmark, 1000 questions)

| Model | JSON | YAML | XML |
|---|---|---|---|
| GPT-5 Nano | 50.3% | **62.1%** | 44.4% |
| Llama 3.2 3B | **52.7%** | 49.1% | 50.7% |
| Gemini 2.5 Flash Lite | 43.1% | **51.9%** | 33.8% |

YAML wins for reading in 2/3 models. TOML was not tested.

### LLM Generation Accuracy (Aider benchmarks)

- Markdown/plain text pass rates: 60.0-60.9%
- JSON-wrapped pass rates: 51.2-59.5% (**3-9% decline**)
- Primary failure: improper escaping of quotes and newlines in JSON strings

### Implications

- **JSON generation**: LLMs make escaping errors with embedded code/multiline content
- **YAML generation**: LLMs read YAML well but indentation errors in generation are a known failure mode
- **TOML generation**: No benchmarks exist, but explicit syntax + no indentation sensitivity should reduce ambiguity
- **For simple key-value manifests**: All formats are reliable
- **For manifests with code snippets or multiline descriptions**: TOML > JSON (no escaping needed for multiline strings)

---

## 5. Cross-Language Parser Availability

| | JSON | TOML | YAML | JSONC |
|---|---|---|---|---|
| **Rust** | serde_json (stdlib-tier) | toml (Cargo's own) | serde_yaml | jsonc-parser |
| **Python** | json (stdlib) | tomllib (stdlib 3.11+) | PyYAML | commentjson |
| **Node.js** | JSON.parse (built-in) | smol-toml | js-yaml | jsonc-parser (MS) |
| **Go** | encoding/json (stdlib) | BurntSushi/toml | gopkg.in/yaml.v3 | gojsonc |
| **C#/.NET** | System.Text.Json (stdlib) | Tomlyn | YamlDotNet | None (strip comments) |
| **Stdlib count** | **5/5** | **2/5** | **0/5** | **0/5** |
| **Mature 3rd-party** | 5/5 | 5/5 | 5/5 | 2/5 |

---

## 6. The Case FOR Each Format (as aipm.toml alternative)

### The Case for TOML (status quo)

1. **Rust-native**: `toml` crate powers Cargo itself; zero-risk parser choice
2. **Human-editable**: Comments, typed values, no indentation traps
3. **Precedent**: Both Cargo and pyproject.toml chose TOML for the same reasons
4. **PEP 518 validated**: Python's formal evaluation backs TOML over JSON and YAML
5. **AI-safe generation**: No indentation sensitivity, explicit multiline strings

### The Case for JSON

1. **Ecosystem alignment**: Every tool AIPM integrates with (Claude Code, MCP, Agency) uses JSON
2. **Universal parsing**: Stdlib in every language; zero dependencies for consumers
3. **JSON Schema**: Mature validation, IDE autocomplete, SchemaStore integration
4. **AI familiarity**: LLMs have the most training data on JSON structures
5. **No learning curve**: Every developer already knows JSON

### The Case for JSONC

1. **Best of both worlds**: JSON compatibility + comments + trailing commas
2. **VS Code precedent**: Microsoft's own choice for human-edited config
3. **Ecosystem alignment**: Closer to JSON than TOML for Claude Code/MCP interop
4. **tsconfig precedent**: TypeScript ecosystem already embraced "JSON that allows comments"

### The Case for YAML

1. **DevOps alignment**: CI/CD, GitHub Actions, Docker Compose all use YAML
2. **Best LLM reading comprehension**: 62.1% vs 50.3% for JSON in benchmarks
3. **Agent Skills already use it**: YAML frontmatter is the Agent Skills standard
4. **Complex nesting**: YAML handles deeply nested structures most elegantly
5. **pnpm workspace precedent**: pnpm chose YAML for workspace configuration

---

## 7. The Case AGAINST Each Format

### Against TOML
- **Ecosystem mismatch**: No AI tooling uses TOML; requires format translation for Claude Code/MCP integration
- **Nesting pain**: `[[array.of.tables]]` becomes unwieldy for complex dependency declarations
- **Less known**: Developers outside Rust/Python may not know TOML syntax

### Against JSON
- **No comments**: Cannot annotate configs; AI agents can't leave explanatory notes
- **Merge conflicts**: No trailing commas means every addition changes two lines
- **Not human-friendly**: PEP 518 formally rejected it for this reason

### Against JSONC
- **Non-standard**: Two competing specs, fragmented parser ecosystem
- **Worst of both worlds risk**: Neither fully JSON-compatible nor as readable as TOML
- **C#/.NET gap**: No mature JSONC parser for a key AIPM target platform

### Against YAML
- **Norway problem**: Version "3.10" silently becomes float 3.1; country code "NO" becomes false
- **Security CVEs**: Active deserialization vulnerabilities as recently as 2026
- **Indentation traps**: A single space error silently changes semantics
- **PEP 518 formally rejected it**: "Not safe by default"

---

## Related Research

- `research/docs/2026-03-09-npm-core-principles.md` — npm uses JSON (package.json)
- `research/docs/2026-03-09-cargo-core-principles.md` — Cargo uses TOML (Cargo.toml)
- `research/docs/2026-03-09-pnpm-core-principles.md` — pnpm uses YAML + JSON
- `research/docs/2026-03-09-agency-and-ai-orchestration.md` — Agency/MCP/Claude Code all use JSON

## Sources

- [PEP 518 - Specifying Minimum Build System Requirements](https://peps.python.org/pep-0518/)
- [TOML Official Site](https://toml.io/en/)
- [JSON Schema](https://json-schema.org/)
- [JSONC Specification](https://jsonc.org/)
- [Which Nested Data Format Do LLMs Understand Best?](https://www.improvingagents.com/blog/best-nested-data-format/)
- [LLMs are bad at returning code in JSON (Aider)](https://aider.chat/2024/08/14/code-in-json.html)
- [The Norway Problem - StrictYAML](https://hitchdev.com/strictyaml/why/implicit-typing-removed/)
- [PyYAML CVE-2026-24009](https://www.oligo.security/blog/docling-rce-a-shadow-vulnerability-introduced-via-pyyaml-cve-2026-24009)
- [TOML nesting issues (toml-lang/toml#781)](https://github.com/toml-lang/toml/issues/781)
- [Microsoft node-jsonc-parser](https://github.com/microsoft/node-jsonc-parser)
- [HN: Config format consensus](https://news.ycombinator.com/item?id=20731639)
- [Deno Configuration Docs](https://docs.deno.com/runtime/fundamentals/configuration/)
