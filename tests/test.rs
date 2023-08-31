/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Basic tests to demonstrate common usage of pchain_compile.

use std::path::Path;

use pchain_compile::{DockerOption, BuildOptions, DockerConfig};

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

#[tokio::test]
async fn build_contract_to_destination() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .join("hello_contract")
        .to_path_buf();
    let destination_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .to_path_buf();
    let wasm_name = match pchain_compile::build_target(source_path, Some(destination_path.clone()))
        .await
    {
        Ok(wasm_name) => wasm_name,
        Err(e) => {
            println!("{:?}", e);
            panic!("Note: This test require installation of docker. Make sure the permission has been granted to run docker.");
        }
    };
    let _ = std::fs::remove_file(destination_path.join(&wasm_name));
    assert_eq!(wasm_name, "hello_contract.wasm");
}

#[tokio::test]
async fn build_contract_with_docker() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .join("hello_contract")
        .to_path_buf();
    let destination_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .to_path_buf();
    let wasm_name = pchain_compile::Config {
        source_path,
        destination_path: Some(destination_path.clone()),
        build_options: BuildOptions { locked: true },
        docker_option: DockerOption::Docker(DockerConfig::default()),
    }
    .run()
    .await
    .unwrap();

    assert!(destination_path.join("Cargo.lock").exists());
    let _ = std::fs::remove_file(destination_path.join(&wasm_name));
    assert_eq!(wasm_name, "hello_contract.wasm");
}

#[tokio::test]
async fn build_contract_without_docker() {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .join("hello_contract")
        .to_path_buf();
    let destination_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("contracts")
        .to_path_buf();
    let run_result = pchain_compile::Config {
        source_path,
        destination_path: Some(destination_path.clone()),
        build_options: BuildOptions { locked: true },
        docker_option: DockerOption::Dockerless,
    }
    .run() 
    .await;

    let wasm_name = match run_result {
        Ok(wasm_name) => wasm_name,
        Err(e) => {
            println!("{:?}", e);
            panic!("Note: This test require installation of target 'wasm32-unknown-unknown'. It can be installed by 'rustup add wasm32-unknown-unknown'");
        }
    };

    assert!(destination_path.join("Cargo.lock").exists());
    let _ = std::fs::remove_file(destination_path.join(&wasm_name));
    assert_eq!(wasm_name, "hello_contract.wasm");
}
