use apate::test::{DeceitBuilder, DeceitResponseBuilder};

fn main() {
    // You can add your test cases here
    println!("Running example tests...");

    apate_unit_test_example();
}

fn apate_unit_test_example() {
    let deceit = DeceitBuilder::with_uris(&["user/add"])
        .require_method("POST")
        .add_response(
            DeceitResponseBuilder::default()
                .code(200)
                .with_content(r#"{"message":"Success"}"#)
                .build(),
        )
        .build();
}
