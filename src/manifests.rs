/*
    Copyright © 2023, ParallelChain Lab
    Licensed under the Apache License, Version 2.0: http://www.apache.org/licenses/LICENSE-2.0
*/

//! Implements methods to obtain manifests of the contract and its dependencies.

use std::{collections::HashSet, path::Path};

use cargo_toml::{DependencyDetail, Manifest};
use faccess::{AccessMode, PathExt};

use crate::error::Error;

/// Returns paths for dependencies from local manifests.
pub fn get_dependency_paths(
    source_path: &Path,
    dependencies: &mut HashSet<String>,
) -> Result<(), Error> {
    let source_manifest =
        Manifest::from_path(source_path.join("Cargo.toml")).map_err(|_| Error::ManifestFailure)?;

    for dependency in source_manifest.dependencies.values() {
        if let Some(DependencyDetail {
            path: Some(current_path),
            ..
        }) = dependency.detail()
        {
            let derived_path = get_absolute_path(current_path).unwrap_or(get_absolute_path(
                source_path.join(current_path).as_os_str().to_str().unwrap(),
            )?);

            if !dependencies.contains(&derived_path) {
                dependencies.insert(derived_path.clone());
                // SAFETY: recursive call can be very deep
                let _ = get_dependency_paths(Path::new(&derived_path), dependencies);
            }
        }
    }

    Ok(())
}

/// Return package name from manifest file
pub fn package_name(current_dir: &Path) -> Result<String, Error> {
    Manifest::from_path(current_dir.join("Cargo.toml"))
        .map(|f| (f.package.as_ref().unwrap()).name.to_string())
        .map_err(|_| Error::ManifestFailure)
}

/// Returns absolute path of a directory.
pub fn get_absolute_path(dir: &str) -> Result<String, Error> {
    // get canonicalized path of the directory.
    let canonicalized_path =
        dunce::canonicalize(dir).map_err(|_| Error::InvalidDependencyPath)?;

    // also check if pchain-compile has write privileges to the canonicalized path.
    // if check passes, get absolute path of the directory.
    canonicalized_path
        .access(AccessMode::WRITE)
        .map(|_| String::from(canonicalized_path.to_string_lossy()))
        .map_err(|_| Error::InvalidDependencyPath)
}
