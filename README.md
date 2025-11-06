# Apate

[![crates.io](https://img.shields.io/crates/v/apate.svg)](https://crates.io/crates/apate)
[![Released API docs](https://docs.rs/apate/badge.svg)](https://docs.rs/apate)

API mocking service that main purpose is to help with integration and end-to-end testing.

Project named after Apate - the goddess and personification of deceit.


## What is project state is it stable ?

It will be stable after 0.1 MVP release that should be very soon.
Right now almost all API is setted up and only small changes and new features will be provided.


## Running Apate server

### Installation

It is kinda tricky now you should build it from source.
Clone repository, `cd` into it and run:

```sh
cargo install --features server --path .
```

or you can get/build latest version from from crates.io

```sh
cargo install apate --features server
```

### Configuration

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


## Using Apate in tests

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


## Custom Apate server

Using Apate as a library you can spin up your own server.
This is useful when you need to add custom rust logic to Apate.
See [processors](./examples/processors.rs) example.


## Apate specification

Respository contains [specification example file](./examples/apate-specs.toml) with verbose comments.
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

### For local development

Because it could be more convinient to interact with fast and predictable APIs on localhost that calling something far away.

### For unit tests

There is no need to mock your application logic that performing HTTP calls.
Just provide local API endpoints with predefined responses for each test.
So the flow will be like this:
```
[call your lib logic] -> [real API call] -> [Apate] -> [handling response]
```

### Integration / E2E tests

You could deploy Apate server within your dev/stage/whatever environment.
Now you will have predictable responses without calling 3rd party API provider.
It is very useful if 3rd party API provider does not have any test environment or it is not stable, has rate limits etc.

### Load tests

Apate server does not mean to be super fast but if deployed alongside your application it could work much better than remote server with some weird logic behind it. 
Thus you will be able to focus mostly on your apps performance ignoring 3rd party API delays.
