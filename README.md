# Apate

[![crates.io](https://img.shields.io/crates/v/apate.svg)](https://crates.io/crates/apate)
[![Released API docs](https://docs.rs/apate/badge.svg)](https://docs.rs/apate)

API mocking service that main purpose is to help with integration and end-to-end testing.

Project named after Apate - the goddess and personification of deceit.

## Danger WIP MVP

Right now project approaching MVP stage.
Anything could change without prior notice.
But I'm planning to release 0.1 stable MVP version soon.


## Butt why do I need it?

Keep in mind that apart from unit testing you could use Apate with any technology stack that does HTTP calls.

### Local development

It could be useful to do not call external APIs during development but interact with something that behaves like it.

### Unit tests

You want to test your application logic without mocking client HTTP calls. 
So the flow will look like this:
```
[call function] -> [real API call] -> [mocked response] -> [response processing] -> [final result]
```

### Integration / E2E tests

You need to run integration tests where 3rd party API calls required to complete the logic flow. But 3rd party API does not has test environment, or it is not stable, has rate limits etc.

You could deploy Apate service with your application env and call it instead external provider.


### Load tests

Apate service does not mean to be super fast. But if deployed alongside your application it could work much better than remote server with some logic behind API.

Thus you will be able to focus mostly on your apps performance ignoring 3rd party API delays.

## CLI server

### Installation

It is kinda tricky now you should build it from source.
Clone repository, `cd` into it and run:

```sh
cargo install --features server --path .
```

### Configuration

You could use next ENV variables:

 - `RUST_LOG` and `RUST_LOG_STYLE` - to configure logging
 - `APATHE_PORT` - to provide port to run server on (default 8545)
 - `APATHE_SPECS_FILE...` - any ENV variable which name is started with this suffix will be parsed as a path to spec file

You could start `apate` with CLI arguments to.

```sh
apate -p 8080 -l warn ./path/to/spec.toml ./path/to/another_spec.toml
```

- `-p` - port to run server on
- `-l` - logging level
- positional arguments - paths to spec files

CLI arguments has higher priority than ENV variables but I do not recommend to mix them.


## Use with your unit tests

You can use apate as a library for you unit tests.
Here are some examples how to use it [see test examples](./tests/test-api.rs).

Before test start you should create instance of apate test server and launch it.
After that you will be able to call your API endpoints at `http://localhost:8545` or any other port you specified.


This is a how it will looks like in the code.
```rust

/// Yes the test does not require to be async.
fn my_api_test() {
    let config = DeceitBuilder::with_uris(&["/user/check"])
        .require_method("POST")
        .add_header("Content-Type", "application/json")
        .add_response(
            DeceitResponseBuilder::default()
                .code(200)
                .with_content(r#"{"message":"Success"}"#)
                .build(),
        )
        .to_app_config();

    let _server = ApateTestServer::start(config, 0);

    // That's all you need to do.
    // Now you can call http://localhost:8545/user/check 
    // You will get JSON response{"message":"Success"}
    // And response will have header "Content-Type: application/json"
}
```

## Specs explanation

There is a [specs example file](./examples/apate-specs.toml) which contains different configuration options with comments.
I hope that you will be smart enough to understand it by yourself.

Response content utilize [minijinja](https://docs.rs/minijinja/latest/minijinja/) template engine.
Template syntax documentation can be found [here](https://docs.rs/minijinja/latest/minijinja/syntax).
