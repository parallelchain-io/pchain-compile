/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! `pchain_compile` is a command line interface tool to build ParallelChain Smart Contract that can be deployed to
//! ParallelChain Mainnet. It takes a ParallelChain Smart Contract written in Rust and builds by Cargo
//! in a docker environment.

use clap::Parser;
use std::path::{PathBuf, Path};

mod build;

mod docker;

mod error;

mod manifests;

#[derive(Debug, Parser)]
#[clap(
    name = "pchain-compile",
    version = env!("CARGO_PKG_VERSION"), 
    about = "ParallelChain Smart Contract Compile CLI\n\n\
             A command line tool for reproducibly building Rust code into compact, gas-efficient WebAssembly ParallelChain Smart Contract.", 
    author = "<ParallelChain Lab>", 
    long_about = None
)]
enum PchainCompile {
    /// Build the source code. Please make sure:
    /// 1. Docker is installed and its execution permission under current user is granted.
    /// 2. Internet is reachable. (for pulling the docker image from docker hub)
    #[clap(
        arg_required_else_help = false,
        display_order = 1,
        verbatim_doc_comment
    )]
    Build {
        /// Absolute/Relative path to the source code directory. This field can be used multiple times to build multiple contracts at a time, e.g. 
        /// 
        /// --source <path to contract A> --source <path to contract B>
        #[clap(long = "source", display_order = 1, verbatim_doc_comment)]
        source_path: Vec<PathBuf>,
        /// Absolute/Relative path for saving the compiled optimized wasm file.
        #[clap(long = "destination", display_order = 2, verbatim_doc_comment)]
        destination_path: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() {
    let args = PchainCompile::parse();
    match args {
        PchainCompile::Build {
            source_path,
            destination_path,
        } if !source_path.is_empty() => {
            println!("Build process started. This could take several minutes for large contracts.");

            // Spawn threads to handle each contract code
            let mut join_handles = vec![];
            source_path.into_iter().for_each(|source_path|{
               join_handles.push(tokio::spawn(crate::build::build_target(source_path, destination_path.clone())));
            });

            // Join threads to obtain results
            let mut results = vec![];
            for handle in join_handles {
                results.push(handle.await.unwrap());
            }

            // Display the results
            let (
                success, 
                fails
            ): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

            if !success.is_empty() {
                let dst_path = destination_path.clone().unwrap_or(Path::new(".").to_path_buf());
                let contracts: Vec<String> = success.into_iter().map(|r| r.ok().unwrap()).collect();
                print!("Finished compiling. ParallelChain Mainnet smart contract(s) {:?} are saved at ({})", contracts, crate::manifests::get_absolute_path(dst_path.as_os_str().to_str().unwrap()).unwrap());
            }
            
            if !fails.is_empty() {
                fails.into_iter().for_each(|e|{
                    let error = e.err().unwrap();
                    println!("{}\n{}\n", error, error.detail());
                });
            }
        },
        _=>{
            println!("Invalid Command");
        }
    };
}