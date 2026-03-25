---
name: perf-anti-patterns
description: Use when writing or changing TypeScript/Node.js code - prevents O(n²) collection scanning, hand-rolled builtins, wasteful allocation, unsafe dictionaries, polymorphic deopts, and other performance and code quality anti-patterns
---

# Performance & Code Quality Anti-Patterns for TypeScript / Node.js

## Overview

Write code that is correct, idiomatic, and efficient on the first pass. Don't make the reviewer ask for things the language and platform already provide.

**Core principle:** Use the platform. Build maps, not nested loops. Count without allocating. Protect your dictionaries. Keep your shapes monomorphic. Name your types. Hoist your constants.

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

## Anti-Pattern 1: O(n²) Collection Scanning

**The violation:**
```typescript
// ❌ BAD: .find() inside .map() = O(n × m)
const steps = befores.map((b) => {
  const a = afters.find((x) => x.callId === b.callId);
  return { ...b, endTime: a?.endTime };
});
```

**Why:** 1,000 events x 1,000 lookups = 1,000,000 comparisons. Pre-build a Map for O(1) lookups.

**The fix:**
```typescript
// ✅ GOOD: Pre-build Map, then O(1) lookups
const afterMap = new Map(afters.map((a) => [a.callId, a]));

const steps = befores.map((b) => {
  const a = afterMap.get(b.callId!);
  return { ...b, endTime: a?.endTime };
});
```

### Gate Function

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

## Anti-Pattern 2: Allocating Arrays Just to Count

**The violation:**
```typescript
// ❌ BAD: Two temp arrays allocated, only .length is read, then both are discarded
const expects: Step[] = children.filter((c) => c.method === 'expect');
const errors: Step[] = expects.filter((e) => e.error);
return { expectCount: expects.length, expectErrorCount: errors.length };
```

**Why:** V8 does NOT optimize `.filter().length` into a count. It allocates the full array, copies matching elements, reads `.length`, then the GC reclaims it. Benchmarked at **2.4x slower** than a counting loop on real trace data (6,498 events).

**The fix:**
```typescript
// ✅ GOOD: Single pass, zero allocations
let expectCount: number = 0;
let expectErrorCount: number = 0;
for (const c of children) {
  if (c.method === 'expect') {
    expectCount++;
    if (c.error) expectErrorCount++;
  }
}
return { expectCount, expectErrorCount };
```

For standalone counts, use a `countIf` helper (note: accepts `Iterable`, not just arrays):
```typescript
function countIf<T>(items: Iterable<T>, predicate: (item: T) => boolean): number {
  let count: number = 0;
  for (const item of items) {
    if (predicate(item)) count++;
  }
  return count;
}
```

### Gate Function

```
BEFORE writing .filter(...).length:
  Ask: "Do I use the filtered array, or just its count?"

  IF only the count:
    STOP — Use a counting loop or countIf helper
    This avoids allocating a temp array that is immediately discarded

  IF you need both the filtered array AND its length:
    .filter().length is fine — the array is used
```

## Anti-Pattern 3: Multiple Passes When One Will Do

**The violation:**
```typescript
// ❌ BAD: Four O(n) passes over the same array
const contextOpts = events.find((e) => e.type === 'context-options');
const topError = events.find((e) => e.type === 'error');
const befores = events.filter((e) => e.type === 'before');
const afters = events.filter((e) => e.type === 'after');
```

**Why:** 4 full scans when 1 suffices. `Map.groupBy` exists for this.

**The fix:**
```typescript
// ✅ GOOD: Single O(n) pass groups everything
const byType = Map.groupBy(events, (e) => e.type);
const contextOpts = byType.get('context-options')?.[0];
const topError = byType.get('error')?.[0];
const befores = byType.get('before') ?? [];
const afters = byType.get('after') ?? [];
```

### Gate Function

```
BEFORE writing multiple .find() or .filter() calls on the same array:
  Ask: "Am I filtering/finding by the same property each time?"

  IF yes (same discriminant, different values):
    STOP — Use Map.groupBy() once, then .get() each group
```

## Anti-Pattern 4: Unsafe Dictionaries

**The violation:**
```typescript
// ❌ BAD: Plain {} inherits from Object.prototype
const counts: Record<string, number> = {};
for (const s of steps) {
  const m: string = s.method || 'unknown';
  counts[m] = (counts[m] || 0) + 1;
}
```

**Why:** `counts['constructor']` returns `Object.prototype.constructor` (a function, truthy), so `(counts['constructor'] || 0) + 1` produces `NaN`. Also: `Object.fromEntries()` produces prototype-bearing objects — don't use it to build dictionaries.

**The fix:**
```typescript
// ✅ GOOD: Null-prototype object — no inherited keys
const counts: Record<string, number> = Object.create(null);
for (const s of steps) {
  const m: string = s.method || 'unknown';
  counts[m] = (counts[m] || 0) + 1;
}
```

### Gate Function

