/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implements the operations with Docker Containers for building contract, for example, starting container,
//! copying files to container and executing commands inside docker.

use std::{
    io::{Read, Write},
    ops::Not,
    path::{Path, PathBuf}, time::Duration,
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

/// List of docker image tags that can be used. The first (0-indexed) is the default one. 
pub(crate) const PCHAIN_COMPILE_IMAGE_TAGS: [&str; 2] = [env!("CARGO_PKG_VERSION"), "mainnet01"];
/// The repo name in Parallelchain Lab Dockerhub: https://hub.docker.com/r/parallelchainlab/pchain_compile
pub(crate) const PCHAIN_COMPILE_IMAGE: &str = "parallelchainlab/pchain_compile";
const DOCKER_EXEC_TIME_LIMIT: u64 = 15; // secs. It is a time limit to normal docker execution (except cargo build).

/// Generate a random Docker container name
pub fn random_container_name() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(5)
        .map(char::from)
        .collect()
}

/// Pull docker image from ParallelChain Lab DockerHub. Returns the name of docker image.
pub async fn pull_image(docker: &Docker, tag: &str) -> Result<String, Error> {
    let from_image = format!("{PCHAIN_COMPILE_IMAGE}:{tag}");
    let create_image_infos = &docker
        .create_image(
            Some(CreateImageOptions {
                from_image: from_image.clone(),
                ..Default::default()
            }),
            None,
            None,
        )
        .try_collect::<Vec<_>>()
        .await
        .map_err(|_| Error::DockerDaemonFailure)?;

    if create_image_infos.is_empty() || create_image_infos.first().unwrap().error.is_some() {
        return Err(Error::DockerDaemonFailure);
    }

    Ok(from_image)
}

