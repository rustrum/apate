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

If you need to add custom rust logic to Apate you can easily create your own server based on Apate library.
See [processors](./examples/processors.rs) example.


## Apate specification files

Repo contains [specification example file](./examples/apate-specs.toml) with verbose comments.
I hope that you will be smart enough to understand it by yourself.

### Template syntax

Response content utilize [minijinja](https://docs.rs/minijinja/latest/minijinja) template engine.
Template syntax documentation can be found [here](https://docs.rs/minijinja/latest/minijinja/syntax).
See also [minijinja filters](https://docs.rs/minijinja/latest/minijinja/filters).

Look for template usage examples in [this specs file](./examples/apate-template-specs.toml).

### Non string responses

It is possible to respond with binary content instead of string.
See examples [here](./examples/apate-specs-bin.toml).

### Custom post processors

You may need to add custom logic into API to make it looks real.
For example all your responses must be signed.
In this case it is better to implement signing functionality in Apate
instead of adding "skip signature verification" flag to your main app.

Usage examples could be found [here](./examples/processors.rs).


## Butt why do I really need Apate?

You could try to mock any external API with Apate.
The more stupid/unstable/unusable external API is the more reasons for you to use Apate.

- **rust unit tests** - to test your code fully without mocking anything.
- **local development** - when running other API services on localhost is painful.
- **integration tests** - if 3rd party API provider suck/stuck/etc it is better to run test suites against predictable API endpoints.
- **load tests** - when deployed alongside your application Apate should respond fast, so no need to take external API delays into account.


## License

This product distributed under MIT license BUT only under certain conditions that listed in the LICENSE-TERMS file.
