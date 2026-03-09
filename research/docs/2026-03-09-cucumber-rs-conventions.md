# Cucumber-rs Conventions and Gherkin Syntax for Rust BDD Testing

> Research date: 2026-03-09
> Crate: `cucumber` v0.22.1 (docs.rs, crates.io)

## Summary

cucumber-rs is a fully native Rust implementation of the Cucumber BDD testing framework.
It uses Gherkin `.feature` files to describe behavior in plain language, then maps those
descriptions to Rust step definitions via attribute macros. It supports async execution,
concurrent scenarios by default, and integrates with cargo's test infrastructure.

---

## 1. Feature File Format (Gherkin Syntax)

### Basic Structure

```gherkin
Feature: Short description of the feature
  Optional multi-line description providing context.

  Scenario: Descriptive name of the scenario
    Given some initial context
    When an action is performed
    Then an expected outcome is observed
```

### Keywords Reference

| Keyword | Purpose |
|---------|---------|
| `Feature` | Top-level grouping; one per file |
| `Scenario` | A single concrete example of behavior |
| `Scenario Outline` | A template scenario run once per row in `Examples` |
| `Examples` | Data table providing values for a `Scenario Outline` |
| `Background` | Steps executed before every scenario in the feature/rule |
| `Rule` | Groups scenarios representing one business rule (Gherkin 6+) |
| `Given` | Establishes preconditions |
| `When` | Describes the action or event |
| `Then` | Asserts the expected outcome |
| `And` / `But` | Continues a previous Given/When/Then |

### Tags

Tags are `@`-prefixed metadata placed above Features, Rules, Scenarios, Scenario Outlines, or Examples blocks.

Special built-in tags in cucumber-rs:
- `@serial` -- forces the tagged scenario to run in isolation
- `@allow.skipped` -- permits a scenario to remain unimplemented without failing

---

## 2. Project Directory Structure

```
project-root/
  Cargo.toml
  src/
    main.rs
  tests/
    features/
      domain-area/
        feature-name.feature
    bdd.rs
```

---

## 3. Cargo.toml Setup

```toml
[dev-dependencies]
cucumber = "0.22"
futures = "0.3"

[[test]]
name = "bdd"
harness = false
```

`harness = false` is mandatory. The `[[test]]` `name` must match the filename in `tests/`.

---

## 4. Best Practices

- Be declarative, not imperative
- Keep scenarios to 3-5 steps
- One behavior per scenario
- Scenarios must be independent
- One feature per file
- Group features in directories by domain concept

---

## Sources

- [Cucumber Rust Book](https://cucumber-rs.github.io/cucumber/main/)
- [Gherkin Reference](https://cucumber.io/docs/gherkin/reference/)
- [cucumber crate on docs.rs](https://docs.rs/cucumber)
- [cucumber-rs GitHub](https://github.com/cucumber-rs/cucumber)
