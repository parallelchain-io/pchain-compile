pub mod operations;

use clap::Parser;
use operations::{build, ProcessExitCode};
use std::env;

// ParallelChain F Smart Contract Build CLI.
// The `main` is responsible to parse user request and build the supplied contract code. 
#[derive(Debug, Parser)]
#[clap(name = "pchain_compile")]
#[clap(version = "1.1.0", about = "ParallelChain F Smart Contract Compile CLI", author = "<ParallelChain Lab>", long_about = None)]
enum PchainCompile {
    /// build the source code.
    #[clap(arg_required_else_help = false, display_order=1)]
    Build {
        /// Path to the source code directory 
        #[clap(long="source", display_order=1)]
        manifest_path : String,
        /// Path to the compiled optimized wasm file 
        #[clap(long="destination", display_order=2)]
        destination_path : Option<String>,
    },
}

// This maps the argument collection to the corresponding handling build function
#[tokio::main]
async fn main() {
    let args = PchainCompile::parse();
    match args {
        PchainCompile::Build { mut manifest_path, destination_path} => {
            // check the current path of the pchain_compile binary
            let working_directory = env::current_dir().unwrap();
            let path: String = String::from(working_directory.to_string_lossy());
            
            if manifest_path.to_lowercase() == "." { manifest_path = path.clone(); };
            
            let destination = match destination_path {
                Some(d) => {
                    match d.to_lowercase().as_ref() {
                        "." => {path.clone()},
                        _ => {d},
                    }
                },
                None => {path.clone()},
            };

            let result  = match build(manifest_path, destination).await {
                Ok(m) => {m},
                Err(e) => {
                    let exit_result = match e { 
                        ProcessExitCode::ArtifactRemovalFailure => {"The compilation was successful, but pchain_compile failed to stop its Docker containers. Please remove them manually."}, 
                        ProcessExitCode::BuildFailure => {"Failed to compile.\nDetails: Check your source code or command line arguments for errors."}, 
                        ProcessExitCode::DockerDaemonFailure => {"Failed to compile.\nDetails: Docker Daemon Failure. Check if Docker is running on your machine and confirm read/write access privileges."},
                        ProcessExitCode::ManifestFailure => {"Failed to compile.\nDetails: Manifest File Not Found. Check if you have provided the correct path to your source code and confirm read access privileges."},
                        ProcessExitCode::InvalidPath => {"Failed to compile.\nDetails: Destination Path Not Valid. Check if you have provided the correct path to save your optimized WASM binary and confirm write access privileges."},
                        ProcessExitCode::Unknown => {"Failed to compile.\nDetails: Unknown error. If this persists after a system restart, please lodge an issue on pchain_compile's GitHub."},
                    };
                    exit_result.to_string()
                },
            };
            println!("{}", result.to_string());
        }
    };
}

