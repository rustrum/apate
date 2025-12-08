<p align="center" width="100%" style="text-align:center">
<img src="./assets/apate-logo.png" alt="Apate API mocking server" />
</p>

<p align="center" width="100%" style="text-align:center">
<a href="https://crates.io/crates/apate"><img src="https://img.shields.io/crates/v/apate.svg" alt="Crates.io"></a>
<a href="https://docs.rs/apate"><img src="https://docs.rs/apate/badge.svg" alt="Released API docs"></a>
<p align="center" width="100%" style="text-align:center">


API mocking server that main purpose is to help with integration and end-to-end testing.

Project named after Apate - the goddess and personification of deceit.


## Is it stable ?

Yes it is!
Only small changes and new features planned for future stable release v0.1.0.

## Features

 - Mocking any string based response
 - Binary response support
 - Jinja template language for customizing responses
 - Rhai scripting for advanced scenarios
 - Custom builds with custom rust extensions


## Running Apate server

### Docker image

Launching a clean disposable container is easy with docker.

```sh
docker run --rm -tp 8228:8228 ghcr.io/rustrum/apate:latest
```

It will run Apate server without any URI deceit.
So you should add new specification via API endpoints or web UI (see below).

To start server with some specs mount your TOML specs into docker image and provide proper ENV variables.

```sh
docker run --rm -tp 8228:8228 -v $(pwd)/examples:/specs -e APATHE_SPECS_FILE_1=/specs/apate-specs.toml ghcr.io/rustrum/apate:latest
```

Example above expecting you to execute `docker run` from the Apate git repository root.

### Install & run locally via cargo

If you have `cargo` then just install it as `cargo install apate`.
After that you will have `apate` binary in your `$PATH`.


## Apate server configuration

### Web UI

Apate web UI is located at `http://HOST:PORT/apate` (will be `http://localhost:8228/apate` for most cases).
Works for docker too.

**Please notice** that specification shown in web UI is not looking cool.
All because it is automatically generated from the internal representation.
Please see `examples` folder to figure out how to write TOML specs in pretty way.

### ENV variables and CLI args

You could use next ENV variables:

 - `RUST_LOG` and `RUST_LOG_STYLE` - to configure logging
 - `APATHE_PORT` - to provide port to run server on (default 8228)
 - `APATHE_SPECS_FILE...` - any ENV variable which name is started with such prefix will be parsed as a path to spec file

Apate can be also configured with CLI arguments which has higher priority than ENV variables.

```sh
apate -p 8080 -l warn ./path/to/spec.toml ./path/to/another_spec.toml
```

- `-p` - port to run server on
- `-l` - logging level
- positional arguments - paths to spec files

### REST API

If you like `curl` you can configure Apate while it is running.

- GET `/apate/info` - returns JSON with basic info about current server
- GET `/apate/specs` - return TOML with a specs file
- POST `/apate/specs/replace` - replace current specs with a new one from the request body
- POST `/apate/specs/append` - add specs from request after existing
- POST `/apate/specs/prepend` - add specs from request before existing

All POST methods require TOML specification in request body.
Something like this:

```sh
curl -X POST http://localhost:8228/apate/specs/replace -d @./new-specs.toml
```


## Using Apate in rust tests

Some self explanatory tests examples [could be found here](./tests/test-api.rs).

In a nutshell, you should create an instance of Apate server at the beginning of your test.
And you will be able to call your API endpoints at `http://localhost:8228` (or any other port you'll specify).

This is a how it will looks like in the code.
```rust

/// Yes the test does not require to be async.
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
        .to_app_config();

    // Assign the server to some variable and it will be dropped at the end of the test.
    let _apate = ApateTestServer::start(config, 0);

    // That's all you need to do.
    // Now you can call http://localhost:8228/user/check 
    // You will get JSON response: {"message":"Success"}
    // And response will have header: "Content-Type: application/json"
}
```


## Making your custom Apate server

It is possible to run Apate embedded into your application.
You may need this to add custom rust logic into response processing.
For example it could be response signature functionality.
See [processors](./examples/processors.rs) example.


## Apate specification

To understand how it works look into [specification example file](./examples/apate-specs.toml), it has verbose comments.
There are other specification files as well with more advanced examples.

**Rhai** scripting language is used to extend configuration capabilities. 
See [Rhai website](https://rhai.rs), [Rhai docs](https://rhai.rs/book/ref/index.html) and [configuration examples](./examples/apate-specs-rhai.toml).

I expect that for most cases you will not need any Rhai scripting. It is meant only for complex scenarios.


### Matchers

Piece of DSL or Rhai script that returns boolean. In order to proceed further all matchers must return true.

### Processors

Type of logic that runs after response output was generated.
As a result processor could return different body content that will be used instead of original one 
and will be passed to all other processors downstream.

Processors are defined using **Rhai script**.

### Response types

#### String (default)

Simply returns string from specification as is.

####  Binary content

Can handle output string as a binary content in  HEX or Base64 formats.
See examples [here](./examples/apate-specs-bin.toml).

### Jinja (minijinja) templates

Response with `output_type="jinja"` processed as a jinja template 
using [minijinja](https://docs.rs/minijinja/latest/minijinja) template engine.
Template syntax documentation can be found [here](https://docs.rs/minijinja/latest/minijinja/syntax).
See also [minijinja filters](https://docs.rs/minijinja/latest/minijinja/filters).

Apate usage examples available in [this specs file](./examples/apate-template-specs.toml).


## Butt why do I really need Apate?

You could try to mock any external API with Apate.
The more stupid/unstable/unusable external API is the more reasons for you to use Apate.

- **rust unit tests** - to test your code fully without mocking anything.
- **local development** - when running other API services on localhost is painful.
- **integration tests** - if 3rd party API provider suck/stuck/etc it is better to run test suites against predictable API endpoints.
- **load tests** - when deployed alongside your application Apate should respond fast, so no need to take external API delays into account.


## License

This product distributed under MIT license BUT only under certain conditions that listed in the LICENSE-TERMS file.
