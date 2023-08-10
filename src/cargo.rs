/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implements the compilation process of smart contract by utilizing crates `cargo`, `wasm-opt` and `wasm-snip`.

use std::path::{Path, PathBuf};

use cargo::{
    core::compiler::{CompileKind, CompileTarget},
    ops::CompileOptions,
    util::interning::InternedString,
    Config,
};

use crate::error::Error;

use rand::{distributions::Alphanumeric, thread_rng, Rng};

/// Generate a random temporary directory name
pub(crate) fn random_temp_dir_name() -> PathBuf {
    std::env::temp_dir()
        .join(
            thread_rng()
                .sample_iter(&Alphanumeric)
                .take(5)
                .map(char::from)
                .collect::<String>(),
        )
        .to_path_buf()
}

/// Equivalent to running following commands:
/// 1. cargo build --target wasm32-unknown-unknown --release --quiet
/// 2. wasm-opt -Oz <wasm_file> --output temp.wasm
/// 3. wasm-snip temp.wasm --output temp2.wasm --snip-rust-fmt-code --snip-rust-panicking-code
/// 4. wasm-opt --dce temp2.wasm --output <wasm_file>
pub(crate) fn build_contract(
    working_folder: &Path,
    source_path: &Path,
    destination_path: Option<PathBuf>,
    wasm_file: &str,
) -> Result<(), Error> {
    let output_path = destination_path.unwrap_or(Path::new(".").to_path_buf());

    // 1. cargo build --target wasm32-unknown-unknown --release --quiet
    let mut config = Config::default().unwrap();
    config
        .configure(0, true, None, false, false, false, &None, &[], &[])
        .unwrap();
    let mut compile_configs =
        CompileOptions::new(&config, cargo::util::command_prelude::CompileMode::Build).unwrap();
    compile_configs.build_config.requested_kinds = vec![CompileKind::Target(
        CompileTarget::new("wasm32-unknown-unknown").unwrap(),
    )];
    compile_configs.build_config.requested_profile = InternedString::new("release");
    let ws =
        cargo::core::Workspace::new(&source_path.join("Cargo.toml"), &config).map_err(|e| {
            Error::BuildFailure(
                format!("Error in preparing workspace according to the manifest file in source path:\n\n{:?}\n", e),
            )
        })?;
    cargo::ops::compile(&ws, &compile_configs)
        .map_err(|e| Error::BuildFailure(format!("Error in cargo build:\n\n{:?}\n", e)))?;

    // 2. wasm-opt -Oz wasm_file --output temp.wasm
    let temp_wasm = working_folder.join("temp.wasm");
    wasm_opt::OptimizationOptions::new_optimize_for_size_aggressively()
        .run(
            source_path
                .join("target")
                .join("wasm32-unknown-unknown")
                .join("release")
                .join(wasm_file),
            &temp_wasm,
        )
        .map_err(|e| Error::BuildFailure(format!("Wasm optimization error:\n\n{:?}\n", e)))?;

    // 3. wasm-snip temp.wasm --output temp2.wasm --snip-rust-fmt-code --snip-rust-panicking-code
    let temp2_wasm = working_folder.join("temp2.wasm");
    let wasm_snip_options = wasm_snip::Options {
        snip_rust_fmt_code: true,
        snip_rust_panicking_code: true,
        ..Default::default()
    };
    let mut module = walrus::ModuleConfig::new()
        .parse_file(temp_wasm)
        .map_err(|e| Error::BuildFailure(format!("Wasm snip error:\n\n{:?}\n", e)))?;
    wasm_snip::snip(&mut module, wasm_snip_options)
        .map_err(|e| Error::BuildFailure(format!("Wasm snip error:\n\n{:?}\n", e)))?;
    module
        .emit_wasm_file(&temp2_wasm)
        .map_err(|e| Error::BuildFailure(format!("Wasm snip error:\n\n{:?}\n", e)))?;

    // 4. wasm-opt --dce temp2.wasm --output wasm_file
    let optimized_wasm = output_path.join(wasm_file);
    wasm_opt::OptimizationOptions::new_optimize_for_size()
        .add_pass(wasm_opt::Pass::Dce)
        .run(temp2_wasm, optimized_wasm)
        .map_err(|e| Error::BuildFailure(format!("Wasm optimization error:\n\n{:?}\n", e)))?;

    Ok(())
}
