/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Defines enum of Error Codes for exiting the processes.

use thiserror::Error;

/// Describes the exit status codes during building process.
#[derive(Error, Debug)]
pub enum Error {
    #[error("The source code did not compile.")]
    BuildFailure(String),

    #[error("Docker daemon service did not respond.")]
    DockerDaemonFailure,

    #[error("Some artifacts downloaded by pchain-compile were not successfully removed.")]
    ArtifactRemovalFailure,

    #[error("Manifest file not found.")]
    ManifestFailure,

    #[error("Source code path not valid.")]
    InvalidSourcePath,

    #[error("Destination path not valid.")]
    InvalidDestinationPath,

    #[error("Dependency path not valid.")]
    InvalidDependencyPath,
}

impl Error {
    pub fn detail(&self) -> String {
        match self {
            Error::ArtifactRemovalFailure => "The compilation was successful, but pchain-compile failed to stop its Docker containers. Please remove them manually.".to_string(), 
            Error::BuildFailure(e) => format!("\nDetails: {}. Please rectify the errors and build your source code again.", &e),
            Error::DockerDaemonFailure => "Failed to compile.\nDetails: Docker Daemon Failure. Check if Docker is running on your machine and confirm read/write access privileges.".to_string(),
            Error::ManifestFailure => "Failed to compile.\nDetails: Manifest File Not Found. Check if the manifest file exists on the source code path.".to_string(),
            Error::InvalidSourcePath => "Failed to compile.\nDetails: Source Code Path Not Valid. Check if you have provided the correct path to your source code directory and confirm write access privileges.".to_string(),
            Error::InvalidDestinationPath => "\nDetails: Destination Path Not Valid. Check if you have provided the correct path to save your optimized WASM binary and confirm write access privileges.".to_string(),
            Error::InvalidDependencyPath => "\nDetails: Dependency Paths specified within Smart Contract Crate Not Valid. Check if you have provided the correct path to the dependencies on your source".to_string(),
        }
    }
}