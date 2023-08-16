/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! `pchain_compile` is a library to build ParallelChain Smart Contract that can be deployed to
//! ParallelChain Mainnet. It takes a ParallelChain Smart Contract written in Rust and builds by
//! Cargo in a docker environment.
//!
//! # Example
//! ```no_run
//! let source_path = Path::new("/home/user/contract").to_path_buf();
//! let result = pchain_compile::build_target(source_path, None).await;
//! ```
//! 
//! # Example - Run from a configuration
//! ```no_run
//! let result = pchain_compile::Config {
//!     source_path: Path::new("/home/user/contract").to_path_buf(),
//!     docker_option: pchain_compile::DockerOption::Dockerless,
//!     ..Default::default()
//! }
//! .run()
//! .await;
//! ```

pub(crate) mod cargo;

pub mod config;
pub use config::*;

pub(crate) mod docker;

pub mod error;

pub(crate) mod manifests;

pub mod build;
pub use build::build_target;
