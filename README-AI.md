# Apate — AI Agent Reference

Project version: 0.1.2
Git: b6f5384e6ba1082cb04ef1d08a2e8e7b4c249750

> This document is written **for AI agents** (and any tool that consumes it). It is a precise,
> unambiguous reference of the Apate project: its DSL (TOML specification), its scripting
> APIs (Jinja/minijinja and Rhai), its Rust test-library API, and how to run it as a
> Docker image. Prefer this file over `README.md` when generating code or configuration,
> because it reflects the **actual** code behaviour. Where this document and `README.md`
> disagree, **this document is correct**.

---

## 1. Basic overview

**Apate** is an API prototyping / mocking server written in Rust. Its purpose is to help
with integration and end-to-end testing by returning predictable, pre-defined HTTP
responses for a set of URIs (paths). It is named after Apate, the Greek goddess of deceit.

Two consumption modes exist, both from the **same crate** (`apate` on crates.io):

1. **Standalone server** — a single binary (`apate`) that serves HTTP and ships a small
   web UI for editing specs. Also distributable as a Docker image.
2. **Rust test library** — embed the server inside your `#[test]` / `#[tokio::test]` to
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
   response whose matchers all pass (or that has no matchers) is selected. If none pass →
   try next `Deceit`.
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
- Default log filter: **`info,apate=debug`** (`apate::DEFAULT_RUST_LOG`).
- Web UI + admin API base path: **`/apate`**.

### Crate features

- `default = ["server"]` — the `server` feature enables `getopt3` (CLI parsing) and
  `include_dir` (embedded web UI). The binary target requires `server`.
- To use Apate **purely as a library** (no web UI, smaller build), disable default features:
  `apate = { version = "0.1", default-features = false }`.

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
  processors). See §2.4.
- `responses` — ordered list. The first one whose matchers pass is chosen. A response with
  **no** `matchers` always matches (acts as a fallback).

### 2.2 `DeceitResponse` (TOML: `[[deceit.responses]]`)

```toml
[[deceit.responses]]
code       = 200                    # optional. HTTP status for THIS response
matchers   = [ /* Matcher ... */ ]  # optional. ALL must pass; empty = always match
headers    = [["X-Custom", "1"]]    # optional. extra headers for this response
processors = [ /* Processor ... */ ]# optional. run after body render
type       = "string"               # optional. see Output types §2.3 (default "string")
output     = "..."                  # the body source, interpreted according to `type`
```

### 2.3 Matchers — full reference

TOML tag is `type`. All matchers except `rhai`/`rhai_ref` support an optional `negate`
boolean (default `false`). `and`/`or` are combinators.

| `type`      | Fields                                          | Description                                             |
|-------------|-------------------------------------------------|---------------------------------------------------------|
| `method`    | `eq`, `negate`?                                 | Exact HTTP method match (e.g. `eq = "GET"`) |
| `header`    | `key`, `value`, `negate`?                       | Exact match on a request header value                     |
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
{ type = "header", key = "User-Agent", value = "curl" }
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
> that result must be a string equal to `eq`, otherwise the matcher fails.

### 2.4 Output types (TOML: `type` on a response)

| `type`     | `output` is interpreted as                          | Notes                                                    |
|------------|-----------------------------------------------------|----------------------------------------------------------|
| `string`   | raw string, returned as-is (DEFAULT)                |                                                          |
| `jinja`    | minijinja template (Jinja2-compatible syntax)        | See §3 Jinja templates API                                |
| `hex`      | HEX string → decoded bytes (no `0x` prefix expected) | Optional `0x` prefix is stripped                          |
| `base64`   | Base64 string → decoded bytes                         | Standard alphabet                                          |
| `rhai`     | inline Rhai script → returns a Blob (response body)  | See §4 Rhai script API                                     |
| `rhai_ref` | reusable Rhai script by id → returns a Blob          | `id = "..."`, optional `args = [...]`                     |

Binary examples (`hex`, `base64`) are the way to return non-text payloads (e.g. PNG files).

### 2.5 Processors (TOML: `[[deceit.processors]]` or `[[deceit.responses.processors]]`)

A processor rewrites the **already rendered** response body. They run in order; each one
receives the body produced by the previous step.

