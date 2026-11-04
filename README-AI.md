# Apate — AI Agent Reference

**Project version:** 0.2.1 | **Git:** 9d3683721943c19f1c0afaca41846230a4aeac7a

> This document is written **for AI agents** (and any tool that consumes it). It is a precise,
> unambiguous reference of the Apate project: its DSL (TOML specification), its scripting
> APIs (Jinja/minijinja and Rhai), its Rust test-library API,
> and how to run it as a Docker image.
> Prefer this file over `README.md`.

---

## 1. Basic overview

**Apate** is an API prototyping / mocking server and a Rust library.

Two main usage scenarios:

1. **Standalone server** — a single binary `apate` that serves HTTP and ships a small
   web UI for editing specs on fly. Also distributable as a Docker image.
2. **Rust test library** — in-process ApateTestServer for `#[test]` / `#[tokio::test]` to
   test client logic against a real local HTTP endpoint without touching the network.

### Core concepts (vocabulary you will see everywhere)

| Term         | Meaning                                                                                              |
|--------------|------------------------------------------------------------------------------------------------------|
| **Spec**     | A TOML document describing everything Apate should mock. Deserialized into `ApateSpecs`.             |
| **Deceit**   | One unit of a spec. Targets one or more URIs and holds matchers, headers, processors, and responses.  |
| **Response** | A candidate body under a `Deceit`. Has its own matchers, code, headers, processors, output type/text. |
| **Matcher**  | A boolean check on the incoming request. All must pass for a response to be selected.                 |
| **Output**   | The response body, produced from `output` text via a chosen `type` (string/jinja/hex/base64/rhai).   |
| **Processor**| Post-processing step that can rewrite the already-rendered response body (Rhai or embedded Rust).     |
| **Rhai registry** | Named, reusable Rhai scripts (`[[rhai]]`) referenced by id from matchers/outputs/processors.   |

### Request processing pipeline (exact order)

For every incoming HTTP request, Apate does the following:

1. Iterate `deceit[]` **in order**.
2. For each `Deceit`, try to match the request path against `uris[]` (supports path
   arguments like `/user/{user_id}`). No match → try next `Deceit`.
3. If the path matched, evaluate **deceit-level matchers**. If any fail → try next `Deceit`.
4. Evaluate **response-level matchers** for each `responses[]` in order. The **first**
   response whose matchers all pass is selected. A response with an **empty** `matchers`
   list matches immediately and hides all later responses. If none pass → try next `Deceit`.
5. Render the selected response body using its `type` and `output`.
6. Apply **processors** (deceit-level first, then response-level, in order). A processor
   may replace the body.
7. Return the body with the response `code` (or a code forced by a template/script), the
   merged headers (deceit-level then response-level), and default `200` if no code was set.
8. If no `Deceit` matched at all → `404`.

> **Important:** matcher evaluation is short-circuiting. The `and`/`or` combinators and the
> implicit "all must pass" rule both return a single boolean per matcher list.

### Default port & constants

- Default port: **`8228`** (`apate::DEFAULT_PORT`).
- Default log filter: **`info,apate=info`** (`apate::DEFAULT_RUST_LOG`).
- Web UI + admin API base path: **`/apate`**. MCP endpoint: **`/mcp`**.
- Server binds `0.0.0.0:<port>` with HTTP keep-alive **disabled**.

### Crate features

- `default = ["server"]` — the `server` feature enables `getopt3` (CLI parsing) and
  `include_dir` (embedded web UI). The binary target requires `server`.
- To use Apate **purely as a library** (no web UI, smaller build), disable default features:
  `apate = { version = "0.2", default-features = false }`.

### Rust edition & toolchain

Apate uses **Rust edition 2024**. The project requires Rust **1.98+** (see `rust-toolchain.toml`).

---

## 2. DSL specification

The spec is a **TOML** document deserialized into `apate::ApateSpecs`. Top level has two
optional arrays: `deceit` (required in practice) and `rhai` (named reusable scripts).

```toml
# Top-level shape (all fields optional, default to empty arrays)
deceit = [ /* Deceit ... */ ]
rhai   = [ /* { id = "...", script = "..." } ... */ ]
```

### 2.1 `Deceit` (TOML: `[[deceit]]`)

```toml
[[deceit]]
uris      = ["/user/list", "/user/{user_id}"]        # required. One or more URIs / patterns
headers   = [["Content-Type", "application/json"]]   # optional. [key, value] pairs
matchers  = [ /* Matcher ... */ ]                     # optional. ALL must pass
processors = [ /* Processor ... */ ]                  # optional. run after body render
responses = [ /* DeceitResponse ... */ ]              # optional. first passing one is used
```

Field notes:
- `uris` — each entry is a literal path or a pattern with path arguments
  (`/user/{user_id}`). The first URI that captures-matches the request path wins.
- `headers` — an array of `[key, value]` string pairs. Applied to the response.
- `matchers` — deceit-level matchers. If any fails, this whole `Deceit` is skipped.
- `processors` — run after the selected response body is rendered (before response-level
  processors). See §2.5.
- `responses` — ordered list. The first one whose matchers pass is chosen. A response with
  **no** `matchers` always matches; put such a fallback **last** (later responses become
  unreachable).

### 2.2 `DeceitResponse` (TOML: `[[deceit.responses]]`)

```toml
[[deceit.responses]]
code       = 200                    # optional. HTTP status for THIS response
matchers   = [ /* Matcher ... */ ]  # optional. ALL must pass; empty = always match
headers    = [["X-Custom", "1"]]    # optional. extra headers for this response
processors = [ /* Processor ... */ ]# optional. run after body render
type       = "string"               # optional. see Output types §2.4 (default "string")
output     = "..."                  # the body source, interpreted according to `type`
```

### 2.3 Matchers — full reference

TOML tag is `type`. All matchers except `rhai`/`rhai_ref` support an optional `negate`
boolean (default `false`). `and`/`or` are combinators.

