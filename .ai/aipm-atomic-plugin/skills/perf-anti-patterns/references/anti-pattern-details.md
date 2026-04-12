# Anti-Pattern Violation Examples and Fixes

Detailed violation code, why-it-matters explanations, and fix patterns for each of the 13 anti-patterns. The gate functions live in the main SKILL.md; this file is the extended reference.

---

## AP1: O(n²) Collection Scanning

**Violation:**
```typescript
// ❌ BAD: .find() inside .map() = O(n × m)
const steps = befores.map((b) => {
  const a = afters.find((x) => x.callId === b.callId);
  return { ...b, endTime: a?.endTime };
});
```

**Why:** 1,000 events × 1,000 lookups = 1,000,000 comparisons. Pre-build a Map for O(1) lookups.

**Fix:**
```typescript
// ✅ GOOD: Pre-build Map, then O(1) lookups
const afterMap = new Map(afters.map((a) => [a.callId, a]));
const steps = befores.map((b) => {
  const a = afterMap.get(b.callId!);
  return { ...b, endTime: a?.endTime };
});
```

---

## AP2: Allocating Arrays Just to Count

**Violation:**
```typescript
// ❌ BAD: Two temp arrays allocated, only .length is read, then both are discarded
const expects: Step[] = children.filter((c) => c.method === 'expect');
const errors: Step[] = expects.filter((e) => e.error);
return { expectCount: expects.length, expectErrorCount: errors.length };
```

**Why:** V8 does NOT optimize `.filter().length` into a count. It allocates the full array, copies matching elements, reads `.length`, then the GC reclaims it. Benchmarked at **2.4x slower** than a counting loop on real trace data (6,498 events).

**Fix:**
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

---

## AP3: Multiple Passes When One Will Do

**Violation:**
```typescript
// ❌ BAD: Four O(n) passes over the same array
const contextOpts = events.find((e) => e.type === 'context-options');
const topError = events.find((e) => e.type === 'error');
const befores = events.filter((e) => e.type === 'before');
const afters = events.filter((e) => e.type === 'after');
```

**Why:** 4 full scans when 1 suffices. `Map.groupBy` exists for this.

**Fix:**
```typescript
// ✅ GOOD: Single O(n) pass groups everything
const byType = Map.groupBy(events, (e) => e.type);
const contextOpts = byType.get('context-options')?.[0];
const topError = byType.get('error')?.[0];
const befores = byType.get('before') ?? [];
const afters = byType.get('after') ?? [];
```

---

## AP4: Unsafe Dictionaries

**Violation:**
```typescript
// ❌ BAD: Plain {} inherits from Object.prototype
const counts: Record<string, number> = {};
for (const s of steps) {
  const m: string = s.method || 'unknown';
  counts[m] = (counts[m] || 0) + 1;
}
```

**Why:** `counts['constructor']` returns `Object.prototype.constructor` (a function, truthy), so `(counts['constructor'] || 0) + 1` produces `NaN`. Also: `Object.fromEntries()` produces prototype-bearing objects — don't use it to build dictionaries.

**Fix:**
```typescript
// ✅ GOOD: Null-prototype object — no inherited keys
const counts: Record<string, number> = Object.create(null);
for (const s of steps) {
  const m: string = s.method || 'unknown';
  counts[m] = (counts[m] || 0) + 1;
}
```

---

## AP5: Hand-Rolling Node.js Built-ins

**Violation:**
```typescript
// ❌ BAD: Redundant .trim() — parseInt already skips whitespace per the spec
const nums = values.map((s: string) => parseInt(s.trim(), 10));
```

**Why:** `parseInt` skips leading whitespace per the ECMAScript spec. `.trim()` allocates a new string for nothing. Benchmarked at **1.3x slower** per call — pure waste.

**Fix:**
```typescript
// ✅ GOOD: Drop the redundant work
const nums = values.map((s: string) => parseInt(s, 10));
```

---

## AP6: Verbose Map Checks

**Violation:**
```typescript
// ❌ BAD: Allocates an empty array on every cache miss just to check .length
const hasChildren: boolean = (childrenByParent.get(callId) ?? []).length > 0;
```

**Why:** `Map.groupBy` only creates keys for non-empty groups. The `?? []` creates a throwaway array on every miss. Benchmarked at **1.8x slower** than `.has()`.

