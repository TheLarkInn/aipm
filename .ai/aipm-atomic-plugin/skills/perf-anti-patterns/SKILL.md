---
name: perf-anti-patterns
description: Use when writing or changing TypeScript/Node.js code - prevents O(n²) collection scanning, hand-rolled builtins, wasteful allocation, unsafe dictionaries, polymorphic deopts, and other performance and code quality anti-patterns
---

# Performance & Code Quality Anti-Patterns for TypeScript / Node.js

## Overview

Write code that is correct, idiomatic, and efficient on the first pass. Don't make the reviewer ask for things the language and platform already provide.

**Core principle:** Use the platform. Build maps, not nested loops. Count without allocating. Protect your dictionaries. Keep your shapes monomorphic. Name your types. Hoist your constants.

For violation examples, fix patterns, and extended explanations see `references/anti-pattern-details.md`.

## The Iron Laws

```
 1. NEVER use .find()/.filter()/.some() inside a loop over the same collection
 2. NEVER allocate an array just to read its .length
 3. NEVER scan the same array multiple times when Map.groupBy() exists
 4. NEVER hand-roll a utility that exists in Node.js or the JS standard library
 5. NEVER use plain {} for dictionaries with dynamic keys — use Map or Object.create(null)
 6. NEVER create RegExp or allocate objects inside hot callbacks
 7. NEVER pretty-print data consumed by machines
 8. NEVER duplicate utility logic — extract a generic helper
 9. NEVER use .split() in hot paths — use indexOf/slice loops
10. ALWAYS pass { withFileTypes: true } to readdir/readdirSync
11. ALWAYS extract inline type literals into named interfaces
12. ALWAYS keep object shapes consistent for V8 monomorphism
13. ALWAYS accept Iterable<T> (not T[]) when using for...of in helpers
```

## Anti-Pattern Gates

### AP1: O(n²) Collection Scanning

Pre-build a `Map` before the loop; use `.get()` for O(1) lookup instead of `.find()` inside `.map()`.

```
BEFORE using .find(), .filter(), or .some() on a collection:
  Ask: "Is this inside a loop, .map(), .flatMap(), or another .filter()?"

  IF yes:
    STOP — Pre-build a Map or use Map.groupBy() before the loop
    Key the Map by the property being compared

  Acceptable uses of .find()/.filter():
    - Single call, not inside a loop (one-time linear scan is fine)
    - On a collection known to be tiny (< 10 elements)
```

### AP2: Allocating Arrays Just to Count

Use a counting loop or `countIf` helper; never `.filter().length` when you only need the count.

```
BEFORE writing .filter(...).length:
  Ask: "Do I use the filtered array, or just its count?"

  IF only the count:
    STOP — Use a counting loop or countIf helper
    This avoids allocating a temp array that is immediately discarded

  IF you need both the filtered array AND its length:
    .filter().length is fine — the array is used
```

### AP3: Multiple Passes When One Will Do

Use `Map.groupBy()` once and call `.get()` for each group.

```
BEFORE writing multiple .find() or .filter() calls on the same array:
  Ask: "Am I filtering/finding by the same property each time?"

  IF yes (same discriminant, different values):
    STOP — Use Map.groupBy() once, then .get() each group
```

### AP4: Unsafe Dictionaries

Use `Object.create(null)` or `new Map()` for any dictionary with dynamic keys.

```
BEFORE creating a dictionary with dynamic keys:
  IF keys come from user input, external data, or are unpredictable:
    Use Map<string, V> or Object.create(null)

  NEVER use Object.fromEntries() to build a dictionary
    It silently re-adds Object.prototype
```

### AP5: Hand-Rolling Node.js Built-ins

Prefer `node:path`, `node:util`, `node:url` over hand-rolled string utilities.

```
BEFORE writing a string manipulation utility:
  Ask: "Does Node.js or the JS standard library already do this?"

  Check: node:path, node:util, node:url, node:fs, Map.groupBy, structuredClone

  IF a built-in exists:
    Use it — it handles edge cases you haven't thought of
```

Common substitutions:

| Instead of | Use |
|---|---|
| Custom ANSI strip regex | `stripVTControlCharacters` from `node:util` |
| `str.replace(/.*[/\\]/, '')` | `path.basename(str)` (`path.posix.basename` for `/`-only paths) |
| `str.split('/').pop()` | `path.basename(str)` |
| `s.trim()` before `parseInt(s, 10)` | Just `parseInt(s, 10)` |
| `require('fs')` in `.mts` | `import fs from 'node:fs'` |
| Manual URL parsing | `new URL(str)` |

### AP6: Verbose Map Checks

Use `map.has(key)` not `(map.get(key) ?? []).length > 0`.

```
BEFORE writing (map.get(key) ?? []).length > 0:
  IF the Map guarantees no empty-array values (Map.groupBy, or you only .set() non-empty values):
    Use map.has(key) for existence
    Use !map.has(key) for absence
```

### AP7: Duplicate Utility Logic

Extract `buildMapFromProperty`, `countByKey`, `countIf` helpers accepting `Iterable<T>`.