| `type`      | Fields                                          | Description                                             |
|-------------|-------------------------------------------------|---------------------------------------------------------|
| `method`    | `eq`, `negate`?                                 | HTTP method match; `eq` is case-insensitive (e.g. `"GET"` or `"get"`) |
| `header`    | `key`, `value`, `negate`?                       | Exact match on a request header **value**; `key` must be **lowercase** |
| `query_arg` | `name`, `value`, `negate`?                      | Exact match on a URL query argument                       |
| `path_arg`  | `name`, `value`, `negate`?                      | Exact match on a path argument (`/user/{user_id}`)        |
| `json`      | `path`, `eq`, `negate`?                         | JSONPath must yield **exactly one** string equal to `eq` |
| `rhai`      | `script`                                        | Inline Rhai script; must return a **boolean**            |
| `rhai_ref`  | `id`, `args`?                                   | Reusable Rhai matcher by registry id; must return bool   |
| `and`       | `matchers` (array)                              | True only if **all** inner matchers are true             |
| `or`        | `matchers` (array)                              | True if **any** inner matcher is true                    |

Examples:

```toml
{ type = "method", eq = "GET" }
{ type = "header", key = "user-agent", value = "curl" }   # keys are lowercase
{ type = "query_arg", name = "name", value = "Ivan" }
{ type = "path_arg", name = "user_id", value = "740" }
{ type = "json", path = "$.name", eq = "Rajesh" }
{ type = "and", matchers = [ { type = "method", eq = "POST" }, { type = "json", path = "$.id", eq = "1" } ] }
{ type = "or",  matchers = [ { type = "json", path = "$.name", eq = "A" }, { type = "json", path = "$.name", eq = "B" } ] }

# Inline Rhai matcher (returns boolean)
{ type = "rhai", script = """
if ctx.load_query_args().foo == "none" { return false; }
return true;
""" }

# Reusable Rhai matcher (defined in [[rhai]] registry)
{ type = "rhai_ref", id = "top-level-script", args = ["arg1", "arg2"] }
```

> **JSON matcher constraint:** the JSONPath expression must return exactly one result and
> that result must be a string equal to `eq`, otherwise the matcher fails. Request bodies
> that are empty or not valid JSON also make the matcher fail.

> **Header matcher keys:** request header names are normalized to lowercase by the HTTP
> stack, so always use lowercase `key` values (`"user-agent"`, not `"User-Agent"`) and
> compare values exactly (case-sensitive).

> **Query args:** repeated query parameters (`?a=1&a=2`) collapse into a single map entry;
> only one value is compared.

### 2.4 Output types (TOML: `type` on a response)

| `type`     | `output` is interpreted as                          | Notes                                                    |
|------------|-----------------------------------------------------|----------------------------------------------------------|
| `string`   | raw string, returned as-is (DEFAULT)                |                                                          |
| `jinja`    | minijinja template (Jinja2-compatible syntax)        | See §3 Jinja templates API                                |
| `hex`      | HEX string → decoded bytes (no `0x` prefix expected) | Optional `0x` prefix is stripped                          |
| `base64`   | Base64 string → decoded bytes                         | Standard alphabet                                          |
| `rhai`     | inline Rhai script → returns a Blob (response body)  | Return `()` for an **empty** body                  |
| `rhai_ref` | reusable Rhai script by id → returns a Blob          | Registry `id`; optional `args = [...]`             |

Binary examples (`hex`, `base64`) are the way to return non-text payloads (e.g. PNG files).
A Rhai script may return `()` (unit) to produce an **empty** response body; any other
non-Blob return type is an error (→ HTTP 500).

`rhai_ref` as an **output type** uses the externally-tagged form (unlike matchers/processors):

```toml
[[deceit.responses]]
type = { rhai_ref = { id = "script-id", args = ["arg1", "arg2"] } }
```

### 2.5 Processors (TOML: `[[deceit.processors]]` or `[[deceit.responses.processors]]`)

A processor rewrites the **already rendered** response body. They run in order; each one
receives the body produced by the previous step.

| `type`     | Fields        | Description                                          |
|------------|---------------|------------------------------------------------------|
| `rhai`     | `script`      | Inline Rhai; returns Blob (new body) or `()` (keep)  |
| `rhai_ref` | `id`, `args`? | Reusable Rhai script by registry id                  |
| `embedded` | `id`, `args`? | Custom **Rust** processor registered in your app      |

- `embedded` is only available when Apate is embedded in your own Rust application
  (see §7 and the `PostProcessor` trait). It references a processor registered by `id`.
- A processor that returns a **Blob** replaces the body for all following processors.
- A processor that returns **`()`** (no value) leaves the body unchanged.
- Any other return type is an error (→ HTTP 500).
- Processors are cumulative: deceit-level processors run first, then response-level ones;
  each receives the body produced by the previous processor.

`rhai_ref` in a **processor list** uses the same flat form as matchers (NOT the output
form above):

```toml
[[deceit.processors]]
type = "rhai_ref"
id = "script-id"
args = ["arg1", "arg2"]   # optional
```
### 2.6 Rhai script registry (TOML: `[[rhai]]`)

Named, reusable Rhai scripts referenced by `id` from matchers, outputs, and processors
(via `rhai_ref`).

```toml
[[rhai]]
id = "my-script"
script = """
// your Rhai code here
"""
```

The `id` is used by `rhai_ref` matchers/outputs/processors. Each reference may pass
`args` (array of strings) to the script.

---

## 3. Jinja (minijinja) templates API

Used when a response `type = "jinja"`. The engine is **minijinja**, which implements
Jinja2-compatible syntax. Core syntax an agent needs:

| Construct | Example |
|-----------|---------|
| Variable | `{{ ctx.path }}` |
| Condition | `{% if cond %}...{% elif x %}...{% else %}...{% endif %}` |
| Loop | `{% for k in ctx.load_query_args() %}{{ k }}{% endfor %}` |
| Assign | `{% set name = ctx.load_path_args().id %}` |
| Comment | `{# ignored #}` |
| Whitespace trim | `{%- ... -%}` / `{{- ... -}}` |
| Attribute / index | `obj.field`, `obj["field"]` |
| Filter | `{{ value \| tojson }}`, `{{ value \| default('x') }}`, `{{ value \| upper }}` |
| Built-in test | `{% if value is defined %}...{% endif %}` |

String concatenation uses `~`, e.g. `{{ "id-" ~ ctx.path }}`. Arithmetic and comparisons
follow Python-like operators (`+ - * / %`, `== != < > <= >=`, `and or not`).

### 3.1 Global functions (no receiver)

