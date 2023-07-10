/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implements the operations with Docker Containers for building contract, for example, starting container,
//! copying files to container and executing commands inside docker.

use std::{
    io::{Read, Write},
    ops::Not,
    path::{Path, PathBuf}
};

use bollard::{
    container::{
        Config, CreateContainerOptions, DownloadFromContainerOptions, RemoveContainerOptions,
        StartContainerOptions, UploadToContainerOptions,
    },
    exec::{CreateExecOptions, StartExecOptions},
    image::CreateImageOptions,
    service::HostConfig,
    Docker,
};
use futures_util::TryStreamExt;
use tar::Archive;

use flate2::write::GzEncoder;
use flate2::Compression;
use std::fs::File;

use crate::error::Error;
use rand::{distributions::Alphanumeric, thread_rng, Rng};

const PCHAIN_COMPILE_IMAGE: &str = "parallelchainlab/pchain_compile:mainnet01";

/// get a random string for naming the docker container.
pub fn random_container_name() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect()
}

/// Pull docker image from ParallelChain Lab DockerHub
pub async fn pull_image(docker: &Docker) -> Result<(), Error> {
    let create_image_infos = &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: PCHAIN_COMPILE_IMAGE,
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await
        .unwrap();

    if create_image_infos.is_empty() || create_image_infos.first().unwrap().error.is_some() {
        return Err(Error::DockerDaemonFailure);
    }

    Ok(())
}

