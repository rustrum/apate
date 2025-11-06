use std::io;

use apate::{
    ApateConfigBuilder, DEFAULT_RUST_LOG, apate_server_run,
    deceit::{DeceitBuilder, DeceitResponseBuilder},
    processors::{ApateProcessor, PostProcessor, Processor},
};

#[actix_web::main]
async fn main() -> io::Result<()> {
    // I do not call apate init config function thus have to do something like this
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(DEFAULT_RUST_LOG))
        .init();

    // You could do it without builders for sure
    let config = ApateConfigBuilder::default()
        // Here we registering custom processor with id "signer"
        .register_processor(ApateProcessor::post(
            "signer",
            JsonSignerPostProcessor::boxed(),
        ))
        // Warning this part should be in TOML file but I'm too to load it from file
        // Anyway you could get it via Apate admin API to figure out proper TOML syntax.
        .add_deceit(
            DeceitBuilder::with_uris(&["/transaction/{id}"])
                .add_header("Content-type", "application/json")
                .add_response(
                    DeceitResponseBuilder::default()
                        .with_output(
                            r#"{"id": "{{ path_args.id }}","amount":"{{ random_num(42) }}"}"#,
                        )
                        .build(),
                )
                // Now we referencing custom processor from registry by id "signer"
                .add_processor(Processor::Custom {
                    id: "signer".to_string(),
                    // Custom processor could have some user defined input for each scope.
                    // In TOML you could skip it - it will be defined as an empty string.
                    input: "abcd".to_string(),
                })
                .build(),
        )
        .build();

    log::debug!("Configuration initialized: {:?}", config);

    apate_server_run(config).await
}

/// In this example post processor will do some kind of "signing" of the output JSON.
struct JsonSignerPostProcessor {}

impl JsonSignerPostProcessor {
    fn boxed() -> Box<Self> {
        Box::new(Self {})
    }
}

impl PostProcessor for JsonSignerPostProcessor {
    fn process<'a>(
        &self,
        input: &str,
        _context: &apate::deceit::DeceitResponseContext<'a>,
        response: &[u8],
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>> {
        let seed = input.len();
        let text = str::from_utf8(response)?;

        let mut json_body: serde_json::Map<String, serde_json::Value> = serde_json::from_str(text)?;

        // Yes this is not a signature, but it is just an example
        let mut data: Vec<u8> = Default::default();

        data.extend(
            json_body
                .get("id")
                .unwrap_or_default()
                .as_str()
                .unwrap_or_default()
                .as_bytes(),
        );
        data.extend(
            json_body
                .get("amount")
                .unwrap_or_default()
                .as_str()
                .unwrap_or_default()
                .as_bytes(),
        );

        let hash = cityhasher::hash_with_seed(&data, seed as u64);

        json_body.insert("signature".to_string(), hash.into());

        let result = serde_json::to_string(&json_body)?.into_bytes();
        Ok(Some(result))
    }
}