```
BEFORE creating a dictionary with dynamic keys:
  IF keys come from user input, external data, or are unpredictable:
    Use Map<string, V> or Object.create(null)

  NEVER use Object.fromEntries() to build a dictionary
    It silently re-adds Object.prototype
```

## Anti-Pattern 5: Hand-Rolling Node.js Built-ins

**The violation:**
```typescript
// ❌ BAD: Redundant .trim() — parseInt already skips whitespace per the spec
const nums = values.map((s: string) => parseInt(s.trim(), 10));
```

**Why:** `parseInt` skips leading whitespace per the ECMAScript spec. `.trim()` allocates a new string for nothing. Benchmarked at **1.3x slower** per call — pure waste.

**The fix:**
```typescript
// ✅ GOOD: Drop the redundant work
const nums = values.map((s: string) => parseInt(s, 10));
```

### Common built-ins to prefer

| Instead of | Use |
|---|---|
| Custom ANSI strip regex | `stripVTControlCharacters` from `node:util` |
| `str.replace(/.*[/\\]/, '')` | `path.basename(str)` (`path.posix.basename` for `/`-only paths) |
| `str.split('/').pop()` | `path.basename(str)` |
| `s.trim()` before `parseInt(s, 10)` | Just `parseInt(s, 10)` |
| `require('fs')` in `.mts` | `import fs from 'node:fs'` |
| Manual URL parsing | `new URL(str)` |

### Gate Function

```
BEFORE writing a string manipulation utility:
  Ask: "Does Node.js or the JS standard library already do this?"

  Check: node:path, node:util, node:url, node:fs, Map.groupBy, structuredClone

  IF a built-in exists:
    Use it — it handles edge cases you haven't thought of

  Exceptions (rare):
    - Streaming text decoders that must preserve encoding state across chunks
    - Cases where the built-in's behavior is subtly wrong for your specific use case
```

## Anti-Pattern 6: Verbose Map Checks

**The violation:**
```typescript
// ❌ BAD: Allocates an empty array on every cache miss just to check .length
const hasChildren: boolean = (childrenByParent.get(callId) ?? []).length > 0;
```

**Why:** `Map.groupBy` only creates keys for non-empty groups. The `?? []` creates a throwaway array on every miss. Benchmarked at **1.8x slower** than `.has()`.

**The fix:**
```typescript
// ✅ GOOD: Direct existence check — no allocation, clearer intent
const hasChildren: boolean = childrenByParent.has(callId);
```

### Gate Function

```
BEFORE writing (map.get(key) ?? []).length > 0:
  IF the Map guarantees no empty-array values (Map.groupBy, or you only .set() non-empty values):
    Use map.has(key) for existence
    Use !map.has(key) for absence
```

## Anti-Pattern 7: Duplicate Utility Logic

**The violation:**
```typescript
// ❌ BAD: Same 5-line "build a Map keyed by callId" loop copied twice
const afterMap: Map<string, TraceEvent> = new Map();
for (const a of afters) {
  if (a.callId) afterMap.set(a.callId, a);
}
// ... 200 lines later, identical loop for befores ...
```

**Why:** Two copies of the same bug surface. Extract a helper.

**The fix:**
```typescript
// ✅ GOOD: Generic helper — accepts Iterable so it works on arrays, Sets, Maps, generators
function buildMapFromProperty<K, V>(input: Iterable<V>, selector: (value: V) => K | undefined): Map<K, V> {
  const result: Map<K, V> = new Map();
  for (const elem of input) {
    const key: K | undefined = selector(elem);
    if (key !== undefined) result.set(key, elem);
  }
  return result;
}

const afterMap = buildMapFromProperty(afters, (a) => a.callId);
const beforeByCallId = buildMapFromProperty(befores, (b) => b.callId);
```

### Gate Function

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

## Anti-Pattern 8: `.split()` on Large Strings

**The violation:**
```typescript
// ❌ BAD: Materializes every line as a separate string object at once
const lines = hugeFile.split('\n');
for (const line of lines) {
  processLine(line);
}
```

**Why:** `.split('\n')` on a 10MB file creates thousands of small string objects in a single burst. Every substring is a heap allocation. The entire array and all its elements must live in memory simultaneously, causing GC pressure spikes. For one-off scripts this is fine, but in hot paths or large inputs, it's wasteful.

**The fix:**
```typescript
// ✅ GOOD: Process one line at a time with indexOf/slice — earlier strings can be GC'd
let pos: number = 0;
while (pos < content.length) {
  const nextNewline: number = content.indexOf('\n', pos);
  const end: number = nextNewline === -1 ? content.length : nextNewline;
  const line: string = content.slice(pos, end);
  processLine(line);
  pos = end + 1;
}
```

### Gate Function

```
BEFORE using .split() on a string:
  Ask: "Is this a hot path?"

  IF yes:
    Use an indexOf/slice loop instead — processes one chunk at a time

  IF this is a one-off operation (not in a hot path):
    .split() is fine — readability wins

  Same principle applies to .split(','), .split('\t'), etc.
```

