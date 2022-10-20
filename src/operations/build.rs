/*
 Copyright (c) 2022 ParallelChain Lab
 
 This program is free software: you can redistribute it and/or modify
 it under the terms of the GNU General Public License as published by
 the Free Software Foundation, either version 3 of the License, or
 (at your option) any later version.
 
 This program is distributed in the hope that it will be useful,
 but WITHOUT ANY WARRANTY; without even the implied warranty of
 MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 GNU General Public License for more details.
 
 You should have received a copy of the GNU General Public License
 along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/
use cargo_toml::Manifest;
use dunce;
use faccess::{AccessMode, PathExt};
use rand::{distributions::Alphanumeric, Rng, thread_rng};
use std::{fs, process::Command};
use thiserror::Error;

pub(crate) struct Processes {
    children: Vec<std::process::Child>,
    container_name: String,
}

impl Drop for Processes {
    fn drop(&mut self) {
        for p in &mut self.children {
            match p.try_wait() {
                Ok(exit_status) => {
                    if exit_status.is_none() {
                        match p.kill() {
                            Err(e) => {
                                println!("Could not kill child process: {}", e)
                            },
                            Ok(_) => {
                                println!("Killed child process successfully.")
                            },
                        }
                    }
                },
                Err(e) => {
                    println!("Error attempting to wait: {}", e)
                },
            }
        }

        let (path_to_shell ,option, suppress_output)  = match cfg!(target_os = "windows") {
            false => {
                ("/bin/bash", "-c", ">/dev/null")
            },
            true =>  {
                ("cmd", "/C", ">nul")
            },
        }; 

        println!("Cleanup in progress. Please do not press Ctrl+C...");

        match Command::new(path_to_shell)
            .arg(option)
            .arg(&format!("docker stop {container} {silence} && \
                           docker rm {container} {silence}",container=self.container_name, silence=suppress_output))  
            .status() {
                Ok(p) => {
                   match p.success() {
                        true => {
                            println!("All artifacts from pchain-compile have been successfully stopped and removed.")
                        }, 
                        false => {
                            println!("Docker container '{}' cannot be stopped and removed.", self.container_name)
                        },
                    }; 
                },
                Err(_) => {
                    println!("Docker container '{}' cannot be stopped and removed.", self.container_name)
                },
        }; 
    }
}

// `build` takes the path to the cargo manifest file, generates an optimized WASM binary after building
// the source code and saves it to the designated destination_path.
pub async fn build(manifest_path:String, destination_path:String) -> Result<String, ProcessExitCode> {
    // create destination directory if it does not exist   
    match fs::create_dir_all(destination_path.to_owned()) {
        Ok(dir) => {
            dir
        },
        Err(_) => {
            return Err(ProcessExitCode::InvalidPath);
        },
    };

    // get absolute path to the destination directory
    // also check if destination path has write privileges.
    let absolute_destination_path = match dunce::canonicalize(destination_path) {
        Ok(dir) => {
            let write_path = match dir.access(AccessMode::WRITE) {
                Ok(_)  => {
                    String::from(dir.to_string_lossy())
                },
                Err(_) =>{
                    return Err(ProcessExitCode::InvalidPath);
                },
            };
            write_path
        },
        Err(_) => {
            return Err(ProcessExitCode::Unknown);
        }
    };

    if manifest_path.contains(" ") {
       return Err(ProcessExitCode::InvalidFilePath);
    }

    // check if the manifest file exists on the path supplied   
    let manifest_file = match Manifest::from_path(format!("{}{}", &manifest_path, &"/Cargo.toml")) {
        Ok(manifest) => {
            manifest
        },
        Err(_) => {
            return Err(ProcessExitCode::ManifestFailure);
        },
    };

    // retrieve the package name from the manifest file and append the extension to package name.
    let wasm_file = format!("{}{}", &(manifest_file.package.as_ref().unwrap()).name, &".wasm").replace("-", "_");

    // generate random string for container name.
    // This makes the build process shell agnostic
    let container_name = get_random_string();
    let mut child_process = Processes{ children: vec![], container_name: container_name.clone() };

    // A thread to handle destructor when a “ctrl-c” notification is sent to the process
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {},
            Err(_) => println!("Failed to compile.\nDetails: Docker Daemon Failure. Check if Docker is running on your machine and confirm read/write access privileges."),
        }
    });
    
    println!("Build process started. This could take several minutes for large contracts.");

    let (path_to_shell ,option, suppress_output)  = match cfg!(target_os = "windows") {
        false => ("/bin/bash", "-c", ">/dev/null"),
        true =>  ("cmd", "/C", ">nul"),
    };  
    
    // pull latest pchain_compile image from DockerHub.
    // run the image as a container with name <container_name>.   
    if let Ok(mut process_docker) = Command::new(path_to_shell)
    .arg(option)
    .arg(&format!("docker pull parallelchainlab/pchain_compile:latest {silence} && \
                   docker run -it -d --name {container} parallelchainlab/pchain_compile:latest {silence}", container=container_name, silence=&suppress_output.to_owned()))
    .spawn() {
        match process_docker.wait() {
            Ok(status) => {
                child_process.children.push(process_docker);

                match status.success() {
                    true => { 
                        status 
                    }, 
                    // process got terminated with non-zero ExitStatus. Sources of error DockerHub, docker service closed
                    false => { 
                        return Err(ProcessExitCode::DockerDaemonFailure) 
                    },
                }; 
            },
            // should never reach here
            Err(_) => { 
                unreachable!() 
            },
        };
    }
    else {
        return Err(ProcessExitCode::DockerDaemonFailure)
    };
    
    // spawn child process to build the source code inside docker container <container_name>.    
    // build the docker container, run the container, copy the source code inside the container,
    // execute cargo build inside the container to generate the WASM binary, use wasm-opt and wasm-snip to optimize the file,
    // move the file from the container to the destination path and remove the container.                                                 
    if let Ok(mut process_build) = Command::new(path_to_shell).arg(option)
        .arg(&format!("docker cp {manifest}/. {container}:/home/ {silence} && \
                       docker exec -w /home/ {container} cargo build --target wasm32-unknown-unknown --release {silence} && \
                       docker exec -w /home/target/wasm32-unknown-unknown/release {container} chmod +x /root/bin/wasm-opt {silence} && \
                       docker exec -w /home/target/wasm32-unknown-unknown/release {container} /root/bin/wasm-opt -Oz {wasm} --output temp.wasm {silence} && \
                       docker exec -w /home/target/wasm32-unknown-unknown/release {container} wasm-snip temp.wasm --output temp2.wasm --snip-rust-fmt-code --snip-rust-panicking-code {silence} && \
                       docker exec -w /home/target/wasm32-unknown-unknown/release {container} /root/bin/wasm-opt --dce temp2.wasm --output optimized.wasm {silence} && \
                       docker exec -w /home/target/wasm32-unknown-unknown/release {container} mv optimized.wasm /home/{wasm} {silence} && \
                       docker cp {container}:/home/{wasm} {destination} {silence} ", container=&container_name, manifest=&manifest_path, wasm=&wasm_file, destination=&absolute_destination_path, silence=&suppress_output))
        .spawn() {
        match process_build.wait() {
            Ok(status) => {
                child_process.children.push(process_build);
                match status.success() {
                    true => { 
                        status 
                    }, 
                    // process got terminated with non-zero ExitStatus. Sources of error build failure,external process,software interrupts  
                    false => { 
                        return Err(ProcessExitCode::BuildFailure)
                    },
                }; 
            },
            // should never reach here
            Err(_) => {
                unreachable!() 
            }
        };
    }
    else{
        return Err(ProcessExitCode::DockerDaemonFailure)
    };

    Ok(format!("Finished compiling. ParallelChain F smart contract ({}) is saved at ({}).", wasm_file, absolute_destination_path).to_string())
}

// get_random_string generates a random string 
// for naming the docker container 
fn get_random_string() -> String {
    let s: String = thread_rng()
    .sample_iter(&Alphanumeric)
    .take(5)
    .map(char::from)
    .collect();
    s
}

// ProcessExitCode enum describes the 
// exit code status codes for pchain_compile
#[derive(Error, Debug)]
pub enum ProcessExitCode {
    #[error("The source code did not compile.")]
    BuildFailure,
 
    #[error("Docker daemon service did not respond.")]
    DockerDaemonFailure,

    #[error("Some artifacts downloaded by pchain_compile were not successfully removed.")]
    ArtifactRemovalFailure,

    #[error("Manifest file not found")]
    ManifestFailure,

    #[error("Destination path not found")]
    InvalidPath,

    #[error("Blank spaces present in absolute file path")]
    InvalidFilePath,

    #[error("Process Failure Unknown")]
    Unknown,
}