/// Starts a containter with the Image pulled from ParallelChain Lab DockerHub
pub async fn start_container(
    docker: &Docker,
    container_name: &str,
    image: String,
) -> Result<(), Error> {
    let _container_create_response = docker
        .create_container(
            Some(CreateContainerOptions {
                name: container_name.to_string(),
                platform: None,
            }),
            Config {
                image: Some(image),
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
        .trim_start_matches('/')
        .to_string(); // Remove the starting "/" for linux file path format.

    let src_path = Path::new(source_path).to_path_buf();
    let dst_path = Path::new(
        format!(
            "{}-{}.tar.gz",
            container_name,
            src_path.file_name().unwrap().to_str().unwrap()
        )
        .as_str(),
    )
    .to_path_buf();

    create_tar_gz(src_path, &save_to_path, &dst_path).map_err(|_| Error::DockerDaemonFailure)?;

    // Read Content
    let file_content = File::open(dst_path.clone())
        .map(|mut file| {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            contents
        })
        .map_err(|_| Error::DockerDaemonFailure)?;

    // Save to docker container
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
    let _ = std::fs::remove_file(&dst_path); // remove the compressed file .tar.gz

    result.map_err(|_| Error::DockerDaemonFailure)
}

/// Copy files from docker container to a specified output path. The output path is None, current path becomes the output path.
pub async fn copy_files_from(
    docker: &Docker,
    container_name: &str,
    container_path: &str,
    specified_output_path: Option<PathBuf>,
    build_log: String,
) -> Result<(), Error> {
    let download_option = DownloadFromContainerOptions {
        path: container_path,
    };

    // Parse compressed file
    let compressed_data = docker
        .download_from_container(container_name, Some(download_option))
        .try_collect::<Vec<_>>()
        .await
        .map_err(|e| Error::BuildFailure(e.to_string()))?
        .concat();
    let files_content = files_from_tar_gz(compressed_data)?;

    if files_content.is_empty() {
        return Err(Error::BuildFailureWithLogs(build_log));
    }

    // Save to destination
    let output_path = specified_output_path.unwrap_or(Path::new(".").to_path_buf());
    for (file_name, content) in &files_content {
        let mut fs = File::create(output_path.join(file_name))
            .map_err(|_| Error::BuildFailure("Fail to access to destination path.".to_string()))?;
        fs.write(content).map_err(|_| {
            Error::BuildFailure(
                "Fail to write the compiled contract to destination path.".to_string(),
            )
        })?;
    }

    Ok(())
}

/// Build contract by executing commands in docker container, including `Cargo`, `wasm-opt` and `wasm-snip`.
/// Return the output folder path and the build logs if success.
pub async fn build_contracts(
    docker: &Docker,
    container_name: &str,
    source_path: PathBuf,
    locked: bool,
    wasm_file: &str,
) -> Result<(String, String), Error> {
    let source_path_str = source_path.to_str().unwrap()
        .replace(':', "")
        .replace('\\', "/")
        .replace(' ', "_");
    let working_folder_code = format!("/{source_path_str}").to_string();
    let working_folder_build =
        format!("/{source_path_str}/target/wasm32-unknown-unknown/release").to_string();
    let output_folder = "/result";
    let output_file = format!("{output_folder}/{wasm_file}").to_string();

    // Does not set "--locked" if the Cargo.lock file does not exist.
    let use_cargo_lock = locked && source_path.join("Cargo.lock").exists();
    let cmd_cargo_build = if use_cargo_lock {
        vec![
            "cargo",
            "build",
            "--locked",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ]
    } else {
        vec![
            "cargo",
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ]
    };

    let build_log = execute(
        docker,
        container_name,
        Some(&working_folder_code),
        cmd_cargo_build,
        true,
        None
    )
    .await
    .map_err(|e| Error::BuildFailure(e.to_string()))?;

    let mut cmds = vec![
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

    // Save Cargo.lock to output folder if applicable
    if locked {
        cmds.push(
            (
                &working_folder_code,
                vec!["mv", "Cargo.lock", output_folder]
            )
        );
    }

    for (working_dir, cmd) in cmds {
        execute(
            docker,
            container_name,
            Some(working_dir),
            cmd,
            false,
            Some(DOCKER_EXEC_TIME_LIMIT)
        )
        .await
        .map_err(|e| Error::BuildFailure(e.to_string()))?;
    }

    Ok((output_folder.to_string(), build_log))
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
    log_output: bool,
    timeout_secs: Option<u64>
) -> Result<String, Error> {
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
        .await
        .map_err(|e| Error::BuildFailure(e.to_string()))?;

    let start_exec_results = docker
        .start_exec(
            &create_exec_results.id,
            Some(StartExecOptions {
                detach: false,
                ..Default::default()
            }),
        )
        .await
        .map_err(|e| Error::BuildFailure(e.to_string()))?;

    match start_exec_results {
        bollard::exec::StartExecResults::Attached { output, .. } => {
            let log_outputs =
            if log_output {
                output.try_collect::<Vec<_>>()
                    .await
                    .map_err(|e| Error::BuildFailure(e.to_string()))?
                    .into_iter()
                    .map(|output| output.to_string() )
                    .collect()
            } else {
                Vec::new()
            }
            .join("");

            // Wait until the execution finishes.
            if let Some(timeout) = timeout_secs {
                let is_inspect_ok = tokio::time::timeout(Duration::from_secs(timeout), async {
                    loop {
                        if let Ok(inspect_result) = docker.inspect_exec(&create_exec_results.id).await {
                            if inspect_result.running != Some(true) {
                                return true
                            }
                            // Continue to check if the execution finishes.
                        } else {
                            // Fail to inspect. The loop should be terminated.
                            return false
                        }
                        // A small delay to avoid hitting docker endpoint immediately.
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .map_err(|_| Error::BuildTimeout)?;
                if !is_inspect_ok {
                    return Err(Error::BuildFailureWithLogs(log_outputs))
                }
            }

            return Ok(log_outputs)
        },
        bollard::exec::StartExecResults::Detached => {
            return Err(Error::BuildFailure("Execution Result Not Attached".to_string()));
        }
    }
}
