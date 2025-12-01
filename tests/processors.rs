use apate::processors::{ApateProcessor, PostProcessor, Processor};
use serial_test::serial;

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use apate::deceit::{DeceitBuilder, DeceitResponseBuilder, DeceitResponseContext};

use apate::ApateConfigBuilder;
use apate::test::{ApateTestServer, DEFAULT_PORT};

const INIT_DELAY_MS: usize = 1;

fn api_url(uri: &str) -> String {
    format!("http://localhost:{DEFAULT_PORT}{uri}")
}

struct CustomProcessor {
    counter: Arc<AtomicU32>,
}

impl CustomProcessor {
    fn boxed(counter: Arc<AtomicU32>) -> Box<Self> {
        Box::new(Self { counter })
    }
}

impl PostProcessor for CustomProcessor {
    fn process(
        &self,
        input: &str,
        _context: &DeceitResponseContext,
        response: &[u8],
    ) -> Result<Option<Vec<u8>>, Box<dyn core::error::Error>> {
        self.counter.fetch_add(1, Ordering::SeqCst);

        if input.trim().is_empty() {
            return Ok(None);
        }

        let mut out = Vec::new();

        out.extend(response);
        out.extend(input.as_bytes());

        Ok(Some(out))
    }
}

#[test]
#[serial]
fn test_custom_user_processor() {
    let counter = Arc::new(AtomicU32::new(0));

    let config = ApateConfigBuilder::default()
        .register_processor(ApateProcessor::post(
            "processor_id_1",
            CustomProcessor::boxed(counter.clone()),
        ))
        .add_deceit(
            DeceitBuilder::with_uris(&["/processor/run"])
                .add_processor(Processor::Custom {
                    id: "processor_id_1".to_string(),
                    input: Default::default(),
                })
                .add_response(
                    DeceitResponseBuilder::default()
                        .with_output(r#"simple_reponse"#)
                        .build(),
                )
                .build(),
        )
        .add_deceit(
            DeceitBuilder::with_uris(&["/processor/append"])
                .add_processor(Processor::Custom {
                    id: "processor_id_1".to_string(),
                    input: "_TAIL".to_string(),
                })
                .add_response(
                    DeceitResponseBuilder::default()
                        .with_output(r#"simple_reponse"#)
                        .build(),
                )
                .build(),
        )
        .build();

    let _apate = ApateTestServer::start(config, INIT_DELAY_MS);

    let client = reqwest::blocking::Client::new();

    assert_eq!(counter.load(Ordering::SeqCst), 0);

    let response = client
        .post(api_url("/processor/run"))
        .send()
        .expect("Request failed");

    assert_eq!(response.status(), 200);

    let response_text = response.text().expect("Failed to parse JSON response");

    assert_eq!(response_text, "simple_reponse");

    assert_eq!(counter.load(Ordering::SeqCst), 1);

    let response = client
        .post(api_url("/processor/append"))
        .send()
        .expect("Request failed");

    assert_eq!(response.status(), 200);

    let response_text = response.text().expect("Failed to parse JSON response");

    assert_eq!(response_text, "simple_reponse_TAIL");

    assert_eq!(counter.load(Ordering::SeqCst), 2);
}
