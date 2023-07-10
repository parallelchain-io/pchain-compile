/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Basic tests to demonstrate common usage of pchain_compile.

use std::path::Path;

#[tokio::test]
async fn build_contract() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .join("hello_contract")
        .to_path_buf();
    let wasm_name = match pchain_compile::build_target(source_path, None).await {
        Ok(wasm_name) => wasm_name,
        Err(e) => {
            println!("{:?}", e);
            panic!("Note: This test require installation of docker. Make sure the permission has been granted to run docker.");
        }
    };
    let _ = std::fs::remove_file(Path::new(&wasm_name));
    assert_eq!(wasm_name, "hello_contract.wasm");
}