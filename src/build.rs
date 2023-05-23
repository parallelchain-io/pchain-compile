/*
    Copyright © 2023, ParallelChain Lab 
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implementation of contracts compilation.
//! 
//! The flow of the compilation process is as follows:
//! 1. Setup destination folders and parse the command arguments into in-memory data that is to be used in subsequent steps.
//!    Also pull the docker image from docker hub.
//! 2. Create file structures in the docker container, and then copy the source code to it. It also applies to the libraries 
//!    that are using relative paths in dependencies.
//! 3. Compile the source code in the docker container. The dependencies (if any) are compile first.
//! 4. After compilation, copy the binary (wasm) from docker container to target destination.

use std::thread;
use cargo_toml::Manifest;
use dunce;
use faccess::{AccessMode, PathExt};

use rand::{distributions::Alphanumeric, Rng, thread_rng};
use std::{
    fs, process::Command, sync::{Arc, RwLock}
};

use crate::processes::{ProcessExitCode, Processes};

/// `build_target` takes the path to the cargo manifest file(s), generates an optimized WASM binary(ies) after building
/// the source code and saves the binary(ies) to the designated destination_path.
///
/// ### Arguments
/// * `source_path` - Absolute/Relative path to the source code file
/// * `destination_path` - Absolute/Relative path where the wasm file(s) will be saved
pub async fn build_target(source_path: &str, destination_path: &str) -> Result<String, ProcessExitCode> {
    let (
         absolute_destination_path, 
         package_name, 
         container_name, 
         absolute_source_path
    ) = match prepare_env(&source_path, &destination_path) {
           Ok(paths) => paths,
           Err(e) => return Err(e)
    };

    let dependencies = &mut vec![];
    let mut child_process = Processes{ children: vec![], container_name: container_name.clone()};
    let packages_built = Arc::new(RwLock::new(Vec::with_capacity(package_name.len())));
    let packages_failed = Arc::new(RwLock::new(Vec::with_capacity(package_name.len())));
    
    // A thread to handle destructor when a “ctrl-c” notification is
    // sent to the pchain_compile build process.
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(_) => println!("Failed to compile.\nDetails: Process interrupted."),
            Err(_) => println!("Failed to cleanup.\nDetails: Docker Daemon Failure. Check if Docker is running on your machine and confirm read/write access privileges."),
        }
    }); 

    println!("Build process started. This could take several minutes for large contracts.");

    let (shell ,option, suppress_output)  = match cfg!(target_os = "windows") {
        false => ("/bin/bash", "-c", ">/dev/null"),
        true =>  ("cmd", "/C", ">nul"),
    };  
 
    // pull latest pchain-compile image from DockerHub.
    // run the image as a container with name <container_name>. 
    if let Ok(mut process_docker_setup) = 
        Command::new(shell)
            .arg(option)
            .arg(&format!("docker pull parallelchainlab/pchain_compile:mainnet01 {silence} && \
                           docker run -it -d --privileged --name {container} parallelchainlab/pchain_compile:mainnet01 {silence}", container=container_name, silence=&suppress_output))
            .spawn() {
                match process_docker_setup.wait() {
                    Ok(status) => {
                        child_process.children.push(process_docker_setup);
                        match status.success() {
                            true => status, 
                            false => return Err(ProcessExitCode::DockerDaemonFailure),
                        }; 
                    },
                    Err(_) => unreachable!(),
                };
            } else {
                return Err(ProcessExitCode::DockerDaemonFailure)
            };

    // retrieve dependency paths from manifest.
    match get_dependency_paths(&absolute_source_path.first().unwrap(), dependencies) {
        Ok(paths) => paths,
        Err(e) => return Err(e)
    };

    for dependency in dependencies {
        // Step 1: create dependency directory 
        if let Ok(process_create_dependency) = create_directory(option, shell, &container_name, &dependency) {
            child_process.children.push(process_create_dependency);
        } else {
            return Err(ProcessExitCode::DockerDaemonFailure);
        }
        
        // Step 2: copy source to docker 
        if let Ok(process_copy_dependency) = copy_to(shell, &dependency, &container_name) {
            child_process.children.push(process_copy_dependency);
        } else {
            return Err(ProcessExitCode::DockerDaemonFailure);
        }
    }

    let handle = 
    |absolute_source_path: Vec<String>, 
     package_name: Vec<String>, 
     absolute_destination_path: String, 
     container_name: String,
     packages_built: Arc<RwLock<Vec<String>>>,
     packages_failed: Arc<RwLock<Vec<String>>>|
        thread::spawn(move || {
            for i in 0..package_name.len()  {
                let wasm_file = &format!("{}{}", &package_name[i], &".wasm").replace("-", "_");
                // spawn child process to build the source code inside docker container <container_name>.    
                // build the docker container, run the container, copy the source code inside the container,
                // execute cargo build inside the container to generate the WASM binary, use wasm-opt and wasm-snip to optimize the file,
                // move the file from the container to the destination path and remove the container.

                // Step 1: create directory paths inside docker
                if let Ok(process_create_directory) = create_directory(option, shell, &container_name, &absolute_source_path[i]) {
                    child_process.children.push(process_create_directory);
                } else {
                    let mut package = packages_failed.write().unwrap();
                    package.push(wasm_file.to_string());
                    continue;
                }

                // Step 2: copy file to container 
                if let Ok(process_copy) = copy_to(shell, &absolute_source_path[i], &container_name) {
                    child_process.children.push(process_copy);
                } else {
                    let mut package = packages_failed.write().unwrap();
                    package.push(wasm_file.to_string());
                    continue;
                }

                // Step 3. build the source code inside docker
                if let Ok(mut process_build) = Command::new(shell)
                    .arg(option)
                    .arg(&format!("docker exec -w /{manifest}/ {container_name} cargo build --target wasm32-unknown-unknown --release --quiet && \
                                   docker exec -w /{manifest}/target/wasm32-unknown-unknown/release {container_name} chmod +x /root/bin/wasm-opt && \
                                   docker exec -w /{manifest}/target/wasm32-unknown-unknown/release {container_name} /root/bin/wasm-opt -Oz {wasm_file} --output temp.wasm && \
                                   docker exec -w /{manifest}/target/wasm32-unknown-unknown/release {container_name} wasm-snip temp.wasm --output temp2.wasm --snip-rust-fmt-code --snip-rust-panicking-code && \
                                   docker exec -w /{manifest}/target/wasm32-unknown-unknown/release {container_name} /root/bin/wasm-opt --dce temp2.wasm --output optimized.wasm  && \
                                   docker exec -w /{manifest}/target/wasm32-unknown-unknown/release {container_name} mv optimized.wasm /{wasm_file}", manifest=&absolute_source_path[i].replace(":", "").replace("\\", "/").replace(" ", "_")))
                    .spawn() {
                    match process_build.wait() {
                        Ok(status) => {
                            child_process.children.push(process_build);
                            match status.success() {
                                true => { 
                                    let mut package = packages_built.write().unwrap();
                                    package.push(wasm_file.to_string());
                                    status 
                                }, 
                                false => {
                                    let mut package = packages_failed.write().unwrap();
                                    package.push(wasm_file.to_string());
                                    continue;
                                },
                            }; 
                        },
                        Err(_) => unreachable!(),
                    };
                }
                else {
                    continue;
                };  

                // Step 4: copy file from docker to given location
                if let Ok(process_copy) = copy_from(&absolute_destination_path, &container_name, &wasm_file) {
                    child_process.children.push(process_copy);
                } else {
                    let mut package = packages_failed.write().unwrap();
                    package.push(wasm_file.to_string());
                    continue;
                }
            }
        });
 
    handle(
        absolute_source_path, 
        package_name, 
        absolute_destination_path.clone(), 
        container_name,
        Arc::clone(&packages_built),
        Arc::clone(&packages_failed)
    )
    .join()
    .expect("pchain-compile could not complete the build process.");

    if packages_failed.read().unwrap().len() > 0 {
        if packages_built.read().unwrap().len() > 0 {
            println!("\nFinished compiling with errors. ParallelChain Mainnet smart contract(s) {:?} are saved at ({}).", packages_built.read().unwrap(), &absolute_destination_path)
        }

        return Err(ProcessExitCode::BuildFailure(format!("The following packages could not be built due to errors: {:?}", packages_failed.read().unwrap())));
    } else {

        Ok(format!("\nFinished compiling. ParallelChain Mainnet smart contract(s) {:?} are saved at ({}).", packages_built.read().unwrap(), &absolute_destination_path))
    }
} 

/// `prepare_env` sets up initial variables for pchain_compile build process.
///
/// ### Arguments
/// * `source_path` - Absolute/Relative path to the source code file
/// * `destination_path` - Absolute/Relative path where the wasm file(s) will be saved
fn prepare_env(source_path: &str, destination_path: &str) -> Result<(String, Vec<String>, String, Vec<String>), ProcessExitCode> {
    let mut package_name = Vec::new();
    let mut absolute_source_paths = Vec::new();
    let absolute_destination_path = get_absolute_path(destination_path)?;

    // create destination directory if it does not exist.   
    match fs::create_dir_all(destination_path) {
        Ok(file) => file,
        Err(_) => return Err(ProcessExitCode::InvalidDestinationPath),
    };

    // check if the manifest file exists on the path supplied.   
    if let Err(_) = collect_dependency_paths_from_manifest(source_path, &mut package_name, &mut absolute_source_paths) {
        package_name.clear();
        absolute_source_paths.clear();
        let files = match fs::read_dir(&source_path) {
            Ok(f) => f,
            Err(_) => return Err(ProcessExitCode::InvalidSourcePath)
        };

        for file in files {
            if collect_dependency_paths_from_manifest(
                &(file.unwrap().path().display().to_string()), 
                &mut package_name, 
                &mut absolute_source_paths
            )
            .is_err(){
                continue;
            }
        }
    }

    Ok((absolute_destination_path, package_name, get_random_string(), absolute_source_paths))
}

/// `get_dependency_paths` returns absolute paths for dependencies from local manifests.
///
/// ### Arguments
/// * `absolute_source_path` - absolute path to the source code file
/// * `dependencies` - list for storing dependencies from source code manifest
fn get_dependency_paths(absolute_source_path: &str, dependencies: &mut Vec<String>) -> Result<(), ProcessExitCode> {
    let source_manifest = match Manifest::from_path(format!("{}{}", &absolute_source_path, &"/Cargo.toml")) {
        Ok(file) => file,
        Err(_) => return Err(ProcessExitCode::ManifestFailure),
    };

    for (_ ,dependency) in &source_manifest.dependencies {
        if dependency.detail().is_some() {
            if dependency.detail().unwrap().path.is_none() {
                continue;
            }
            let current_path = dependency.detail().unwrap().path.as_ref().unwrap();
            let derived_path = match get_absolute_path(&current_path) {
                Ok(p) => p,
                Err(_) => {
                    get_absolute_path(&format!("{}/{}", absolute_source_path, current_path))?
                },
            };

            // currently pchain_compile only recurses till depth 1
            if let  Err(_) = get_dependency_paths(&derived_path, dependencies) {
                continue;
            } else {
                dependencies.push(derived_path);
            }
        }
    }

   Ok(())
}

/// `collect_dependency_paths_from_manifest` collects paths from a manifest file.
///
/// ### Arguments
/// * `absolute_source_path` - absolute path to the source code file
/// * `current_dir` - current directory of the smart contract source code
/// * `package_name` - package name listed on manifest file
fn collect_dependency_paths_from_manifest(current_dir: &str, package_name: &mut Vec<String>, absolute_source_path: &mut Vec<String>) -> Result<(), ProcessExitCode> {
    match Manifest::from_path(format!("{}{}", &current_dir, &"/Cargo.toml")) {
        Ok(f) => {
            package_name.push((f.package.as_ref().unwrap()).name.to_string());
            absolute_source_path.push(get_absolute_path(current_dir)?);
        },
        Err(_) => return Err(ProcessExitCode::ManifestFailure),
    };

    Ok(())
}

/// `get_absolute_path` is a helper which returns absolute path for a current directory.
///
/// ### Arguments
/// * `current_dir` - current directory of the smart contract source code
fn get_absolute_path(current_dir: &str) -> Result<String, ProcessExitCode> {
    // get canonicalized path to the directory.
    let canonicalized_path = match dunce::canonicalize(current_dir) {
        Ok(path) => path,
        Err(_) => return Err(ProcessExitCode::InvalidDependenecyPath),
    };

    // also check if pchain-compile has write privileges to the canonicalized path.
    // if check succeeds, get absolute path to the directory.
    let absolute_path = match canonicalized_path.access(AccessMode::WRITE) {
        Ok(_)  => String::from(canonicalized_path.to_string_lossy()),
        Err(_) => return Err(ProcessExitCode::InvalidDependenecyPath),
    };

    Ok(absolute_path)
}

/// `copy_to` is a helper which copies source code from a path to a designated 
/// location in the docker container spawned by `pchain_compile`.
///
/// ### Arguments
/// * `shell` is an attribute dependent on current OS.
/// * `absolute_source_path` - absolute path to the source code file
/// * `container_name` - name of the target container 
fn copy_to(shell: &str, absolute_source_path: &str, container_name: &str) -> Result<std::process::Child, ProcessExitCode> {
    let source_command = match shell {
        "/bin/bash" => format!(r#"{source}/."#, source = &absolute_source_path),
        _ => format!(r#"{source}\."#, source = &absolute_source_path)
    };

    if let Ok(mut copy) = Command::new("docker")
        .arg("cp")
        .arg(source_command)
        .arg(format!("{container_name}:/{manifest}/", manifest = &absolute_source_path.replace(":", "").replace("\\", "/").replace(" ", "_")))
        .spawn() {
        match copy.wait() {
            Ok(_status) => {
                return Ok(copy);
            },
            Err(_)=> {
                return Err(ProcessExitCode::DockerDaemonFailure);
            },
        };
    } else {
        return Err(ProcessExitCode::DockerDaemonFailure);
    }
}

/// `copy_from` is a helper which copies source code from the docker container spawned by `pchain_compile` 
/// to a designated location in the file system.
///
/// ### Arguments
/// * `absolute_source_path` - absolute path to the source code file
/// * `container_name` - name of the target container 
/// * `wasm` - absolute path to the cargo manifest file for the smart contract 
fn copy_from(absolute_source_path: &str, container_name: &str, wasm: &str) -> Result<std::process::Child, ProcessExitCode> {
    if let Ok(mut copy) = Command::new("docker")
        .arg("cp")        
        .arg(format!("{container_name}:/{wasm}"))
        .arg(format!(r#"{absolute_source_path}"#))
        .spawn() {
        match copy.wait() {
            Ok(_status) => {
                return Ok(copy);
            },
            Err(_)=> {
                return Err(ProcessExitCode::DockerDaemonFailure);
            },
        };
    } else {
        return Err(ProcessExitCode::DockerDaemonFailure);
    }
}

/// `create_directory` sets up the initial file system skeleton starting from the root 
/// directory computed from all supplied paths and its dependencies. 
///
/// ### Arguments
/// * `option` and `shell` are attribute dependent on current OS.  
/// * `absolute_source_path` - absolute path to the source code file
/// * `container_name` - name of the target container 
fn create_directory(option: &str, shell: &str, container_name: &str, absolute_source_path: &str) -> Result<std::process::Child, ProcessExitCode> {
    if let Ok(mut cd) = Command::new(shell)
        .arg(option)
        .arg(format!("docker exec {container_name} mkdir -p {manifest}", manifest=&absolute_source_path.replace(":", "").replace("\\", "/").replace(" ", "_")))
        .spawn() {
        match cd.wait() {
            Ok(_status) => {
                return Ok(cd);
            },
            Err(_)=> {
                return Err(ProcessExitCode::DockerDaemonFailure);
            },
        };
    } else {
        return Err(ProcessExitCode::DockerDaemonFailure);
    }
}

/// get_random_string generates a random string for naming the docker container. 
fn get_random_string() -> String {
    thread_rng()
            .sample_iter(&Alphanumeric)
            .take(5)
            .map(char::from)
            .collect()
}
