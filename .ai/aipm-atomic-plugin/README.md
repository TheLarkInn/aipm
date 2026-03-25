# AIPM Atomic Plugin

A Claude Code plugin implementing the **Atomic Workflow** for structured software development, codebase research, and feature implementation.

## Attribution

This plugin is derived from and inspired by the [Atomic](https://github.com/flora131/atomic) project by Flora. The original prompts and workflow concepts are licensed under the MIT License (Copyright 2025 Flora).

## Overview

The Atomic plugin provides a comprehensive workflow for:

- **Researching codebases** without making judgments or recommendations
- **Creating specifications** from research findings
- **Implementing features** in a structured, test-driven manner
- **Making atomic commits** with proper change tracking
- **Creating pull requests** on GitHub

### Philosophy

All agents in this plugin operate as **documentarians, not critics**:

- Document what **IS**, not what **SHOULD BE**
- No recommendations or improvements suggested
- Focus on precise file:line references
- Neutral, factual documentation of existing code

## Installation

This plugin is distributed through the local marketplace in the `thelarkinn/aipm` repository. It is registered in `.ai/.claude-plugin/marketplace.json` and enabled via `.claude/settings.json`.

## Commands

| Command | Description |
|---------|-------------|
| `/atomic:research-codebase` | Conduct comprehensive research and create documentation in `research/` |
| `/atomic:create-spec` | Generate technical design documents/RFCs from research |
| `/atomic:create-feature-list` | Create `feature-list.json` and `progress.txt` from a specification |
| `/atomic:implement-feature` | Implement a single feature from `feature-list.json` |
| `/atomic:commit` | Create well-formatted atomic commits |
| `/atomic:create-github-pr` | Commit, push, and create a GitHub pull request |
| `/atomic:explain-code` | Provide detailed explanation of code functionality |

## Agents

The plugin provides specialized agents for different tasks:

### Codebase Agents

| Agent | Purpose |
|-------|---------|
| `codebase-locator` | Find WHERE files and components live (file paths, directory structure) |
| `codebase-analyzer` | Understand HOW code works (implementation details, data flow) |
| `codebase-pattern-finder` | Find similar implementations and existing patterns with code examples |

### Research Agents

| Agent | Purpose |
|-------|---------|
| `codebase-research-locator` | Discover relevant documents in the `research/` directory |
| `codebase-research-analyzer` | Extract high-value insights from research documents |
| `codebase-online-researcher` | Search the web for external documentation and best practices |

### Utility Agents

| Agent | Purpose |
|-------|---------|
| `debugger` | Debug errors, test failures, and unexpected behavior |

## Skills

| Skill | Description |
|-------|-------------|
| `testing-anti-patterns` | Avoid common testing pitfalls (testing mock behavior, test-only production methods, incomplete mocks) |
| `prompt-engineer` | Create effective prompts using Anthropic's best practices |
| `perf-anti-patterns` | Prevent performance anti-patterns in TypeScript/Node.js code |

## Workflow

### Research Phase

```
/atomic:research-codebase "How does authentication work?"
```

This spawns parallel sub-agents to:
1. Locate relevant files with `codebase-locator`
2. Analyze implementations with `codebase-analyzer`
3. Find patterns with `codebase-pattern-finder`
4. Search external docs with `codebase-online-researcher`

Output is saved to `research/docs/YYYY-MM-DD-topic.md`.

### Specification Phase

```
/atomic:create-spec research/docs/2025-02-05-authentication-flow.md
```

Creates a detailed technical design document in `specs/` with:
- Architecture diagrams
- API interfaces
- Data models
- Migration plans
- Test plans

### Implementation Phase

```
/atomic:create-feature-list specs/authentication-spec.md
/atomic:implement-feature
```

Creates and works through a structured feature list with:
- Priority ordering
- Step-by-step implementation
- Progress tracking in `research/progress.txt`
- Test-driven development

### Commit Phase

```
/atomic:commit
```

Creates atomic commits with:
- Clear, descriptive messages
- AI authorship attribution

### Pull Request Phase

```
/atomic:create-github-pr
```

Creates a GitHub PR with:
- Clear title and description
- Summary of changes
- Testing information

## Research Directory Structure

```
research/
├── tickets/           # Ticket-related research
│   └── YYYY-MM-DD-XXXX-description.md
├── docs/              # General documentation
│   └── YYYY-MM-DD-topic.md
├── notes/             # Meeting notes and discussions
│   └── YYYY-MM-DD-meeting.md
├── feature-list.json  # Features to implement
└── progress.txt       # Implementation progress log
```

## Configuration

The plugin uses Opus model by default for complex tasks. Individual commands specify their allowed tools and model preferences in their frontmatter.

## License

MIT License - See [LICENSE](LICENSE) for details.

Original Atomic prompts: Copyright (c) 2025 Flora
Plugin adaptation: Sean Larkin (thelarkinn@users.noreply.github.com)