| Function                              | Returns                                   |
|---------------------------------------|-------------------------------------------|
| `random_num()`                        | random number as **string**                |
| `random_num(max)`                     | random number in `[0, max)` as string      |
| `random_num(from, to)`                | random number in `[from, to)` as string    |
| `random_hex()`                        | random HEX string (32 bytes → 64 chars)    |
| `random_hex(bytes_len)`               | random HEX string of `bytes_len` bytes     |
| `uuid_v4()`                           | a random UUID v4 (string)                  |
| `set_response_code(code)`             | sets the response status code             |

> All `random_*`/`uuid_v4` return **strings** (not numbers) in Jinja.

### 3.2 `ctx` object (request/response context)

Exposed under the variable name `ctx`.

**Readable properties:**
- `ctx.method` → HTTP method of the request (e.g. `"GET"`)
- `ctx.path` → the full request path (e.g. `"/user/740"`)

**Methods:**
- `ctx.load_headers()` → map of request headers, **lowercase keys**
- `ctx.load_query_args()` → map of URL query arguments
- `ctx.load_path_args()` → map of path arguments from URI pattern (`/user/{user_id}`)
- `ctx.load_body_string()` → request body as a string (empty if no body)
- `ctx.load_body_json()` → request body parsed as JSON. Empty body → `null`; invalid JSON
  raises a template error (→ HTTP 500)
- `ctx.inc_counter("key")` → returns the **previous** counter value for `key`, then
  increments it by 1 (first call returns `0`). Counters are shared server-wide per key.
- `ctx.set_response_code(code)` → **sets** the response status code (e.g. `ctx.set_response_code(503)`)

> `set_response_code(code)` is also available as a global function; both forms are equivalent.

### 3.3 Jinja examples

```jinja
{# echo a request path argument #}
{"id": "{{ ctx.load_path_args().user_id }}"}

{# read a JSON request body field #}
{%- set input = ctx.load_body_json() %}
{ "name": "{{ input.name }}", "surname": "{{ input.surname }}" }

{# conditional + set response code #}
{%- if ctx.load_query_args().id %}
    {%- set id = ctx.load_query_args().id %}
{%- else %}
    {%- set _ = ctx.set_response_code(500) %}
{%- endif %}
{"id":"{{ id }}"}

{# random values #}
"token": "{{ random_hex(16) }}", "id": "{{ random_num(100, 999) }}", "uuid": "{{ uuid_v4() }}"
```

---

## 4. Rhai script API

Rhai is used in three roles, each with a different **context** and **return contract**.
The language is standard Rhai (C-style syntax, `let`, `if`, `for`, closures, arrays, maps).

| Construct | Example |
|-----------|---------|
| Variable | `let x = 1;` (immutable) / `let x = 1; x = 2;` |
| Map literal | `#{ name: "John", age: 42 }` — **`#` is required** |
| Array literal | `[1, 2, 3]` |
| Condition | `if cond { ... } else { ... }` |
| Loop | `for item in array { ... }` / `while cond { ... }` |
| Function | `fn add(a, b) { a + b }` |
| Closure | `array.filter(\|v\| v > 1)` |
| String template | `` `id: ${value}` `` |
| Null/absent | `()` (unit); use `??` for defaults, e.g. `storage_read("k") ?? []` |

### 4.1 Where Rhai is used

| Role            | Triggered by                              | `ctx` type        | Must return            |
|-----------------|-------------------------------------------|-------------------|------------------------|
| **Matcher**     | `type = "rhai"` / `"rhai_ref"` in matchers | `RhaiRequestContext`  | a **boolean** |
| **Output**      | `type = "rhai"` / `type = { rhai_ref = ... }` on response | `RhaiResponseContext` | a **Blob** (body) or `()` (empty body) |
| **Processor**   | `type = "rhai"` / `"rhai_ref"` in processors | `RhaiResponseContext` | a **Blob** (new body) or `()` (keep body) |

### 4.2 Scope variables available inside a Rhai script

| Variable | Matcher | Output | Processor | Description                                        |
|----------|---------|--------|-----------|----------------------------------------------------|
| `ctx`    | ✅      | ✅     | ✅        | context object (see §4.3 / §4.4)                    |
| `args`   | ✅      | ✅     | ✅        | array of strings; **empty for inline `rhai`**, populated for `rhai_ref` |
| `body`   | ❌      | ❌     | ✅        | the current response body as a **Blob**             |

> **Important:** `args` is only non-empty when you reference a registry script via
> `rhai_ref` with an `args` list. Inline `rhai` scripts always see `args = []`.

### 4.3 `RhaiRequestContext` — `ctx` in **matchers**

| Member                        | Returns                              |
|-------------------------------|--------------------------------------|
| `ctx.method`                  | request method (string)              |
| `ctx.path`                    | full request path (string)           |
| `ctx.load_headers()`          | map of request headers (**lowercase keys**) |
| `ctx.load_query_args()`       | map of URL query arguments           |
| `ctx.load_path_args()`        | map of path arguments                |
| `ctx.load_body()`             | request body as **Blob**             |

### 4.4 `RhaiResponseContext` — `ctx` in **outputs** and **processors**

Everything in §4.3, **plus**:

| Member                        | Returns / effect                        |
|-------------------------------|-----------------------------------------|
| `ctx.response_code`           | get **and set** the response status code |
| `ctx.inc_counter("key")`      | returns **previous** counter value, then increments (first call → `0`) |

### 4.5 Global functions (available in all Rhai contexts)

| Function                            | Returns                                  |
|-------------------------------------|-------------------------------------------|
| `random_num()`                      | random `i64`                               |
| `random_num(max)`                   | random `i64` in `[0, max)`                 |
| `random_num(from, to)`              | random `i64` in `[from, to)`               |
| `random_hex()`                      | random HEX string (32 bytes)               |
| `random_hex(bytes_len)`             | random HEX string of `bytes_len` bytes     |
| `uuid_v4()`                         | random UUID v4 (string)                    |
| `to_json_blob(value)`               | serialize any Rhai value → **Blob** (JSON) |
| `from_json_blob(blob)`              | deserialize **Blob** (JSON) → Rhai value   |
| `storage_read(key)`                 | read a value from the global in-memory KV store; absent → `()` (unit) |
| `storage_write(key, value)`         | write a value to the global in-memory KV store |

