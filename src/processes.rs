/*
    Copyright © 2023, ParallelChain Lab 
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Defines a struct that contains the processes for contract building, with enums of Error Codes for exiting the processes.

use std::process::{Command, Child};
use thiserror::Error;

pub(crate) struct Processes {
    pub(crate) children: Vec<Child>,
    pub(crate) container_name: String,
}

impl Drop for Processes {
    fn drop(&mut self) {
        for p in &mut self.children {
            match p.try_wait() {
                Ok(exit_status) => {
                    if exit_status.is_none() {
                        match p.kill() {
                            Err(e) => println!("Could not kill child process: {}", e),
                            Ok(_) => println!("Killed child process successfully."),
                        }
                    }
                },
                Err(e) => println!("Error attempting to wait: {}", e),
            }
        }

        let (shell ,option, suppress_output)  = match cfg!(target_os = "windows") {
            false => ("/bin/bash", "-c", ">/dev/null"),
            true =>  ("cmd", "/C", ">nul"),
        }; 

        match Command::new(shell)
            .arg(option)
            .arg(&format!("docker stop {container} {silence} && \
                           docker rm {container} {silence}",container=self.container_name, silence=suppress_output))  
            .status() {
                Ok(p) => {
                    if !p.success() {
                        println!("Docker container '{}' cannot be stopped and removed.", self.container_name);
                    }
                },
                Err(_) => println!("Docker container '{}' cannot be stopped and removed.", self.container_name),
        }; 
    }
}


/// ProcessExitCode is enum that describes the exit status codes during building process.
#[derive(Error, Debug)]
pub enum ProcessExitCode {
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
    InvalidDependenecyPath,
}