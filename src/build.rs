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

use bollard::Docker;
use std::{collections::HashSet, path::PathBuf};

use std::fs;

use crate::error::Error;

/// `build_target` takes the path to the cargo manifest file(s), generates an optimized WASM binary(ies) after building
/// the source code and saves the binary(ies) to the designated destination_path.
pub async fn build_target(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
) -> Result<String, Error> {
    // create destination directory if it does not exist.
    if let Some(dst_path) = &destination_path {
        fs::create_dir_all(dst_path).map_err(|_| Error::InvalidDestinationPath)?;
    }

    // check if the manifest file exists on the path supplied.
    let package_name =
        crate::manifests::package_name(&source_path).map_err(|_| Error::InvalidSourcePath)?;
    let wasm_file = format!("{package_name}.wasm").replace('-', "_");

    // retrieve dependency paths from manifest.
    let mut dependencies = HashSet::new();
    crate::manifests::get_dependency_paths(&source_path, &mut dependencies)?;

    let container_name = crate::docker::random_container_name();

    let docker = Docker::connect_with_local_defaults().map_err(|_| Error::DockerDaemonFailure)?;
    crate::docker::pull_image(&docker).await?;
    crate::docker::start_container(&docker, &container_name).await?;

    // Build Contract in docker container
    let result = build_in_docker(
        &docker,
        &container_name,
        dependencies,
        source_path,
        destination_path,
        &wasm_file,
    )
    .await;

    // Remove container no matter if build is successful
    let _ = crate::docker::remove_container(&docker, &container_name).await;

    result.map(|_| wasm_file)
}

async fn build_in_docker(
    docker: &Docker,
    container_name: &str,
    dependencies: impl IntoIterator<Item = String>,
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    wasm_file: &str,
) -> Result<(), Error> {
    // Step 1. create dependency directory and copy source to docker
    for dependency in dependencies {
        crate::docker::copy_files(docker, container_name, &dependency).await?;
    }

    // Step 2: create directory paths inside docker and  copy file to container
    crate::docker::copy_files(docker, container_name, source_path.to_str().unwrap()).await?;

    // Step 3: build the source code inside docker
    let result_in_docker = crate::docker::build_contracts(
        docker,
        container_name,
        source_path.to_str().unwrap(),
        wasm_file,
    )
    .await?;

    // Step 4: copy file from docker to given location
    crate::docker::copy_files_from(
        docker,
        container_name,
        &result_in_docker,
        destination_path.clone(),
    )
    .await?;

    Ok(())
}
