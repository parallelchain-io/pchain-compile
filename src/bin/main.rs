/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! `pchain_compile` is a command line interface tool to build ParallelChain Smart Contract that can be deployed to
//! ParallelChain Mainnet. It takes a ParallelChain Smart Contract written in Rust and builds by Cargo
//! in a docker environment.

use clap::Parser;
use pchain_compile::{config::Config, DockerConfig, DockerOption};
use std::path::{Path, PathBuf};

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
    #[clap(arg_required_else_help = true, display_order = 1, verbatim_doc_comment)]
    Build {
        /// Absolute/Relative path to the source code directory. This field can be used multiple times to build multiple contracts at a time.
        /// For example,
        /// --source <path to contract A> --source <path to contract B>
        #[clap(long = "source", display_order = 1, verbatim_doc_comment)]
        source_path: Vec<PathBuf>,
        /// Absolute/Relative path for saving the compiled optimized wasm file.
        #[clap(long = "destination", display_order = 2, verbatim_doc_comment)]
        destination_path: Option<PathBuf>,

        /// Compile contract without using docker. This option requires installation of Rust and target "wasm32-unknown-unknown".
        /// **Please note the compiled contracts are not always consistent with the previous compiled ones, because the building 
        /// process happens in your local changing environment.**
        /// 
        /// To install target "wasm32-unknown-unkown", run the following command:
        ///
        /// $ rustup target add wasm32-unknown-unknown
        #[clap(
            long = "dockerless",
            display_order = 3,
            verbatim_doc_comment,
            group = "docker-option"
        )]
        dockerless: bool,

        /// Tag of the docker image being pulled from Dockerhub. Please find the tags information in
        /// https://hub.docker.com/r/parallelchainlab/pchain_compile.
        ///
        /// Available tags:
        /// - mainnet01
        /// - 0.4.2
        #[clap(
            long = "use-docker-tag",
            display_order = 4,
            verbatim_doc_comment,
            group = "docker-option"
        )]
        docker_image_tag: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let args = PchainCompile::parse();
    match args {
        PchainCompile::Build {
            source_path,
            destination_path,
            dockerless,
            docker_image_tag,
        } => {
            if source_path.is_empty() {
                println!("Please provide at least one source!");
                std::process::exit(-1);
            }
            println!("Build process started. This could take several minutes for large contracts.");

            let docker_option = if dockerless {
                DockerOption::Dockerless
            } else {
                DockerOption::Docker(DockerConfig {
                    tag: docker_image_tag,
                })
            };

            // Spawn threads to handle each contract code
            let mut join_handles = vec![];
            source_path.into_iter().for_each(|source_path| {
                let config = Config {
                    source_path,
                    destination_path: destination_path.clone(),
                    docker_option: docker_option.clone(),
                };

                join_handles.push(tokio::spawn(config.run()));
            });

            // Join threads to obtain results
            let mut results = vec![];
            for handle in join_handles {
                results.push(handle.await.unwrap());
            }

            // Display the results
            let (success, fails): (Vec<_>, Vec<_>) = results.into_iter().partition(Result::is_ok);

            if !success.is_empty() {
                let dst_path = destination_path
                    .clone()
                    .unwrap_or(Path::new(".").to_path_buf());
                let contracts: Vec<String> = success.into_iter().map(|r| r.ok().unwrap()).collect();
                println!("Finished compiling. ParallelChain Mainnet smart contract(s) {:?} are saved at ({})", contracts,  dunce::canonicalize(dst_path).unwrap().to_str().unwrap());
            }

            if !fails.is_empty() {
                println!("Compiling fails.");
                fails.into_iter().for_each(|e| {
                    let error = e.err().unwrap();
                    println!("{}\n{}\n", error, error.detail());
                });
            }
        }
    };
}
