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