```
BEFORE writing a loop that builds a Map or counts by key:
  Ask: "Does this exact pattern already exist in this file?"

  IF yes:
    Extract a generic helper parameterized by a selector/key function
    Accept Iterable<T> (not T[]) if the body uses for...of

  Good candidates for extraction:
    - "Build Map<K, V> from collection by property" → buildMapFromProperty
    - "Count occurrences by key" → countByKey
    - "Count items matching predicate" → countIf
```

### AP8: `.split()` on Large Strings

Use an `indexOf`/`slice` loop in hot paths; `.split()` is fine for one-off operations.

```
BEFORE using .split() on a string:
  Ask: "Is this a hot path?"

  IF yes:
    Use an indexOf/slice loop instead — processes one chunk at a time

  IF this is a one-off operation (not in a hot path):
    .split() is fine — readability wins

  Same principle applies to .split(','), .split('\t'), etc.
```

### AP9: Polymorphic Object Shapes (V8 Deoptimization)

Always initialize all properties; use `undefined` for absent values to keep V8 monomorphic.

```
BEFORE conditionally adding properties to an object:
  Ask: "Is this object created in a loop or hot function?"

  IF yes:
    STOP — Always include all properties, use undefined for absent values
    This keeps V8 monomorphic

  IF this is a one-off config object or rarely-created:
    Conditional properties are fine
```

Key rules: always initialize all properties; maintain consistent property order; never add properties after creation; avoid `delete` (creates a hidden class transition — use `Map.delete()` or create a new object); prefer interfaces over ad-hoc objects.

### AP10: RegExp and Object Allocation in Hot Paths

Hoist constant RegExp to module scope; accept `RegExp` parameters instead of `string`.

```
BEFORE creating a RegExp:
  IF the pattern is constant: hoist to module scope as a RegExp literal
  IF inside a loop or callback: move it outside
```

### AP11: Pretty-Printing Machine-Consumed Data

Use `JSON.stringify(data)` not `JSON.stringify(data, null, 2)` for output piped to parsers or LLMs.

### AP12: Untyped Filesystem APIs

Always pass `{ withFileTypes: true }` to `readdirSync`; filter with `.isFile()` / `.isDirectory()`.

### AP13: Inline Type Literals Instead of Named Interfaces

Extract inline types with 3+ properties to a named `interface` following repo conventions.

## Quick Reference

| Anti-Pattern | Fix |
|---|---|
| `.find()`/`.filter()` inside a loop | Pre-build a `Map`, use `.get()` |
| `.filter().length` when only counting | `countIf` helper or single-pass loop |
| Multiple passes filtering by same property | `Map.groupBy()` once, `.get()` each group |
| Plain `{}` for dynamic-key dictionaries | `Object.create(null)` or `new Map()` |
| `Object.fromEntries()` on sorted data | `Object.create(null)` + manual loop |
| `s.trim()` before `parseInt(s, 10)` | Just `parseInt(s, 10)` |
| Custom string utilities | `node:path`, `node:util`, `node:url` |
| `(map.get(k) ?? []).length > 0` | `map.has(k)` |
| Same Map-building loop copied twice | Extract `buildMapFromProperty` with `Iterable<V>` input |
| Same counting loop copied twice | Extract `countByKey` / `countIf` with `Iterable<T>` input |
| Helper accepts `T[]` but uses `for...of` | Widen to `Iterable<T>` to support Set, Map, generators |
| `.split()` in hot paths | `indexOf`/`slice` loop — one chunk at a time |
| Conditional property addition in hot loops | Always include all properties, use `undefined` for absent |
| Objects with varying shapes passed to same fn | Keep shapes monomorphic — same properties, same order |
| `new RegExp()` inside callbacks | Hoist to module scope, accept `RegExp` param |
| `JSON.stringify(data, null, 2)` for machines | `JSON.stringify(data)` |
| `readdirSync` without `withFileTypes` | `{ withFileTypes: true }`, filter `.isFile()` |
| Inline type with 3+ properties | Named `interface` |

## Red Flags

- `.filter()` whose result is only used for `.length`
- `.find()` or `.filter()` inside `.map()`, `.forEach()`, or a `for` loop
- The same array is scanned with `.find()` or `.filter()` more than twice
- `Record<string, V>` initialized with `{}` instead of `Object.create(null)`
- `Object.fromEntries()` used to build a dictionary
- A 5+ line loop that appears twice with the same structure
- `(map.get(key) ?? []).length > 0` instead of `map.has(key)`
- A helper function accepts `T[]` but only uses `for...of` — should accept `Iterable<T>`
- `.split()` in a hot path
- Object properties added conditionally in a hot loop (varying hidden classes)
- `delete obj.prop` anywhere — use `Map.delete()` or create a new object instead
- A function accepts `pattern: string` but all callers pass constants
- `JSON.stringify` with indentation where output is piped or parsed
- `readdirSync` without `{ withFileTypes: true }`
- `.trim()` before `parseInt` or `parseFloat`
- An inline type literal spanning more than ~80 characters

## The Bottom Line

**Use the platform. Build maps, not nested loops. Count without allocating. Protect your dictionaries. Keep your shapes monomorphic. Name your types. Hoist your constants.**

The cost of getting this right at authoring time is near zero. The cost of fixing it in code review is a full round-trip. Write it right the first time.