/// Start containter with the Image pulled from ParallelChain Lab DockerHub
pub async fn start_container(docker: &Docker, container_name: &str) -> Result<(), Error> {
    let _container_create_response = docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.to_string(),
                platform: None,
            }),
            Config {
                image: Some(PCHAIN_COMPILE_IMAGE),
                open_stdin: Some(true),
                tty: Some(true),
                host_config: Some(HostConfig {
                    privileged: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .map_err(|_| Error::DockerDaemonFailure)?;

    docker
        .start_container(
            container_name,
            Some(StartContainerOptions::<String>::default()),
        )
        .await
        .map_err(|_| Error::DockerDaemonFailure)?;
    Ok(())
}

/// Copy Files from source path to docker container
pub async fn copy_files(
    docker: &Docker,
    container_name: &str,
    source_path: &str,
) -> Result<(), Error> {
    let save_to_path = source_path
        .replace(':', "")
        .replace('\\', "/")
        .replace(' ', "_")
        .trim_start_matches("/").to_string(); // Remove the starting "/" for linux file path format.

    let src_path = Path::new(source_path).to_path_buf();
    let dst_path =
        Path::new(format!("{}.tar.gz", src_path.file_name().unwrap().to_str().unwrap()).as_str())
            .to_path_buf();

    create_tar_gz(src_path, &save_to_path, &dst_path).unwrap();

    // Read Content
    let file_content = File::open(dst_path.clone())
        .map(|mut file| {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            contents
        })
        .map_err(|_| Error::DockerDaemonFailure)?;

    let result = docker
        .upload_to_container(
            container_name,
            Some(UploadToContainerOptions {
                path: "/",
                ..Default::default()
            }),
            file_content.into(),
        )
        .await;

    // Remove file
    std::fs::remove_file(&dst_path).unwrap();

    result.map_err(|_| Error::DockerDaemonFailure)
}

/// Copy files from docker container to a specified output path. The output path is None, current path becomes the output path.
pub async fn copy_files_from(
    docker: &Docker,
    container_name: &str,
    container_path: &str,
    specified_output_path: Option<PathBuf>,
) -> Result<(), Error> {
    let download_option = DownloadFromContainerOptions {
        path: container_path
    };

    // Parse Compress File
    let compressed_data = docker
        .download_from_container(container_name, Some(download_option))
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| Error::BuildFailure(e.to_string()))?
        .concat();
    let files_content = files_from_tar_gz(compressed_data)?;

    // Save to Distination
    let output_path = specified_output_path
        .unwrap_or(Path::new(".").to_path_buf());
    files_content.iter().for_each(|(file_name, content)| {
        let mut fs = File::create(output_path.join(file_name)).unwrap();
        let _ = fs.write(content);
    });

    Ok(())
}

/// Build contract by executing commands in docker container, including `Cargo`, `wasm-opt` and `wasm-snip`.
pub async fn build_contracts(
    docker: &Docker,
    container_name: &str,
    source_path: &str,
    wasm_file: &str,
) -> Result<String, Error> {
    let source_path = source_path
        .replace(':', "")
        .replace('\\', "/")
        .replace(' ', "_");
    let working_folder_code = format!("/{source_path}").to_string();
    let working_folder_build =
        format!("/{source_path}/target/wasm32-unknown-unknown/release").to_string();
    let output_folder = "/result";
    let output_file = format!("{output_folder}/{wasm_file}").to_string();

    let cmds = vec![
        (
            &working_folder_code,
            vec![
                "cargo",
                "build",
                "--target",
                "wasm32-unknown-unknown",
                "--release",
                "--quiet",
            ],
        ),
        (
            &working_folder_build,
            vec!["chmod", "+x", "/root/bin/wasm-opt"],
        ),
        (
            &working_folder_build,
            vec![
                "/root/bin/wasm-opt",
                "-Oz",
                wasm_file,
                "--output",
                "temp.wasm",
            ],
        ),
        (
            &working_folder_build,
            vec![
                "wasm-snip",
                "temp.wasm",
                "--output",
                "temp2.wasm",
                "--snip-rust-fmt-code",
                "--snip-rust-panicking-code",
            ],
        ),
        (
            &working_folder_build,
            vec![
                "/root/bin/wasm-opt",
                "--dce",
                "temp2.wasm",
                "--output",
                "optimized.wasm",
            ],
        ),
        (&working_folder_build, vec!["mkdir", "-p", output_folder]),
        (
            &working_folder_build,
            vec!["mv", "optimized.wasm", &output_file],
        ),
    ];

    for (working_dir, cmd) in cmds {
        execute(docker, container_name, Some(working_dir), cmd)
            .await
            .map_err(|e| Error::BuildFailure(e.to_string()))?;
    }

    Ok(output_folder.to_string())
}

/// Force stop and remove a container
pub async fn remove_container(docker: &Docker, container_name: &str) -> Result<(), Error> {
    let remove_option = RemoveContainerOptions {
        v: true,
        link: false,
        force: true,
    };
    docker
        .remove_container(container_name, Some(remove_option))
        .await
        .map_err(|_| Error::ArtifactRemovalFailure)?;
    Ok(())
}

fn create_tar_gz(
    src_path: PathBuf,
    tar_path: &str,
    dst_path: &PathBuf,
) -> Result<(), std::io::Error> {
    let tar_gz = File::create(dst_path)?;
    let enc = GzEncoder::new(tar_gz, Compression::default());
    let mut tar = tar::Builder::new(enc);
    tar.append_dir_all(tar_path, src_path)?;
    tar.finish()
}

fn files_from_tar_gz(tar_gz_bytes: Vec<u8>) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let mut archive = Archive::new(&tar_gz_bytes[..]);

    let result = archive
        .entries()
        .map_err(|e| Error::BuildFailure(e.to_string()))?
        .filter_map(|e| e.ok())
        .filter(|entry| entry.size() > 0)
        .filter_map(|mut entry| {
            let file_name = entry
                .path()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            let mut content = vec![];
            entry.read_to_end(&mut content).unwrap();
            content.is_empty().not().then_some((file_name, content))
        })
        .collect::<Vec<(String, Vec<u8>)>>();
    Ok(result)
}

async fn execute(
    docker: &Docker,
    container_name: &str,
    working_dir: Option<&str>,
    cmd: Vec<&str>,
) -> Result<(), bollard::errors::Error> {
    let create_exec_results = docker
        .create_exec(
            container_name,
            CreateExecOptions {
                working_dir,
                attach_stderr: Some(true),
                attach_stdout: Some(true),
                cmd: Some(cmd),
                ..Default::default()
            },
        )
        .await?;

    let start_exec_results = docker
        .start_exec(
            &create_exec_results.id,
            Some(StartExecOptions {
                detach: false,
                ..Default::default()
            }),
        )
        .await?;

    if let bollard::exec::StartExecResults::Attached { output, .. } = start_exec_results {
        let _output = output.try_collect::<Vec<_>>().await?;
    } else {
        return Err(bollard::errors::Error::DockerStreamError {
            error: "Execution Result Not Attached".to_string(),
        });
    }

    Ok(())
}