**Rhai `serde` extras** (enabled by Apate's `rhai` features, available everywhere):
- `parse_json(string)` → Rhai map/array/value
- `map.to_json()` → JSON string (maps only)
- `string.to_blob()` → Blob (UTF-8 bytes)
- `blob.as_string()` → string (lossy UTF-8)
- For arrays/other values use `to_json_blob(value)` to get a Blob.

> **Storage** is an in-memory key/value store that persists across requests for the
> lifetime of the server (shared by all Rhai scripts). It is the primary way to mimic
> database / stateful behaviour. Values are stored as JSON-serialized Rhai values.
> Storage is **not** cleared by `/apate/specs/*` updates.

### 4.6 Logging

Rhai scripts can log via the standard Rhai macros:
- `print("...")` / `print!("...")` → logged at **info** level with prefix `RHAI:`
- `debug("...")` / `debug!("...")` → logged at **debug** level with source position

### 4.7 Rhai errors

- Matcher script fails to compile or throws → matcher evaluates to **`false`** (logged).
- Output script error, or a non-Blob/non-unit return → **HTTP 500**.
- Processor script error, or a non-Blob/non-unit return → **HTTP 500**.
- Missing `rhai_ref` id → matcher `false`; output/processor → HTTP 500.

### 4.8 Rhai examples

**Matcher (returns boolean):**
```rhai
if ctx.method == "GET" { return true; }
let q = ctx.load_query_args();
if q.foo == "none" { return false; }
return true;
```

**Output template (returns Blob):**
```rhai
let data = [
    #{ name: "Ivan", surname: "Ivanov" },
    #{ name: "John", surname: "Smith" },
];
let q = ctx.load_query_args();
if "name" in q { data = data.filter(|r| r.name == q["name"]); }
return to_json_blob(data);
```

**Processor (rewrites body, uses `body`, `ctx`, `args`):**
```rhai
let map = from_json_blob(body);
map.post = "hello from post processor";
map.args = args;
return map.to_json().to_blob();
```

**Stateful (mimic a DB via storage):**
```rhai
let users = storage_read("users") ?? [];
let rec = from_json_blob(ctx.load_body());
rec.id = ctx.inc_counter("user_id") + 1;
users.push(rec);
storage_write("users", users);
return to_json_blob(rec);
```

---

## 5. Rhai language cheat-sheet

> Imported as-is from the Rhai project cheat-sheet. Applies to all Rhai scripts in Apate.

For AI agents writing or reviewing Rhai scripts. Rhai is JavaScript-like plus certain Rust-inspired syntax.
**Start from JavaScript, then apply the deltas below.**
Full reference: [_The Rhai Book_](https://rhai.rs/book/ref).

### JS → Rhai deltas (read first)

| JavaScript habit | Rhai reality |
|---|---|
| `{a: 1}` object literal | `#{a: 1}` (object map) |
| `null` / `undefined` | single value `()`; absent map key reads as `()` |
| truthiness (`if (x)`) | conditions must be `bool` |
| `===` / `==` coercion | only `==`; no coercion; `1` (`i64`) and `1.0` (`f64`) are distinct types |
| objects/arrays are references | **no references exist**: values are cloned on assign and on call; mutate via the variable path |
| `function f(){}` sees globals/outer scope | `fn` cannot see outer variables; closures `\|x\| ...` capture (shared) |
| `arr.length`, `.forEach`, `.includes` | `arr.len()`, `for` loop, `.contains()` |
| `console.log` | `print(x)` / `debug(x)` |
| `Math.floor(x)` | `x.floor()`; `PI()`; `x.sin()` etc. |
| `for (let i=0;i<n;i++)` | `for i in 0..n` (exclusive) / `0..=n` (inclusive) |
| `if (c) stmt` | `if c { ... }` braces mandatory, no parentheses |
| `return` required | last statement is always the value of the block (not just functions), with or without trailing `;` (different from Rust) |
| Blocks <> expressions | every block is an expression (the last statement is the value) |
| `throw`/`try`/`catch (e)` | same; `throw` takes any value |
| `class`, `async`, `await`, `new` | none; use maps + closures, or host-registered types |
| `import x from` | `import "name" as x; x::f()` |
| `'a'` is a string | `'a'` is a `char`; strings use `"..."` or `` `...${x}...` `` |
| `s.length`, UTF-16 indexing | `s.len()` counts chars; `s[i]` is a `char` (different from Rust where strings are UTF-8 encoded bytes) |
| `x = a ? b : c` | `x = if a { b } else { c }` |

When unsure whether a method exists, check the host's registered functions instead of guessing JS names.

### Values

```rhai
let count = 0;                  // mutable, dynamically typed
const LIMIT = 10;
let u;                          // ()
let items = [10, 20, 30,];      // trailing comma ok; items[-1] == 30
let opts = #{mode: "fast", "odd-key": true};
opts.mode = "safe";
opts["odd-key"];                // brackets for non-identifier keys
"mode" in opts;                 // key test
opts.timeout ?? 30;             // ?? defaults when left is ()
opts?.a?.b;                     // ?. short-circuits on ()
x = if a { b } else { c };      // if statement used as expression, similar to x = a ? b : c;
```

- Integer literals: decimal, `0b`, `0o`, `0x`, `_` separators. Default int is `i64`.
- Out-of-range array reads are errors. Unsupported operations are errors, not coercions.
- Blocks, `if`, `switch`, `while`, `loop`, `for` are expressions; `break value` yields from a loop.
- Mismatched non-numeric `==` is `false`; mixed numeric types may fail: convert with `to_int()`/`to_float()`.
- `&&`/`||` short-circuit; single `&`/`|` on booleans evaluate both sides.

### IMPORTANT: value semantics

```rhai
let a = [1, 2];
let b = a;                      // clone
b[0] = 99;                      // a unchanged
fn f(x) { x[0] = 99; }
f(a);                           // a unchanged

a[0] = 7;                       // edits a
let user = #{s: #{theme: "light"}};
user.s.theme = "dark";          // edits nested property in place
let s = user.s;                 // clones
```

- Rhai has no reference type: scripts cannot hold, pass, or return a reference to a value. The only
  in-place mutation is through a variable-rooted path or a method call on a receiver (`this`).
  To refer to data indirectly, pass an index, key, or ID and resolve it at the point of mutation.
- Property/index paths rooted at a variable mutate in place (built-in arrays, BLOBs, maps only).
  Custom-type getters and function results do not.
- **Prefer built-in types in scripts.** Arrays, maps and BLOBs edit in place; custom types are
  cloned and set back through getters/setters. Keep script data in arrays/maps and convert
  to/from custom types at the edge (host functions or `serde`).
- `take(x)` moves a value out (source becomes `()`); needs the standard package.
- Strings are immutable values; string operations return new strings.

### Control flow

```rhai
for item in items { if item == 20 { continue; } if item > 30 { break; } }
for (item, i) in items { }      // with index
let r = if ok { "go" } else { "wait" };   // omitted else yields ()
let n = 0; while n < 3 { n += 1; }
loop { break; }
```

`switch` exists (`switch x { 1 => ..., 2 | 3 => ..., _ => ... }`); see the reference for ranges and guards.

### Functions, closures, pointers

```rhai
fn add(a, b) { a + b }                  // global-level only, no nesting, no capture
let offset = 2;
let f = |n| n + offset;                 // closure: captures SHARED variables
f.call(3);                              // call closures/pointers with .call()
let g = add;                            // fn pointer (same as Fn("add"))
g.call(1, 2);
```

- Function pointers store only a name; they cannot name functions inside modules.
- Loop closures all see the same shared captured variable.
- Do not use a captured variable as both capture and `this` receiver (data-race error; deadlock under `sync`).

### `this` and methods

```rhai
fn increase(amount) { this += amount; }
let score = 10;
score.increase(5);                      // score == 15; plain increase(5) errors
fn int.double() { this * 2 }            // restrict receiver type (match type_of())
let c = #{v: 0, inc: || this.v += 1};   // closures in map props get `this`
c.inc();
```

### Strings

`"escaped"`, `` `interpolated ${x}, no escapes` ``, `#"raw \ string"#`, `'c'` (char).

### Modules and host dependence

`import "m" as m; m::f();` `export` marks items from a module script.

Built-ins depend on the host's registered packages and features (`no_index`, `no_object`,
`no_function`, `no_closure`, `no_module`, ...). If a built-in seems missing, check the embedding.

### Performance style

Calls and module lookups are not inlined or flattened: avoid deep module trees and trivial
wrapper functions; inline simple logic.

---

## 6. DSL usage examples

Full, runnable spec files live in `examples/`. Reference them when in doubt:

| File | Demonstrates |
|------|--------------|
| `examples/apate-specs.toml` | matchers (method/query/path/json, `and`/`or`), Jinja templates, multi-response fallback |
| `examples/apate-template-specs.toml` | Jinja context (`ctx`), template functions, `set_response_code`, counters |
| `examples/apate-specs-rhai.toml` | Rhai as matcher, output, processor, and reusable registry scripts |
| `examples/apate-specs-bin.toml` | binary responses via `base64` and `hex` |
| `examples/apate-specs-app.toml` | stateful behaviour via `storage_read`/`storage_write`, `rhai_ref` output |
| `examples/processors.rs` | embedded Rust `PostProcessor` (custom signing logic) |

### 5.1 Minimal spec (one static JSON endpoint)

```toml
[[deceit]]
uris = ["/user/check"]
headers = [["Content-Type", "application/json"]]
matchers = [{ type = "method", eq = "POST" }]

[[deceit.responses]]
output = """
{"message":"Success"}
"""
```

### 5.2 Multiple responses selected by query arg, with a fallback

```toml
[[deceit]]
uris = ["/user/list"]
matchers = [{ type = "method", eq = "GET" }]

[[deceit.responses]]
matchers = [{ type = "query_arg", name = "name", value = "Ivan" }]
output = "[{\"id\":42,\"name\":\"Ivan\"}]"

[[deceit.responses]]
matchers = [{ type = "query_arg", name = "name", value = "Rajesh" }]
code = 503
output = "{\"message\":\"Impossible to list them all\"}"

[[deceit.responses]]
# no matchers -> fallback (always matches)
output = "[{\"id\":42,\"name\":\"Ivan\"},{\"id\":740,\"name\":\"Adolph\"}]"
```

### 5.3 Jinja template that echoes the request

```toml
[[deceit]]
uris = ["/echo/{name}"]
headers = [["Content-Type", "application/json"]]

[[deceit.responses]]
type = "jinja"
output = """
{"name": "{{ ctx.load_path_args().name }}",
 "query": {{ ctx.load_query_args() | tojson }},
 "id": "{{ uuid_v4() }}"}
"""
```

### 5.4 Rhai output that filters a dataset by query args

```toml
[[deceit]]
uris = ["/rhai/list"]
headers = [["Content-Type", "application/json"]]

[[deceit.responses]]
type = "rhai"
output = """
let data = [
    #{ name: "Ivan", surname: "Ivanov" },
    #{ name: "John", surname: "Smith" },
];
let q = ctx.load_query_args();
if "name" in q    { data = data.filter(|r| r.name == q["name"]); }
if "surname" in q { data = data.filter(|r| r.surname == q["surname"]); }
return to_json_blob(data);
"""
```

### 5.5 Stateful "add then list" using the storage KV

```toml
[[deceit]]
uris = ["/app/user/add"]
matchers = [{ type = "method", eq = "POST" }]
headers = [["Content-Type", "application/json"]]

[[deceit.responses]]
type = "rhai"
output = """
let users = storage_read("users") ?? [];
let rec = from_json_blob(ctx.load_body());
rec.id = ctx.inc_counter("user_id") + 1;
users.push(rec);
storage_write("users", users);
return to_json_blob(rec);
"""

[[deceit]]
uris = ["/app/user/list"]
matchers = [{ type = "method", eq = "GET" }]

[[deceit.responses]]
type = "rhai"
output = """
return to_json_blob(storage_read("users") ?? []);
"""
```

### 5.6 Reusable script shared by a matcher and a processor

```toml
[[deceit]]
uris = ["/guarded"]

[[deceit.matchers]]
type = "rhai_ref"
id = "require-admin"
args = ["secret-token"]

[[deceit.responses]]
output = "{\"ok\":true}"

[[deceit.processors]]
type = "rhai_ref"
id = "stamp-args"

[[rhai]]
id = "require-admin"
script = """
let h = ctx.load_headers();
return "x-token" in h and h["x-token"] == args[0];
"""

[[rhai]]
id = "stamp-args"
script = """
let m = from_json_blob(body);
m.args = args;
return m.to_json().to_blob();
"""
```

### 5.7 Binary response (PNG) via base64

```toml
[[deceit]]
uris = ["/file/base64.png"]
headers = [["Content-Type", "image/png"]]

[[deceit.responses]]
type = "base64"
output = "iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAMAAACdt4Hs..."  # base64 payload
```

### 5.8 Error and edge cases

```toml
# 404 is automatic: no deceit matches path/matchers/response matchers.
# Body: "Nothing can handle your requiest with path: <path>"

[[deceit]]
uris = ["/only-get"]
matchers = [{ type = "method", eq = "GET" }]
[[deceit.responses]]
output = "ok"
# POST /only-get -> falls through -> 404

[[deceit]]
uris = ["/not-debug"]
matchers = [{ type = "query_arg", name = "debug", value = "1", negate = true }]
[[deceit.responses]]
output = "normal"
# ?debug=1 -> 404; any other query -> 200 "normal"

[[deceit]]
uris = ["/broken"]
[[deceit.responses]]
type = "base64"
output = "not-base64!"
# invalid payload -> 500 "It happened! <error>"
```

| Condition | Response |
|-----------|----------|
| No path / matcher / response match | `404` plain text |
| Output render error (bad base64/hex, template error, Rhai error) | `500` `It happened! <error>` |
| Processor error or non-Blob processor return | `500` `Can't apply post processors! <error>` |
| `code` valid and non-zero | that status code |
| no `code` / `code` invalid (e.g. `0`) | `200` |

---

## 7. Using Apate as a Rust test library

Apate is a regular library. You start an in-process HTTP server inside your test, call it
with any HTTP client, and it is automatically shut down when the server handle is dropped.

### 7.1 Cargo dependency

```toml
[dev-dependencies]
apate = "0.2"          # test-only dependency
reqwest = { version = "0.12", features = ["blocking", "json"] }
serial_test = "3"      # to serialize tests that share the default port
```

### 7.2 Public API (module paths)

| Item | Path |
|------|------|
| `ApateConfig` | `apate::ApateConfig` |
| `ApateSpecs` | `apate::ApateSpecs` |
| `ApateConfigBuilder` | `apate::ApateConfigBuilder` |
| `apate_server_run(config)` (async) | `apate::apate_server_run` |
| `apate_init_server_config(port: Option<u16>, log: Option<String>, files: Vec<String>)` | `apate::apate_init_server_config` |
| `DEFAULT_PORT` (= 8228) | `apate::DEFAULT_PORT` (also `apate::test::DEFAULT_PORT`) |
| `DEFAULT_RUST_LOG` | `apate::DEFAULT_RUST_LOG` (also `apate::test::DEFAULT_RUST_LOG`) |
| `Deceit`, `DeceitResponse` | `apate::deceit::{Deceit, DeceitResponse}` |
| `DeceitBuilder` | `apate::deceit::DeceitBuilder` |
| `DeceitResponseBuilder` | `apate::deceit::DeceitResponseBuilder` |
| `DeceitResponseContext` | `apate::deceit::DeceitResponseContext` |
| `Matcher` | `apate::matchers::Matcher` |
| `OutputType` | `apate::output::OutputType` |
| `Processor`, `ApateProcessor`, `PostProcessor` (trait) | `apate::processors::{Processor, ApateProcessor, PostProcessor}` |
| `ApateTestServer` | `apate::test::ApateTestServer` |
| `init_env_logger(log)` | `apate::test::init_env_logger` |

`ApateConfig` fields: `port: u16`, `specs: ApateSpecs`, `processors: HashMap<String, ApateProcessor>`.
`ApateConfig::default()` uses port `8228` and empty specs/processors.

### 7.3 `ApateTestServer`

```rust
apate::test::ApateTestServer::start(config: ApateConfig, delay_ms: usize) -> ApateTestServer
```

- Starts the server on `config.port` (default `8228`); **panics** if the port cannot be bound.
- `delay_ms` — a short sleep after start for slow environments (use `0` or `1` in CI).
- **Synchronous API** — works in both `#[test]` and `#[tokio::test]`.
- **Auto-cleanup**: the server is stopped when the returned value is dropped. Bind it to
  a variable that lives for the duration of the test, e.g. `let _apate = ...;`.
- The server runs on its own OS thread with its own runtime, independent of the test's runtime.

### 7.4 Simplest test (builder style, sync)

```rust
use apate::deceit::{DeceitBuilder, DeceitResponseBuilder};
use apate::test::ApateTestServer;

#[test]
fn my_api_test() {
    let config = DeceitBuilder::with_uris(&["/user/check"])
        .require_method("POST")
        .add_header("Content-Type", "application/json")
        .add_response(
            DeceitResponseBuilder::default()
                .code(200)
                .with_output(r#"{"message":"Success"}"#)
                .build(),
        )
        .to_app_config();          // wraps the single Deceit into an ApateConfig

    // Server stops automatically when `_apate` is dropped at end of test.
    let _apate = ApateTestServer::start(config, 0);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post("http://localhost:8228/user/check")
        .send()
        .expect("request failed");

    assert_eq!(resp.status(), 200);
    assert_eq!(resp.headers().get("Content-Type").unwrap(), "application/json");
    assert_eq!(resp.json::<serde_json::Value>().unwrap()["message"], "Success");
}
```

### 7.5 Builder reference

`DeceitBuilder` (entry point `DeceitBuilder::with_uris(&[...])`):
- `add_header(key, value)`, `add_processor(Processor)`, `add_matcher(Matcher)`
- `require_method(m)`, `require_header(k,v)`, `require_query_arg(n,v)`,
  `require_path_arg(n,v)`, `require_json_match(path, eq)`
- `with_matchers(Vec<Matcher>)`, `add_response(DeceitResponse)`, `with_responses(Vec<_>)`
- `build() -> Deceit`
- `to_app_config() -> ApateConfig` (default port) / `to_app_config_with_port(port)`

`DeceitResponseBuilder` (entry point `DeceitResponseBuilder::default()`):
- `code(u16)`, `add_header(k,v)`, `add_processor(Processor)`, `add_matcher(Matcher)`
- `with_output(&str)`, `with_output_type(OutputType)`
- `require_*` helpers (same as `DeceitBuilder`)
- `build() -> DeceitResponse`

`ApateConfigBuilder` (for multiple deceits / custom processors / named scripts):
- `with_port(u16)`
- `add_deceit(Deceit)`
- `register_processor(ApateProcessor)`
- `add_script(id, script)` (adds to the `[[rhai]]` registry)
- `build() -> ApateConfig`

### 7.6 Multiple deceits + custom port (async test)

```rust
use apate::ApateConfigBuilder;
use apate::deceit::{DeceitBuilder, DeceitResponseBuilder};
use apate::test::ApateTestServer;

#[tokio::test]
async fn multi_endpoint_test() {
    let config = ApateConfigBuilder::default()
        .with_port(9321)
        .add_deceit(
            DeceitBuilder::with_uris(&["/user/add"])
                .require_method("POST")
                .add_response(
                    DeceitResponseBuilder::default()
                        .code(200)
                        .with_output(r#"{"message":"Success"}"#)
                        .build(),
                )
                .build(),
        )
        .add_deceit(
            DeceitBuilder::with_uris(&["/user/{id}"])
                .require_method("GET")
                .add_response(
                    DeceitResponseBuilder::default()
                        .code(200)
                        .add_header("Content-Type", "application/json")
                        .with_output_type(apate::output::OutputType::Jinja)
                        .with_output(r#"{"id":"{{ ctx.load_path_args().id }}"}"#)
                        .build(),
                )
                .build(),
        )
        .build();

    let _apate = ApateTestServer::start(config, 0);
    let client = reqwest::Client::new();

    let r = client.get("http://localhost:9321/user/1133").send().await.unwrap();
    assert_eq!(r.json::<serde_json::Value>().await.unwrap()["id"], "1133");
}
```

### 7.7 Registering a custom Rust `PostProcessor`

Embed Apate in your application and register a `PostProcessor` via
`ApateConfigBuilder::register_processor`. Reference it from TOML/DSL with an
`embedded` processor (`id` + optional `args`).

```rust
use apate::processors::{ApateProcessor, PostProcessor, Processor};
use apate::deceit::DeceitResponseContext;

struct MyProcessor;

impl PostProcessor for MyProcessor {
    fn process(
        &self,
        _args: &[&str],                    // values from Processor::Embedded.args
        _ctx: &DeceitResponseContext,      // request + response context
        body: &[u8],                       // already rendered body
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>> {
        Ok(Some(body.to_vec()))            // Some = replace; None = keep
    }
}

let config = ApateConfigBuilder::default()
    .register_processor(ApateProcessor::post("my_id", Box::new(MyProcessor)))
    .build();
```

`PostProcessor` requires `Sync + Send`. Panics inside a processor are not caught.

### 7.8 Test authoring notes (gotchas)

- **Port conflicts:** all tests default to port `8228`. If you run many tests in parallel,
  either give each test a **distinct port** (via `with_port` / `to_app_config_with_port`)
  or annotate with `#[serial]` from the `serial_test` crate (the repo's own tests use
  `#[serial]`).
- The server is bound to `0.0.0.0:PORT`, so `http://localhost:PORT` always works.
- To enable verbose Apate logging in a test: `apate::test::init_env_logger("debug,apate=trace");`
  (`init_env_logger("")` is a no-op).
- The test server does **not** require an async runtime; it spawns its own.
- For a full working reference, see `tests/test-api.rs`, `tests/scripting.rs`,
  and `tests/processors.rs`.

---

## 8. Running the Apate server

### 8.1 Install & run locally (CLI)

Install the `apate` binary from crates.io, then run it directly:

```sh
cargo install apate            # puts the `apate` binary on your $PATH
apate                          # start on default port 8228, no specs
apate -p 8228 ./spec.toml      # start on port 8228 with a TOML spec file
```

CLI arguments (higher priority than env vars) and env configuration: see §8.3.

### 8.2 Run from Docker image

Official image: `ghcr.io/rustrum/apate`. The container runs the `apate` binary, listens on
port **8228**, and starts with **no specs** (add them via UI or API). Use tag `latest` for this version.

#### 7.2.1 Run an empty server

```sh
docker run --rm -t -p 8228:8228 ghcr.io/rustrum/apate:latest
```

#### 7.2.2 Run with mounted TOML specs

Mount your specs and expose their path(s) through `APATE_SPECS_FILE_*` env variables:

```sh
# from the apate repo root (uses ./examples as the specs dir)
docker run --rm -t -p 8228:8228 \
  -v "$(pwd)/examples:/specs" \
  -e APATE_SPECS_FILE_1=/specs/apate-specs.toml \
  -e APATE_SPECS_FILE_2=/specs/apate-specs-rhai.toml \
  ghcr.io/rustrum/apate:latest
```

> **Any** env variable whose name starts with `APATE_SPECS_FILE` is treated as a path to a
> spec file. You can therefore register many specs with
> `APATE_SPECS_FILE_1`, `APATE_SPECS_FILE_2`, …, `APATE_SPECS_FILE_N`.

### 8.3 Server configuration

**Env variables:**
- `RUST_LOG`, `RUST_LOG_STYLE` — logging (e.g. `RUST_LOG=info,apate=debug`)
- `APATE_PORT` — server port (default `8228`). **Invalid/non-numeric value panics.**
- `APATE_SPECS_FILE*` — spec file paths (see above). Order across env vars is
  filesystem/OS-dependent; use numbered suffixes only for convenience, not for ordering.

**CLI arguments (higher priority than env):**
```sh
apate -p 8080 -l warn ./spec.toml ./another.toml
#   -p <port>      port to listen on
#   -l <filter>    env_logger filter string (e.g. warn, info, debug, "info,apate=debug")
#   positional args = paths to spec files
```

Spec load order: **all positional CLI files first**, then all `APATE_SPECS_FILE*` files;
each file is parsed and appended to the specs. A missing file or invalid TOML aborts startup.

### 8.4 Web UI & admin REST API

Base path: `http://HOST:PORT/apate` (e.g. `http://localhost:8228/apate`).

| Method | Path | Response |
|--------|------|----------|
| `GET`  | `/apate` | the web UI (`index.html`, SPA fallback for any sub-path) |
| `GET`  | `/apate/assets/{filename}` | embedded static asset (404 if absent) |
| `GET`  | `/apate/info` | JSON `{"name":"Apate API mocking server","version":"0.2.1"}` |
| `GET`  | `/apate/specs` | current specs as TOML (`Content-Type: text/x-toml`) |
| `POST` | `/apate/specs/replace` | replace all specs with TOML in request body → `200 "Specification replaced"` |
| `POST` | `/apate/specs/append` | append TOML specs after existing → `200 "New specification appended to the existing one"` |
| `POST` | `/apate/specs/prepend` | prepend TOML specs before existing → `200 "New specification prepended to the existing one"` |

Example live spec update:
```sh
curl -X POST http://localhost:8228/apate/specs/replace -d @./new-specs.toml
curl http://localhost:8228/apate/specs        # dump current specs as TOML
curl http://localhost:8228/apate/info         # {"name":"Apate API mocking server","version":"0.2.1"}
```

All `POST` spec endpoints accept a TOML document (the same shape as §2) in the request body.
Invalid TOML → **`400 Bad Request`** with `Failed to parse TOML from request body: <error>`.
The Jinja template cache and Rhai AST cache are cleared and rebuilt on every update, so
changes take effect immediately. Rhai **storage** is not cleared.

> Admin and MCP routes exist only when the `server` feature is enabled (default).

### 8.5 MCP API (`/mcp`) — tools only

Endpoint: `POST http://HOST:PORT/mcp` (JSON-RPC 2.0; always HTTP `200`, errors in the JSON
body). Protocol version: `2025-06-18`. Batch requests are not supported.

Two tools are exposed by `tools/list`:

| Tool | Input | Output |
|------|-------|--------|
| `specs_get` | none (`arguments: {}`) | current specs as TOML text |
| `specs_replace` | `{"toml": "<full TOML>"}` (required) | stored specs as TOML text |

Call shape and result:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call",
 "params":{"name":"specs_replace","arguments":{"toml":"[[deceit]]\nuris=[\"/ping\"]\n"}}}