| `type`     | Fields        | Description                                          |
|------------|---------------|------------------------------------------------------|
| `rhai`     | `script`      | Inline Rhai; returns Blob (new body) or empty (keep)  |
| `rhai_ref` | `id`, `args`? | Reusable Rhai script by registry id                   |
| `embedded` | `id`, `args`? | Custom **Rust** processor registered in your app      |

- `embedded` is only available when Apate is embedded in your own Rust application
  (see §5 and the `PostProcessor` trait). It references a processor registered by `id`.
- A processor that returns **no value** (empty) leaves the body unchanged.

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

Used when a response `type = "jinja"`. The template engine is [minijinja](https://docs.rs/minijinja/latest/minijinja)
with Jinja2-compatible syntax. Full syntax reference:
<https://docs.rs/minijinja/latest/minijinja/syntax> · filters:
<https://docs.rs/minijinja/latest/minijinja/filters>.

### 3.1 Global functions (no receiver)

| Function                              | Returns                                   |
|---------------------------------------|-------------------------------------------|
| `random_num()`                        | random number as **string**                |
| `random_num(max)`                     | random number in `[0, max)` as string      |
| `random_num(from, to)`                | random number in `[from, to)` as string    |
| `random_hex()`                        | random HEX string (32 bytes → 64 chars)    |
| `random_hex(bytes_len)`               | random HEX string of `bytes_len` bytes     |
| `uuid_v4()`                           | a random UUID v4 (string)                  |
| `set_response_code(code)`           | **sets/overwrites** the response status code for this request |

> All `random_*`/`uuid_v4` return **strings** (not numbers) in Jinja.

### 3.2 `ctx` object (request/response context)

Exposed under the variable name `ctx`.

**Readable properties:**
- `ctx.method` → HTTP method of the request (e.g. `"GET"`)
- `ctx.path` → the matched path (e.g. `"/user/740"`)

**Methods:**
- `ctx.load_headers()` → map of request headers, **lowercase keys**
- `ctx.load_query_args()` → map of URL query arguments
- `ctx.load_path_args()` → map of path arguments from URI pattern (`/user/{user_id}`)
- `ctx.load_body_string()` → request body as a string (empty string if none)
- `ctx.load_body_json()` → request body parsed as JSON (errors if not valid JSON)
- `ctx.inc_counter("key")` → returns the **previous** counter value for `key`, then
  increments it by 1 (first call returns `0`). Counters are shared server-wide per key.
- `ctx.set_response_code(code)` → **sets** the response status code (e.g. `ctx.set_response_code(503)`)

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

Rhai (<https://rhai.rs>) is used in three roles, each with a different **context** and
**return contract**. The scripting language itself is standard Rhai — see the
[Rhai book](https://rhai.rs/book/ref/index.html) for syntax.

### 4.1 Where Rhai is used

| Role            | Triggered by                              | `ctx` type        | Must return            |
|-----------------|-------------------------------------------|-------------------|------------------------|
| **Matcher**     | `type = "rhai"` / `"rhai_ref"` in matchers | `RhaiRequestContext`  | a **boolean**            |
| **Output**      | `type = "rhai"` / `"rhai_ref"` on response | `RhaiResponseContext` | a **Blob** (body) or empty (empty body) |
| **Processor**   | `type = "rhai"` / `"rhai_ref"` in processors | `RhaiResponseContext` | a **Blob** (new body) or empty (keep) |

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
| `ctx.path`                    | matched path (string)                |
| `ctx.load_headers()`          | map of request headers               |
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
| `storage_read(key)`                 | read a value from the global in-memory KV store (default if absent) |
| `storage_write(key, value)`         | write a value to the global in-memory KV store |

> **Storage** is an in-memory key/value store that persists across requests for the
> lifetime of the server (shared by all Rhai scripts). It is the primary way to mimic
> database / stateful behaviour. Values are stored as JSON-serialized Rhai values.

### 4.6 Logging

Rhai scripts can log via the standard Rhai macros:
- `print!("...")` → logged at `info` level with prefix `RHAI:`
- `debug!("...")` → logged at `debug` level

### 4.7 Rhai examples

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

## 5. DSL usage examples

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

---

## 6. Using Apate as a Rust test library

Apate is a regular library. You start an in-process HTTP server inside your test, call it
with any HTTP client, and it is automatically shut down when the server handle is dropped.

### 6.1 Cargo dependency

```toml
[dev-dependencies]
apate = "0.1"          # test-only dependency
reqwest = { version = "0.12", features = ["blocking", "json"] }
serial_test = "3"      # to serialize tests that share the default port
```

### 6.2 Public API (module paths)

| Item | Path |
|------|------|
| `ApateConfig` | `apate::ApateConfig` |
| `ApateSpecs` | `apate::ApateSpecs` |
| `ApateConfigBuilder` | `apate::ApateConfigBuilder` |
| `apate_server_run(config)` (async) | `apate::apate_server_run` |
| `apate_init_server_config(port, log, files)` | `apate::apate_init_server_config` |
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

### 6.3 `ApateTestServer`

```rust
apate::test::ApateTestServer::start(config: ApateConfig, delay_ms: usize) -> ApateTestServer
```

- Starts the server on `config.port` (default `8228`).
- `delay_ms` — a short sleep after start for slow environments (use `0` or `1` in CI).
- **Synchronous API** — works in both `#[test]` and `#[tokio::test]`.
- **Auto-cleanup**: the server is stopped when the returned value is dropped. Bind it to
  a variable that lives for the duration of the test, e.g. `let _apate = ...;`.

### 6.4 Simplest test (builder style, sync)

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

### 6.5 Builder reference

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

### 6.6 Multiple deceits + custom port (async test)

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

### 6.7 Registering a custom Rust `PostProcessor`

```rust
use apate::ApateConfigBuilder;
use apate::deceit::{DeceitBuilder, DeceitResponseBuilder, DeceitResponseContext};
use apate::processors::{ApateProcessor, PostProcessor, Processor};
use apate::test::ApateTestServer;

struct MySigner;
impl PostProcessor for MySigner {
    fn process(
        &self,
        _input: &[&str],
        _ctx: &DeceitResponseContext,
        response: &[u8],
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>> {
        // `response` is the rendered body as bytes. Return Some(new) to replace it,
        // or None to keep the original body.
        let mut v = response.to_vec();
        v.extend(b" SIGNED");
        Ok(Some(v))
    }
}

#[test]
fn embedded_processor_test() {
    let config = ApateConfigBuilder::default()
        .register_processor(ApateProcessor::post("signer", Box::new(MySigner)))
        .add_deceit(
            DeceitBuilder::with_uris(&["/tx"])
                .add_processor(Processor::Embedded { id: "signer".into(), args: vec![] })
                .add_response(DeceitResponseBuilder::default().with_output("body").build())
                .build(),
        )
        .build();

    let _apate = ApateTestServer::start(config, 0);
    let client = reqwest::blocking::Client::new();
    let r = client.get("http://localhost:8228/tx").send().unwrap();
    assert_eq!(r.text().unwrap(), "body SIGNED");
}
```

### 6.8 Test authoring notes (gotchas)

- **Port conflicts:** all tests default to port `8228`. If you run many tests in parallel,
  either give each test a **distinct port** (via `with_port` / `to_app_config_with_port`)
  or annotate with `#[serial]` from the `serial_test` crate (the repo's own tests use
  `#[serial]`).
- The server is bound to `0.0.0.0:PORT`, so `http://localhost:PORT` always works.
- To enable verbose Apate logging in a test: `apate::test::init_env_logger("debug,apate=trace");`
- The test server does **not** require an async runtime; it spawns its own.
- For a full working reference, see `tests/test-api.rs`, `tests/scripting.rs`,
  and `tests/processors.rs`.

---

## 7. Running the Apate server

### 7.1 Install & run locally (CLI)

Install the `apate` binary from crates.io, then run it directly:

```sh
cargo install apate            # puts the `apate` binary on your $PATH
apate                          # start on default port 8228, no specs
apate -p 8228 ./spec.toml      # start on port 8228 with a TOML spec file
```

CLI arguments (higher priority than env vars) and env configuration: see §7.3.

### 7.2 Run from Docker image

Official image: `ghcr.io/rustrum/apate`. The container runs the `apate` binary, listens on
port **8228**, and starts with **no specs** (add them via UI or API).

#### 7.2.1 Run an empty server

```sh
docker run --rm -t -p 8228:8228 ghcr.io/rustrum/apate:latest
```

#### 7.2.2 Run with mounted TOML specs

Mount your specs and expose their path(s) through `APATHE_SPECS_FILE_*` env variables:

```sh
# from the apate repo root (uses ./examples as the specs dir)
docker run --rm -t -p 8228:8228 \
  -v "$(pwd)/examples:/specs" \
  -e APATHE_SPECS_FILE_1=/specs/apate-specs.toml \
  -e APATHE_SPECS_FILE_2=/specs/apate-specs-rhai.toml \
  ghcr.io/rustrum/apate:latest
```

> **Any** env variable whose name starts with `APATHE_SPECS_FILE` is treated as a path to a
> spec file. You can therefore register many specs with
> `APATHE_SPECS_FILE_1`, `APATHE_SPECS_FILE_2`, …, `APATHE_SPECS_FILE_N`.

### 7.3 Server configuration

**Env variables:**
- `RUST_LOG`, `RUST_LOG_STYLE` — logging (e.g. `RUST_LOG=info,apate=debug`)
- `APATHE_PORT` — server port (default `8228`)
- `APATHE_SPECS_FILE*` — spec file paths (see above)

**CLI arguments (higher priority than env):**
```sh
apate -p 8080 -l warn ./spec.toml ./another.toml
#   -p <port>      port to listen on
#   -l <level>     log level / filter
#   positional args = paths to spec files
```

### 7.4 Web UI & admin REST API

Base path: `http://HOST:PORT/apate` (e.g. `http://localhost:8228/apate`).

| Method | Path | Description |
|--------|------|-------------|
| `GET`  | `/apate/info` | JSON with name + version |
| `GET`  | `/apate/specs` | current specs as TOML |
| `POST` | `/apate/specs/replace` | replace all specs with TOML in request body |
| `POST` | `/apate/specs/append` | append TOML specs (request body) after existing |
| `POST` | `/apate/specs/prepend` | prepend TOML specs (request body) before existing |
| `GET`  | `/apate` | the web UI (single page app) |
| `GET`  | `/apate/assets/{file}` | static UI assets |

Example live spec update:
```sh
curl -X POST http://localhost:8228/apate/specs/replace -d @./new-specs.toml
curl http://localhost:8228/apate/specs        # dump current specs as TOML
curl http://localhost:8228/apate/info         # {"name":"Apate API mocking server","version":"0.1.2"}
```

All `POST` spec endpoints accept a TOML document (the same shape as §2) in the request body
and return a plain-text confirmation. The specs cache (Jinja + Rhai AST) is cleared and
rebuilt on every update, so changes take effect immediately.

---

## 8. Quick decision guide for an AI agent

- **Static / fixed response** → `type = "string"` (or omit `type`).
- **Echo request data / light conditionals / randoms** → `type = "jinja"`.
- **Complex logic, data filtering, stateful behaviour** → `type = "rhai"` (or registry `rhai_ref`).
- **Return binary bytes** → `type = "base64"` or `type = "hex"`.
- **Modify the body after render** → add a `processor` (`rhai`/`rhai_ref`/`embedded`).
- **Share a script across endpoints** → define it once in `[[rhai]]`, reference by `id`.
- **Rust-side logic (signing, crypto, real work)** → embed Apate in your app and register a
  `PostProcessor`, reference with `type = "embedded"`.
- **Unit-test a client against a local API** → use `ApateTestServer::start(config, 0)`.

## 9. Source map (where things live)

| Concern | File(s) |
|---------|---------|
| Public API, `ApateConfig`, `ApateSpecs`, server bootstrap | `src/lib.rs` |
| `Deceit`, `DeceitResponse`, builders, response context | `src/deceit.rs` |
| Matchers (`Matcher` enum + evaluation) | `src/matchers.rs` |
| Output rendering (`OutputType`, Jinja/Hex/Base64/Rhai) | `src/output.rs` |
| Processors (`Processor`, `PostProcessor`, `ApateProcessor`) | `src/processors.rs` |
| Jinja/minijinja context + global functions | `src/jinja.rs` |
| Rhai engine, contexts, global functions, storage | `src/rhai.rs` |
| HTTP request handling pipeline | `src/handlers/mod.rs` |
| Admin API + web UI | `src/handlers/admin.rs` |
| Test server (`ApateTestServer`) | `src/test.rs` |
| CLI entry point | `src/main.rs` |
| Reference tests | `tests/*.rs` |
| Reference specs | `examples/*.toml` |