**Fix:**
```typescript
// ✅ GOOD: Direct existence check — no allocation, clearer intent
const hasChildren: boolean = childrenByParent.has(callId);
```

---

## AP7: Duplicate Utility Logic

**Violation:**
```typescript
// ❌ BAD: Same 5-line "build a Map keyed by callId" loop copied twice
const afterMap: Map<string, TraceEvent> = new Map();
for (const a of afters) {
  if (a.callId) afterMap.set(a.callId, a);
}
// ... 200 lines later, identical loop for befores ...
```

**Fix:**
```typescript
// ✅ GOOD: Generic helper — accepts Iterable so it works on arrays, Sets, Maps, generators
function buildMapFromProperty<K, V>(
  input: Iterable<V>,
  selector: (value: V) => K | undefined
): Map<K, V> {
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

---

## AP8: `.split()` on Large Strings

**Violation:**
```typescript
// ❌ BAD: Materializes every line as a separate string object at once
const lines = hugeFile.split('\n');
for (const line of lines) { processLine(line); }
```

**Why:** `.split('\n')` on a 10MB file creates thousands of small string objects in a single burst. Every substring is a heap allocation. The entire array and all its elements must live in memory simultaneously, causing GC pressure spikes.

**Fix:**
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

---

## AP9: Polymorphic Object Shapes (V8 Deoptimization)

**Violation:**
```typescript
// ❌ BAD: Objects created with different property orders or optional properties
function makeResult(event: TraceEvent) {
  const result: any = { callId: event.callId };
  if (event.error) { result.error = event.error; }
  if (event.title) { result.title = event.title; }
  return result;
}
```

**Why:** V8 assigns a hidden class to every object. Varying shapes push V8 from monomorphic (fast) to polymorphic or megamorphic (slow hash table). Adding properties conditionally creates different hidden classes for each combination — a function called 1,000 times with 3 optional fields can produce up to 8 distinct shapes.

- **Monomorphic** (1 shape): Direct offset lookup. Fast.
- **Polymorphic** (2–4 shapes): Linear search through cached shapes. Slower.
- **Megamorphic** (5+ shapes): Hash table fallback. Slowest.

**Fix:**
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

---

## AP10: RegExp and Object Allocation in Hot Paths

**Violation:**
```typescript
// ❌ BAD: RegExp compiled on every .find() iteration
function findFile(dir: string, pattern: string): string | null {
  const files = fs.readdirSync(dir);
  return files.find((f) => f.match(new RegExp(pattern))) ?? null;
}
```

**Fix:**
```typescript
// ✅ GOOD: Hoisted RegExp constant, function takes RegExp directly
const TEST_TRACE_PATTERN: RegExp = /^test\.trace$/;

function findFile(dir: string, pattern: RegExp): string | null {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const match = entries.find((e) => e.isFile() && pattern.test(e.name));
  return match ? path.join(dir, match.name) : null;
}
```

---

## AP11: Pretty-Printing Machine-Consumed Data

**Violation:**
```typescript
// ❌ BAD: 2-space indentation for data an LLM or parser will consume
console.log(JSON.stringify(data, null, 2));
```

**Why:** Whitespace adds ~30–40% to JSON size. LLMs and parsers don't benefit.

**Fix:**
```typescript
// ✅ GOOD: Compact output for machine consumers
console.log(JSON.stringify(data));
```

---

## AP12: Untyped Filesystem APIs

**Violation:**
```typescript
// ❌ BAD: Returns string[], can't tell files from directories
const files = fs.readdirSync(dir);
```

**Fix:**
```typescript
// ✅ GOOD: Dirent objects with .isFile()/.isDirectory()
const entries = fs.readdirSync(dir, { withFileTypes: true });
const match = entries.find((e) => e.isFile() && pattern.test(e.name));
```

---

## AP13: Inline Type Literals Instead of Named Interfaces

**Violation:**
```typescript
// ❌ BAD: Long inline type, unreadable and unreusable
const frames: Array<{ file: string; fullPath: string; line: number; function: string | undefined }> = [];
```

**Fix:**
```typescript
// ✅ GOOD: Named interface — follow repo conventions (this monorepo enforces `I` prefix)
interface IErrorStackFrame {
  file: string;
  fullPath: string;
  line: number;
  function: string | undefined;
}

const frames: IErrorStackFrame[] = [];
```
