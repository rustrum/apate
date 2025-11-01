use std::collections::HashMap;

use apate::test::{
    ApateSpecs, ApateTestServer, AppConfig, AppConfigBuilder, DEFAULT_PORT, Deceit, DeceitBuilder,
    DeceitResponse, DeceitResponseBuilder, Matcher,
};

fn api_url(uri: &str) -> String {
    format!("http://localhost:{DEFAULT_PORT}{uri}")
}

/// `ApateTestServer` does not require async context so it can be used in regular tests.
#[test]
fn non_async_test() {
    // An example how to build a config if fancy builders does not fit your needs.
    let config = AppConfig {
        specs: ApateSpecs {
            deceit: vec![Deceit {
                uris: vec!["/user/check".to_string()],
                matchers: vec![Matcher::Method {
                    eq: "POST".to_string(),
                }],
                headers: vec![("Content-Type".to_string(), "application/json".to_string())],
                responses: vec![DeceitResponse {
                    content: r#"{"message":"Success"}"#.to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            }],
        },
        ..Default::default()
    };

    let _server = ApateTestServer::start(config, "", 0);

    let client = reqwest::blocking::Client::new();
    let response = client
        .post(api_url("/user/check"))
        .send()
        .expect("Request failed");

    assert_eq!(response.status(), 200);

    let cth = response.headers().get("Content-Type");
    assert!(
        matches!(cth, Some(v) if v == "application/json"),
        "Content-Type header not found"
    );

    let response_json: HashMap<String, String> =
        response.json().expect("Failed to parse JSON response");

    assert_eq!(response_json["message"], "Success");
}

#[tokio::test]
async fn async_test() {
    let config = DeceitBuilder::with_uris(&["/user/check"])
        .require_method("POST")
        .add_header("Content-Type", "application/json")
        .add_response(
            DeceitResponseBuilder::default()
                .code(200)
                .with_content(r#"{"message":"Success"}"#)
                .build(),
        )
        // If you have only single deceit there is a shortcut to build application config.
        .to_app_config();

    let _server = ApateTestServer::start(config, "", 0);

    let client = reqwest::Client::new();
    let response = client
        .post(api_url("/user/check"))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);

    let cth = response.headers().get("Content-Type");
    assert!(
        matches!(cth, Some(v) if v == "application/json"),
        "Content-Type header not found"
    );

    let response_json: HashMap<String, String> = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    assert_eq!(response_json["message"], "Success");

    // Calling non existing endpoint
    let response = client
        .post(api_url("/user/wrong/api"))
        .send()
        .await
        .expect("Request failed");

    assert!(!response.status().is_success());
}

#[tokio::test]
async fn complex_configuration_test() {
    // More complex configuration with several deceit
    let config = AppConfigBuilder::default()
        .add_deceit(
            DeceitBuilder::with_uris(&["/user/add"])
                .require_method("POST")
                .add_header("Content-Type", "application/json")
                .add_response(
                    DeceitResponseBuilder::default()
                        .code(200)
                        .with_content(r#"{"message":"Success"}"#)
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
                        .with_content(r#"{"id": "{{ path_args.id }}", "name":"Ignat"}"#)
                        .build(),
                )
                .build(),
        )
        .build();

    let _server = ApateTestServer::start(config, "", 0);

    let client = reqwest::Client::new();

    // POST query to address "/user/add"
    let response = client
        .post(api_url("/user/add"))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);

    assert!(
        matches!(response.headers().get("Content-Type"), Some(v) if v == "application/json"),
        "Content-Type header not found"
    );

    let response_json: HashMap<String, String> = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    assert_eq!(response_json["message"], "Success");

    // GET query to address "/user/{id}"
    let response = client
        .get(api_url("/user/1133"))
        .send()
        .await
        .expect("Request failed");

    assert_eq!(response.status(), 200);
    assert!(
        matches!(response.headers().get("Content-Type"), Some(v) if v == "application/json"),
        "Content-Type header not found"
    );

    let response_json: HashMap<String, String> = response
        .json()
        .await
        .expect("Failed to parse JSON response");

    assert_eq!(response_json["name"], "Ignat");
    assert_eq!(response_json["id"], "1133");
}
