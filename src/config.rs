/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Configuration of pchain_compile. The struct `Config` specifies parameters being used, and
//! provide a method `run` to start the compilation process.

use std::path::PathBuf;

use crate::error::Error;

/// Configuration to compile smart contract.
#[derive(Clone, Default)]
pub struct Config {
    /// Path to source code folder.
    pub source_path: PathBuf,
    /// Path to destination folder. None if current folder should be used.
    pub destination_path: Option<PathBuf>,
    /// Compilation option regards to use of docker.
    pub docker_option: DockerOption,
}

/// Compilation option regards to docker.
#[derive(Clone)]
pub enum DockerOption {
    /// Compile contract in docker container. (Default)
    Docker(DockerConfig),
    /// Compile contract without using Docker.
    Dockerless,
}

impl Default for DockerOption {
    fn default() -> Self {
        Self::Docker(DockerConfig::default())
    }
}

#[derive(Clone, Default)]
pub struct DockerConfig {
    /// Docker Image tag.
    pub tag: Option<String>,
}

impl Config {
    pub async fn run(self) -> Result<String, Error> {
        match self.docker_option {
            DockerOption::Docker(docker_config) => {
                crate::build::build_target_with_docker(
                    self.source_path,
                    self.destination_path,
                    docker_config,
                )
                .await
            }
            DockerOption::Dockerless => {
                crate::build::build_target_without_docker(self.source_path, self.destination_path)
                    .await
            }
        }
    }
}
