/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implementation of contracts compilation.
//!
//! ## Compilation with use of Docker
//!
//! The flow of the compilation process is as follows:
//! 1. Setup destination folders and parse the command arguments into in-memory data that is to be used in subsequent steps.
//!    Also pull the docker image from docker hub.
//! 2. Create file structures in the docker container, and then copy the source code to it. It also applies to the libraries
//!    that are using relative paths in dependencies.
//! 3. Compile the source code in the docker container. The dependencies (if any) are compile first.
//! 4. After compilation, copy the binary (wasm) from docker container to target destination.
//!
//! ## Compilation without using Docker
//!
//! This way to compile smart contract requires the caller to install Rust and add target `wasm32-unknown-unknown` beforehand.
//! The actual steps are as same as those commands executing inside the docker container. In simple words, build by `Cargo build`,
//! then optimize and snip by `wasm-opt` and `wasm-snip`.
//!
//! **Please note the compiled contracts are not always consistent with the previous compiled ones, because the building process happens in
//! your local changing environment.**

use bollard::Docker;
use std::path::Path;
use std::{collections::HashSet, path::PathBuf};

use std::fs;

use crate::error::Error;
use crate::{DockerConfig, BuildOptions};

/// `build_target` takes the path to the cargo manifest file(s), generates an optimized WASM binary(ies) after building
/// the source code and saves the binary(ies) to the designated destination_path.
/// 
/// This method is equivalent to run the command:
/// 
/// `pchain_compile` build --source `source_path` --destination `destination_path`
pub async fn build_target(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
) -> Result<String, Error> {
    build_target_with_docker(source_path, destination_path, BuildOptions::default(), DockerConfig::default()).await
}

/// Validates inputs and trigger building process that uses docker.
pub(crate) async fn build_target_with_docker(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    options: BuildOptions,
    docker_config: DockerConfig,
) -> Result<String, Error> {
    // create destination directory if it does not exist.
    if let Some(dst_path) = &destination_path {
        fs::create_dir_all(dst_path).map_err(|_| Error::InvalidDestinationPath)?;
    }

    // check validity of source path (and convert relative path to absolute path if applicable)
    let source_path = validated_source_path(source_path)?;

    // check if the manifest file exists on the path supplied.
    let package_name =
        crate::manifests::package_name(&source_path).map_err(|_| Error::InvalidSourcePath)?;
    let wasm_file = format!("{package_name}.wasm").replace('-', "_");

    // check if docker image tag is valid
    let docker_image_tag = docker_config
        .tag
        .unwrap_or(crate::docker::PCHAIN_COMPILE_IMAGE_TAGS[0].to_string());
    if !crate::docker::PCHAIN_COMPILE_IMAGE_TAGS.contains(&docker_image_tag.as_str()) {
        return Err(Error::UnkownDockerImageTag(docker_image_tag));
    }

    build_target_in_docker(source_path, destination_path, options, docker_image_tag, wasm_file).await
}

/// Validates inputs and trigger building process that does not use docker.
pub(crate) async fn build_target_without_docker(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    options: BuildOptions,
) -> Result<String, Error> {
    // create destination directory if it does not exist.
    if let Some(dst_path) = &destination_path {
        fs::create_dir_all(dst_path).map_err(|_| Error::InvalidDestinationPath)?;
    }

    // check validity of source path (and convert relative path to absolute path if applicable)
    let source_path = validated_source_path(source_path)?;

    // check if the manifest file exists on the path supplied.
    let package_name =
        crate::manifests::package_name(&source_path).map_err(|_| Error::InvalidSourcePath)?;
    let wasm_file = format!("{package_name}.wasm").replace('-', "_");

    build_target_by_cargo(source_path, destination_path, options, wasm_file).await
}

fn validated_source_path(source_path: PathBuf) -> Result<PathBuf, Error> {
    let src_str = source_path.to_str().ok_or(Error::InvalidSourcePath)?;
    let src_absolute_str =
        crate::manifests::get_absolute_path(src_str).map_err(|_| Error::InvalidSourcePath)?;
    Ok(Path::new(&src_absolute_str).to_path_buf())
}

/// Setup docker environment and build contract in docker container. It manages to pull docker image, start and remove containers.
async fn build_target_in_docker(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    options: BuildOptions,
    docker_image_tag: String,
    wasm_file: String,
) -> Result<String, Error> {
    // Retrieve dependency paths from manifest.
    let mut dependencies = HashSet::new();
    crate::manifests::get_dependency_paths(&source_path, &mut dependencies)?;

    // Create container from Parallelchain Lab docker image
    let container_name = crate::docker::random_container_name();
    let docker = Docker::connect_with_local_defaults().map_err(|_| Error::DockerDaemonFailure)?;
    let image_name = crate::docker::pull_image(&docker, &docker_image_tag).await?;
    crate::docker::start_container(&docker, &container_name, image_name).await?;

    // Compile Contract in docker container
    let result = compile_contract_in_docker_container(
        &docker,
        &container_name,
        dependencies,
        source_path,
        destination_path,
        options,
        &wasm_file,
    )
    .await;

    // Remove container no matter if build is successful
    let _ = crate::docker::remove_container(&docker, &container_name).await;

    result.map(|_| wasm_file)
}

/// Inner process in method [build_target_in_docker] to compile contract in docker container. It does not remove docker container after use.
async fn compile_contract_in_docker_container(
    docker: &Docker,
    container_name: &str,
    dependencies: impl IntoIterator<Item = String>,
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    options: BuildOptions,
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
        source_path,
        options.locked,
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

/// Setup filesystem and build contract by cargo. It manages to create a temporary workding folder and 
/// remove it after call.
async fn build_target_by_cargo(
    source_path: PathBuf,
    destination_path: Option<PathBuf>,
    options: BuildOptions,
    wasm_file: String,
) -> Result<String, Error> {
    // 1. Create temporary folder as a working directory for cargo build
    let temp_dir = crate::cargo::random_temp_dir_name();
    std::fs::create_dir_all(temp_dir.as_path()).map_err(|_| Error::CreateTempDir)?;

    // 2. Build the source code locally by cargo build
    let result = crate::cargo::build_contract(
        &temp_dir,
        source_path.as_path(),
        destination_path,
        options.locked,
        &wasm_file,
    );

    // 3. Remove temporary files after building
    let _ = std::fs::remove_dir_all(temp_dir);

    result.map(|_| wasm_file)
}