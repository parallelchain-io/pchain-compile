/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implements the compilation process of smart contract by utilizing crates `cargo`, `wasm-opt` and `wasm-snip`.

use std::{path::{Path, PathBuf}, io::Write, sync::{Arc, Mutex}};

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

/// Equivalent to run following commands:
/// 1. cargo build --target wasm32-unknown-unknown --release --quiet
/// 2. wasm-opt -Oz <wasm_file> --output temp.wasm
/// 3. wasm-snip temp.wasm --output temp2.wasm --snip-rust-fmt-code --snip-rust-panicking-code
/// 4. wasm-opt --dce temp2.wasm --output <wasm_file>
pub(crate) fn build_contract(
    working_folder: &Path,
    source_path: &Path,
    destination_path: Option<PathBuf>,
    locked: bool,
    wasm_file: &str,
) -> Result<(), Error> {
    let output_path = destination_path.unwrap_or(Path::new(".").to_path_buf());

    // 1. cargo build --target wasm32-unknown-unknown --release --quiet
    // Does not set "--locked" if the Cargo.lock file does not exist.
    let use_cargo_lock = locked && source_path.join("Cargo.lock").exists();
    let mut config = CargoConfig::new();
    config
        .configure(0, false, None, false, use_cargo_lock, false, &None, &[], &[])
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
    if let Err(_) = cargo::ops::compile(&ws, &compile_configs) {
        return Err(Error::BuildFailureWithLogs(config.logs()))
    }

    // Save Cargo.lock to output folder if applicable
    if locked {
        let _ = std::fs::copy(source_path.join("Cargo.lock"), output_path.join("Cargo.lock"));
    }

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

/// Captures the [cargo::util::Config] with custom instantiation.
pub struct CargoConfig {
    /// The logs from the shell which is used by the cargo
    logs: Arc<Mutex<Vec<String>>>,
    /// Cargo configuration
    config: Config,
}

impl CargoConfig {
    pub fn new() -> Self {
        // Setup a shell that stores logs in memory.
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let log_writter = BuildLogWritter { buffer: logs.clone() };
        let shell = cargo::core::Shell::from_write(Box::new(log_writter));

        // Setup Cargo configuration with the custom shell.
        let current_dir = std::env::current_dir().unwrap();
        let home_dir = cargo::util::homedir(&current_dir).unwrap();
        let config = Config::new(shell, current_dir, home_dir);
        Self {
            logs,
            config
        }
    }

    /// Return logs as a String
    pub fn logs(&self) -> String{
        self.logs.lock().unwrap().join("")
    }
}

impl std::ops::Deref for CargoConfig {
    type Target = Config;
    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl std::ops::DerefMut for CargoConfig {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.config
    }
}

/// Implements [std::io::Write] and be used by Cargo. It stores the 
/// output logs during cargo building process.
#[derive(Default)]
pub struct BuildLogWritter {
    pub buffer: Arc<Mutex<Vec<String>>>
}

impl Write for BuildLogWritter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(ref mut mutex) = self.buffer.try_lock() {
            mutex.push(String::from_utf8_lossy(buf).to_string());
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}