```

```json
{"jsonrpc":"2.0","id":1,
 "result":{"isError":false,"content":[{"type":"text","text":"[[deceit]]\nuris = [\"/ping\"]\n"}]}}
```

| Error case | JSON-RPC code |
|------------|---------------|
| invalid JSON | `-32700` |
| batch array / bad envelope / wrong `jsonrpc` | `-32600` |
| missing `toml`, unknown tool, invalid TOML | `-32602` |
| unknown method | `-32601` |
| unexpected internal failure | `-32603` |

A malformed TOML passed to `specs_replace` is a protocol error (`-32602`), not an
`isError` result. Successful `specs_replace` also clears Jinja/Rhai caches.

---

## 9. Important notes for AI agent

### 9.1 Quick decision guide

- **Static / fixed response** → `type = "string"` (or omit `type`).
- **Echo request data / light conditionals / randoms** → `type = "jinja"`.
- **Complex logic, data filtering, stateful behaviour** → `type = "rhai"` (or registry `rhai_ref`).
- **Return binary bytes** → `type = "base64"` or `type = "hex"`.
- **Modify the body after render** → add a `processor` (`rhai`/`rhai_ref`/`embedded`).
- **Share a script across endpoints** → define it once in `[[rhai]]`, reference by `id`.
- **Unit-test a client against a local API** → use `ApateTestServer::start(config, 0)`.
- **Custom logic depending on Rust libraries** → should embed Apate in your app (custom server) and register a
  `PostProcessor`, reference with `type = "embedded"`.

### 9.2 DSL syntax tweaks & gotchas

TOML triple-quoted strings are multiline and raw. No need to escape quotes inside them.
Leading/trailing newlines are part of the string:

```toml
script = """
let a = 1; // line number 2
let b = 2; // line number 3
"""
```

This script has 4 lines because the opening/closing `"""` are on their own lines. Consider
this when reading Rhai/Jinja error line numbers and when whitespace matters in JSON bodies.

Rhai object maps use the `#{ ... }` prefix: `#{ name: "John", surname: "Smith" }`.

### 9.3 Corner cases that commonly cause wrong specs

| Situation | Correct behavior |
|-----------|------------------|
| Several deceits match | first deceit in file order wins |
| Empty `matchers` on a response | always matches; place it **last** or later responses are unreachable |
| Header matcher | `key` must be lowercase; value comparison is exact/case-sensitive |
| Method matcher | `eq` comparison is case-insensitive |
| JSON matcher on `"id": 42` | fails — only JSON **string** values compare with `eq` (use `"42"`) |
| Repeated query arg `?a=1&a=2` | collapses to one value |
| No `code` on a response | `200` |
| `code = 0` | treated as unset → `200` |
| `set_response_code` / `ctx.response_code` | overrides `code` from the response |
| Rhai output returns `()` | response body is empty (not the `output` text) |
| Rhai processor returns `()` | body stays unchanged |
| Rhai script returns a non-Blob in output/processor | HTTP 500 |
| `rhai_ref` as response output | `type = { rhai_ref = { id = "...", args = [...] } }` |
| `rhai_ref` as matcher/processor | flat: `type = "rhai_ref"` + `id` + optional `args` |
| Missing `rhai_ref` id | matcher → false; output/processor → HTTP 500 |
| Counters | first call returns `0`; shared server-wide; survive spec updates |
| Rhai storage | server-wide; survives spec updates; not cleared by admin API |
| `ctx.path` (Jinja/Rhai) | full request path, including path args |
| `ctx.load_headers()` | keys are lowercase |
| Request body empty | `load_body_json()` → `null`; `load_body()` → empty Blob; `load_body_string()` → empty |
| Invalid JSON body + `load_body_json()` | Jinja: template error (500); JSON matcher: false |
| Spec update via API | Jinja + Rhai caches cleared; Rhai storage kept |
| Multiple specs from files | CLI files first, then `APATE_SPECS_FILE*`; all appended |
| Binding failure / invalid `APATE_PORT` | process panics/aborts at startup |
| Keep-alive | disabled by the server |

> **Triage:** `404` means no deceit/response matched; `500` means DSL/render/processor
> error. Check logs (`RUST_LOG=debug,apate=debug`) for the underlying message.


