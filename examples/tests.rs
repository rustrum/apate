use std::collections::HashMap;

use apate::{
    DEFAULT_RUST_LOG,
    test::{ApateTestServer, DeceitBuilder, DeceitResponseBuilder, apate_run_test},
};

#[tokio::main]
async fn main() {
    println!("Running example tests...");

    apate_unit_test_example().await;
}

fn apate_init() -> ApateTestServer {
    let config = DeceitBuilder::with_uris(&["/user/add"])
        .require_method("POST")
        .add_response(
            DeceitResponseBuilder::default()
                .code(200)
                .with_content(r#"{"message":"Success"}"#)
                .build(),
        )
        .to_app_config(8088);

    apate_run_test(config, DEFAULT_RUST_LOG)
}

async fn apate_unit_test_example() {
    let apate = apate_init();

    println!("STARTING TEST");

    let client = reqwest::Client::new();

    let resp = client
        .post("http://localhost:8088/user/add")
        .send()
        .await
        .expect("Valid response");

    println!("CHECKING response");

    assert_eq!(resp.status().as_u16(), 200);

    let body = resp
        .json::<HashMap<String, String>>()
        .await
        .expect("Parsed response");

    assert_eq!(body["message"], "Success");

    println!("EXIT TEST");
    drop(apate);
}