## Anti-Pattern 9: Polymorphic Object Shapes (V8 Deoptimization)

**The violation:**
```typescript
// ❌ BAD: Objects created with different property orders or optional properties
function makeResult(event: TraceEvent) {
  const result: any = { callId: event.callId };
  if (event.error) {
    result.error = event.error;     // sometimes present, sometimes absent
  }
  if (event.title) {
    result.title = event.title;     // same — shape varies per call
  }
  return result;
}
```

**Why:** V8 assigns a hidden class (called "Map" internally, confusingly) to every object. When a function always produces objects with the same properties in the same order, V8 uses a **monomorphic** inline cache — direct memory offset lookup, extremely fast. When shapes vary:

- **Monomorphic** (1 shape): Direct offset lookup. Fast.
- **Polymorphic** (2-4 shapes): Linear search through cached shapes. Slower.
- **Megamorphic** (5+ shapes): Hash table fallback. Slowest.

Adding properties conditionally creates different hidden classes for each combination of present/absent fields. A function called 1,000 times with 3 optional fields can produce up to 8 distinct shapes, pushing V8 into megamorphic mode for every downstream consumer.

**The fix:**
```typescript
// ✅ GOOD: Always produce the same shape — use undefined for absent values
function makeResult(event: TraceEvent) {
  return {
    callId: event.callId,
    error: event.error ?? undefined,
    title: event.title ?? undefined
  };
}
```

### Key Rules

- **Always initialize all properties**, even if the value is `undefined`. This ensures one hidden class.
- **Maintain consistent property order** across object literals that share a type.
- **Never add properties after creation** (e.g., `result.newProp = ...` in an if-block).
- **Avoid the `delete` operator altogether** — it forces a hidden class transition. Instead, create a new object without the property, or use `Map` which is designed for dynamic key removal.
- **Prefer interfaces over ad-hoc objects** — the interface definition naturally enforces a consistent shape.

### Gate Function

```
BEFORE conditionally adding properties to an object:
  Ask: "Is this object created in a loop or hot function?"

  IF yes:
    STOP — Always include all properties, use undefined for absent values
    This keeps V8 monomorphic

  IF this is a one-off config object or rarely-created:
    Conditional properties are fine
```

## Anti-Pattern 10: RegExp and Object Allocation in Hot Paths

**The violation:**
```typescript
// ❌ BAD: RegExp compiled on every .find() iteration, callers pass constant strings
function findFile(dir: string, pattern: string): string | null {
  const files = fs.readdirSync(dir);
  return files.find((f) => f.match(new RegExp(pattern))) ?? null;
}
```

**Why:** `new RegExp(pattern)` compiles per iteration. All callers pass constants — hoist to module scope.

**The fix:**
```typescript
// ✅ GOOD: Hoisted RegExp constant, function takes RegExp directly
const TEST_TRACE_PATTERN: RegExp = /^test\.trace$/;

function findFile(dir: string, pattern: RegExp): string | null {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const match = entries.find((e) => e.isFile() && pattern.test(e.name));
  return match ? path.join(dir, match.name) : null;
}
```

### Gate Function

```
BEFORE creating a RegExp:
  IF the pattern is constant: hoist to module scope as a RegExp literal
  IF inside a loop or callback: move it outside
```

## Anti-Pattern 11: Pretty-Printing Machine-Consumed Data

**The violation:**
```typescript
// ❌ BAD: 2-space indentation for data an LLM or parser will consume
console.log(JSON.stringify(data, null, 2));
```

**Why:** Whitespace adds ~30-40% to JSON size. LLMs and parsers don't benefit. For very large data, even compact `JSON.stringify` can hit V8's string length limit (~512MB) — consider streaming serialization in those cases.

**The fix:**
```typescript
// ✅ GOOD: Compact output for machine consumers
console.log(JSON.stringify(data));
```

## Anti-Pattern 12: Untyped Filesystem APIs

**The violation:**
```typescript
// ❌ BAD: Returns string[], can't tell files from directories
const files = fs.readdirSync(dir);
```

**Why:** Without `withFileTypes`, you need a separate `statSync` per entry.

**The fix:**
```typescript
// ✅ GOOD: Dirent objects with .isFile()/.isDirectory()
const entries = fs.readdirSync(dir, { withFileTypes: true });
const match = entries.find((e) => e.isFile() && pattern.test(e.name));
```

## Anti-Pattern 13: Inline Type Literals Instead of Named Interfaces

**The violation:**
```typescript
// ❌ BAD: Long inline type, unreadable and unreusable
const frames: Array<{ file: string; fullPath: string; line: number; function: string | undefined }> = [];
```

**Why:** Hard to read, can't be reused, buries logic changes in diffs.

**The fix:**
```typescript
// ✅ GOOD: Named interface. Follow repo conventions (this monorepo enforces `I` prefix).
interface IErrorStackFrame {
  file: string;
  fullPath: string;
  line: number;
  function: string | undefined;
}

const frames: IErrorStackFrame[] = [];
```

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
