use apate::{
    ApateConfig, ApateConfigBuilder,
    deceit::{DeceitBuilder, DeceitResponseBuilder},
    matchers::Matcher,
    processors::Processor,
    test::{ApateTestServer, DEFAULT_PORT},
};
use serial_test::serial;

const INIT_DELAY_MS: usize = 1;

const MATCHER_SCRIPT: &str = r#"
if ctx.method == "GET" {
    return true;
}

let headers = ctx.load_headers();
debug!(`headers: ${headers}`);
if headers["user-agent"] == "Apate" {
    return true;
}

let qargs = ctx.load_query_args();
debug!(`qargs: ${qargs}`);
if qargs["library"] == "Apate" {
    return true;
}

let pargs = ctx.load_path_args();
debug!(`pargs: ${pargs}`);
if pargs["name"] == "rhai" {
    return true;
}

let body = ctx.load_body().as_string();
if body.contains("Apate") {
    return true;
}

return false;
"#;

const REF_ID: &str = "rhai-processor";

const REF_SCRIPT: &str = r#"
let body = body.as_string();
let map = #{ 
    output: body,
    path: ctx.path,
    args: args,
    counter: ctx.inc_counter("cnt"),
};

let pargs = ctx.load_path_args();
if "name" in pargs {
    map.pname = pargs["name"];
    if pargs.name == "java" {
        return ();
    }
}

let qargs = ctx.load_query_args();
if "name" in qargs {
    map.qname = qargs.name;
    if qargs.name == "rhai" {
        ctx.response_code = 201;
    }
}

return map.to_json().to_blob();
"#;

fn api_url(uri: &str) -> String {
    format!("http://localhost:{DEFAULT_PORT}{uri}")
}

fn build_config() -> ApateConfig {
    ApateConfigBuilder::default()
        .add_script(REF_ID, REF_SCRIPT)
        .add_deceit(
            DeceitBuilder::with_uris(&["/match/{name}", "/matcher"])
                .add_matcher(Matcher::Rhai {
                    script: MATCHER_SCRIPT.to_string(),
                })
                .add_response(
                    DeceitResponseBuilder::default()
                        .with_output(r#"It is .k"#)
                        .build(),
                )
                .build(),
        )
        .add_deceit(
            DeceitBuilder::with_uris(&["/process/{name}", "/processor"])
                .add_processor(Processor::RhaiRef {
                    id: REF_ID.to_string(),
                    args: vec!["rhai_arg_1".to_string(), "rhai_arg_2".to_string()],
                })
                .add_response(
                    DeceitResponseBuilder::default()
                        .with_output(r#"It is .k"#)
                        .build(),
                )
                .build(),
        )
        .build()
}

#[tokio::test]
#[serial]
async fn test_rhai_matcher() {
    // apate::test::init_env_logger("debug,apate=trace");

    let _apate = ApateTestServer::start(build_config(), INIT_DELAY_MS);

    let client = reqwest::Client::new();

    // Method
    let response = client.post(api_url("/matcher")).send().await.unwrap();
    assert_ne!(response.status(), 200, "Error in {response:?}");

    let response = client.get(api_url("/matcher")).send().await.unwrap();
    assert_eq!(response.text().await.unwrap(), "It is .k");

    // Headers
    let response = client
        .post(api_url("/matcher"))
        .header("User-Agent", "curl")
        .send()
        .await
        .unwrap();
    assert_ne!(response.status(), 200, "Error in {response:?}");

    let response = client
        .post(api_url("/matcher?name=rhai"))
        .header("User-Agent", "Apate")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "It is .k");

    // Path arg
    let response = client.post(api_url("/match/lua")).send().await.unwrap();
    assert_ne!(response.status(), 200, "Error in {response:?}");

    let response = client.post(api_url("/match/rhai")).send().await.unwrap();
    assert_eq!(response.text().await.unwrap(), "It is .k");

    // Query arg
    let response = client
        .post(api_url("/matcher?library=Postman"))
        .send()
        .await
        .unwrap();
    assert_ne!(response.status(), 200, "Error in {response:?}");

    let response = client
        .post(api_url("/matcher?library=Apate"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "It is .k");

    // Body content
    let response = client
        .post(api_url("/matcher"))
        .body("This is custom body")
        .send()
        .await
        .unwrap();
    assert_ne!(response.status(), 200, "Error in {response:?}");

    let response = client
        .post(api_url("/matcher"))
        .body("Valid Apate body")
        .send()
        .await
        .unwrap();
    assert_eq!(response.text().await.unwrap(), "It is .k");
}

#[tokio::test]
#[serial]
async fn test_rhai_ref_matcher() {
    // apate::test::init_env_logger("debug,apate=trace");
    let _apate = ApateTestServer::start(build_config(), INIT_DELAY_MS);
    let client = reqwest::Client::new();

    // basic stuff
    let response = client.post(api_url("/processor")).send().await.unwrap();
    let jval: serde_json::Value = response.json().await.unwrap();
    json_basic_assert(&jval, "/processor", 0);

    // No return
    let response = client.post(api_url("/process/java")).send().await.unwrap();
    let text = response.text().await.unwrap();
    assert_eq!(text, "It is .k");

    // path args
    let response = client.post(api_url("/process/rhai")).send().await.unwrap();
    let jval: serde_json::Value = response.json().await.unwrap();
    json_basic_assert(&jval, "/process/rhai", 2);
    assert_eq!(jval.get("pname").unwrap().as_str().unwrap(), "rhai");

    // query args
    let response = client
        .post(api_url("/processor?name=java"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let jval: serde_json::Value = response.json().await.unwrap();
    json_basic_assert(&jval, "/processor", 3);
    assert_eq!(jval.get("qname").unwrap().as_str().unwrap(), "java");

    // query args with response code
    let response = client
        .post(api_url("/processor?name=rhai"))
        .send()
        .await
        .unwrap();
    assert_eq!(
        response.status().as_u16(),
        201,
        "{:?}",
        response.text().await
    );
    let jval: serde_json::Value = response.json().await.unwrap();
    json_basic_assert(&jval, "/processor", 4);
    assert_eq!(jval.get("qname").unwrap().as_str().unwrap(), "rhai");
}

fn json_basic_assert(v: &serde_json::Value, path: &str, cnt: usize) {
    assert_eq!(v.get("output").unwrap().as_str().unwrap(), "It is .k");
    assert_eq!(v.get("path").unwrap().as_str().unwrap(), path);
    assert_eq!(v.get("counter").unwrap().as_u64().unwrap(), cnt as u64);
    let args: Vec<&str> = v
        .get("args")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();

    assert_eq!(args.len(), 2);
    assert!(args.contains(&"rhai_arg_1"), "{args:?}");
    assert!(args.contains(&"rhai_arg_2"), "{args:?}");
}
