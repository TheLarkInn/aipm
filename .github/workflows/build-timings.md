---
description: >
  Daily build timings analyzer — runs once per day on weekdays. Collects
  cargo build timing data, analyzes compilation bottlenecks, checks cargo
  lints for build-time improvements, and opens an issue summarizing findings
  with actionable recommendations to reduce build times.
on:
  schedule: daily on weekdays
  workflow_dispatch:
permissions:
  contents: read
  issues: read
  pull-requests: read
tools:
  github:
    toolsets: [default]
network:
  allowed: [defaults, rust]
steps:
  - uses: dtolnay/rust-toolchain@stable
    with:
      components: clippy, rustfmt
  - uses: Swatinem/rust-cache@v2
safe-outputs:
  create-issue:
    max: 1
  add-labels:
    allowed: [build-timings]
    max: 1
  noop:
---

# Build Timings Analyzer

You are an expert Rust build engineer analyzing compilation performance for
this project. Your goal is to collect build timing data, identify bottlenecks,
and report actionable recommendations that can reduce build times.

## Lint Rules (MUST follow — compiler will reject violations)

All lint rules are defined in `Cargo.toml` under `[workspace.lints]`.
Key rules:

- **NEVER** add `#[allow(...)]` or `#[expect(...)]` attributes.
- **NEVER** use `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, `unimplemented!()`, `unreachable!()`. Use `Result`/`Option` combinators or `?`.
- **NEVER** use `println!()`, `eprintln!()` — use `write!()`/`writeln!()` or tracing.
- **NEVER** use `dbg!()` or `unsafe`.
- Prefer `.get()` over `[]` indexing.

## Step-by-step Instructions

### 1 — Clean build environment

Ensure a clean build state so timing data is accurate and reproducible:

```bash
cargo clean
```

### 2 — Run cargo build with timings

Run a full workspace build with the `--timings` flag to generate detailed
compilation timing data:

```bash
cargo build --workspace --timings 2>&1
```

This generates an HTML report at `target/cargo-timings/cargo-timing.html`
(a symlink to the latest timestamped report) and prints a summary to stdout.

### 3 — Capture the build timing summary

Parse the build output and the generated timing data. Record:

- **Total build time** (wall clock)
- **Number of compilation units**
- **Maximum concurrency achieved**
- **Top 5 slowest compilation units** (by duration)
- **Critical path dependencies** (units that block the most other units)
- **Code generation times** vs **compilation times** for the slowest units

Also check the timing report:

```bash
ls -la target/cargo-timings/
```

### 4 — Run cargo lints check

Check for cargo lints that can help decrease build times. Run clippy with
particular attention to performance-related suggestions:

```bash
cargo clippy --workspace -- -D warnings 2>&1
```

Additionally, check for common build-time issues:

```bash
# Check for multiple versions of the same crate
cargo tree --duplicates 2>&1

# Count direct dependencies (high counts may indicate over-dependency)
echo "Direct dependency count:"
cargo tree --depth 1 2>&1 | wc -l
```

### 5 — Analyze the Cargo.toml for build-time lints

Read the workspace `Cargo.toml` and check for:

- **`[profile.dev]` optimizations**: Are `opt-level`, `debug`, `split-debuginfo`
  configured for faster dev builds?
- **`[profile.dev.package."*"]`**: Are dependencies compiled with optimizations
  while keeping the workspace crates in debug mode?
- **Feature flags**: Are there unnecessary features enabled on dependencies that
  increase compile time?
- **Cargo lint configuration**: Check if `[lints.cargo]` section exists and
  whether build-time-relevant lints are enabled.

### 6 — Check for build-time improvement opportunities

Look for these common issues:

1. **Slow dependencies**: Identify crates that take disproportionately long
   to compile. Check if they have optional features that could be disabled.
2. **Duplicate dependencies**: Multiple versions of the same crate increase
   compile time. Suggest version unification where possible.
3. **Large crates**: Identify crates that could be split for better parallelism.
4. **Bottleneck crates**: Find crates that many others depend on — improving
   these unlocks more parallelism.
5. **Unused dependencies**: Check if any declared dependencies are not actually
   used in the code.

### 7 — Compare with previous build timings (if available)

Search for previous build-timings issues to compare trends:

Use the GitHub tool to search for existing issues with the `build-timings`
label. If previous issues exist, compare key metrics:

- Total build time trend (improving / stable / degrading)
- New slow dependencies added since last report
- Dependency count changes

### 8 — Create a summary issue

Use the `create-issue` safe output to open an issue with the following format:

**Title**: `[build-timings] Build Performance Report — <date>`

**Body** should include:

1. **Build Summary**
   - Total build time
   - Number of compilation units
   - Max concurrency achieved
   - Compiler version

2. **Top 5 Slowest Compilation Units**
   - Table with: crate name, build time, codegen time, features enabled

3. **Dependency Analysis**
   - Total number of direct dependencies
   - Total number of transitive dependencies
   - Any duplicate crate versions found

4. **Cargo Lint Findings**
   - Any clippy warnings or suggestions
   - Build-time-relevant lint recommendations

5. **Recommendations**
   - Prioritized list of actionable improvements
   - Estimated impact (high / medium / low)
   - Specific changes to make (e.g., disable feature X on crate Y)

6. **Trend** (if previous reports exist)
   - Build time comparison with last report
   - New dependencies added
   - Dependencies removed

Then use the `add-labels` safe output to add the `build-timings` label.

### 9 — Nothing noteworthy?

If the build is fast (under 30 seconds) and there are no actionable
recommendations, call the `noop` safe output with a message like:
> "Build timings analysis complete — build is healthy at X seconds with
> no actionable recommendations."
