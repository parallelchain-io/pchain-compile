/*
    Copyright © 2023, ParallelChain Lab 
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! `pchain_compile` is a command line interface tool to build ParallelChain Smart Contract that can be deployed to
//! ParallelChain Mainnet. It takes a ParallelChain Smart Contract which is written in Rust, and then builds by Cargo
//! in a docker environment. 

use std::env;
use clap::Parser;

pub mod build;
use build::build_target;

pub mod processes;
use processes::ProcessExitCode;

#[derive(Debug, Parser)]
#[clap(
    name = "pchain-compile",
    version = "0.4.0", 
    about = "ParallelChain Smart Contract Compile CLI\n\n\
             A command line tool for reproducibly building Rust code into compact, gas-efficient WebAssembly ParallelChain Smart Contract.", 
    author = "<ParallelChain Lab>", 
    long_about = None
)]
/// asdasdasd
enum PchainCompile {
    /// Build the source code. Please make sure:
    /// 1. Docker is installed and its execution permission under current user is granted.
    /// 2. Internet is reachable. (for pulling the docker image from docker hub)
    #[clap(arg_required_else_help = false, display_order=1, verbatim_doc_comment)]
    Build {
        /// Path to the source code directory.
        /// 1. The path to the source code of the ParallelChain Mainnet smart contract. 
        /// 2. This is a *mandatory* field.
        /// 3. If "." is passed to the source field the path will be set to the current working. directory. 
        #[clap(long="source", display_order=1, verbatim_doc_comment)]
        source_path : String,
        /// Path to the compiled optimized wasm file. 
        /// 1. The path where the resulting WASM smart contract with the name <package_name>.wasm will be stored. package_name is the name of the package denoted in the manifest file of the source code directory. 
        /// 2. This is an *optional* field. pchain_compile saves the generated wasm file to the **source** path if this flag is not supplied.
        /// 3. If "." is passed to the destination field the destination path will be set to the current working directory. 
        #[clap(long="destination", display_order=2, verbatim_doc_comment)]
        destination_path : Option<String>,
    },
}
 
#[tokio::main]
async fn main() {
    let args = PchainCompile::parse();
    match args {
        PchainCompile::Build { source_path, destination_path } => {
            let path: String = String::from(env::current_dir().unwrap().to_string_lossy());
            let patterns : &[_] = &['~', '!', '"', '/'];

            let mut source_path = String::from(source_path.trim_end_matches(patterns));
            if source_path.to_lowercase() == "." { 
                source_path = path.clone();
            }

            let destination = match destination_path {
                Some(dir) => {
                    match dir.to_lowercase().as_ref() {
                        "." => path,
                        _ => dir,
                    }
                },
                None => path,
            };

            let result: String = match build_target(&source_path, &destination).await {
                Ok(res) => res,
                Err(e) => match_error(e).to_string(),
            };

            println!("{}", result);
        },
    };
}


/// match_error matches ProcessExitCode 
fn match_error(error: ProcessExitCode) -> &'static str {
    match error { 
        ProcessExitCode::ArtifactRemovalFailure => "The compilation was successful, but pchain-compile failed to stop its Docker containers. Please remove them manually.", 
        ProcessExitCode::BuildFailure(e) => Box::leak(format!("\nDetails: {}. Please rectify the errors and build your source code again.", &e).into_boxed_str()),        
        ProcessExitCode::DockerDaemonFailure => "Failed to compile.\nDetails: Docker Daemon Failure. Check if Docker is running on your machine and confirm read/write access privileges.",
        ProcessExitCode::ManifestFailure => "Failed to compile.\nDetails: Manifest File Not Found. Check if the manifest file exists on the source code path.",
        ProcessExitCode::InvalidSourcePath => "Failed to compile.\nDetails: Source Code Path Not Valid. Check if you have provided the correct path to your source code directory and confirm write access privileges.",
        ProcessExitCode::InvalidDestinationPath => "\nDetails: Destination Path Not Valid. Check if you have provided the correct path to save your optimized WASM binary and confirm write access privileges.",
        ProcessExitCode::InvalidDependenecyPath => "\nDetails: Dependency Paths specified within Smart Contract Crate Not Valid. Check if you have provided the correct path to the dependencies on your source",
    }
}
