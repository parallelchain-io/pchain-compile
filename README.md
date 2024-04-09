# ParallelChain Mainnet Contract Compiler (pchain_compile) 

`pchain_compile` is a command line tool for reproducibly building [Rust](https://www.rust-lang.org/) code into compact, gas-efficient WebAssembly [ParallelChain Mainnet Smart Contracts](https://github.com/parallelchain-io/parallelchain-protocol/blob/master/Contracts.md).

## Pre-Requisites

`pchain_compile` compiles the ParallelChain smart contract code in Rust into contract binary in WebAssembly. To know more about developing ParallelChain smart contracts, visit [ParallelChain Mainnet Contract SDK](https://crates.io/crates/pchain-sdk).

By default, the compiler requires **docker** to be installed in your local machine. In detail, it pulls the [Docker Image](#about-docker-image) from DockerHub, and starts a docker container which provides a complete environment for building WebAssembly binary. To install docker, follow the instructions in [Docker Docs](https://docs.docker.com/get-docker/).

## Installation


Download prebuilt executables from the Github [Releases page](https://github.com/parallelchain-io/pchain-compile/releases). 

Alternatively, you can install the binary crate [pchain_compile](https://crates.io/crates/pchain_compile) by [cargo install](https://doc.rust-lang.org/cargo/commands/cargo-install.html) if Rust has been installed already.

```sh
cargo install pchain_compile
```

## Build Smart Contract

Let say your smart contract source code is in the folder `contract` under your home directory. 

```text
/home/
|- user/
   |- contract/
      |- src/
         |- lib.rs
      |- Cargo.toml
```

Run `pchain_compile` with the arguments **source** and **destination** to specify the folder of the source code and the folder for saving the result.

```sh
pchain_compile build --source /home/user/contract --destination /home/user/result
```

Once complete, the console displays message:

```text
Build process started. This could take several minutes for large contracts.

Finished compiling. ParallelChain Mainnet smart contract(s) ["contract.wasm"] are saved at (/home/user/result).
```

Your WebAssembly smart contract is now saved with file extension `.wasm` at the destination folder. 

If you are running on Windows, here is the example output:
```powershell
$ .\pchain_compile.exe build --source 'C:\Users\user\contract' --destination 'C:\Users\user\result'
Build process started. This could take several minutes for large contracts.

Finished compiling. ParallelChain Mainnet smart contract(s) ["contract.wasm"] are saved at (C:\Users\user\result).
```

To understand more about the commands and arguments, run `pchain_compile build --help`.

## About Docker Image

`pchain_compile` pulls docker image from ParallelChain Lab's official DockerHub [repository](https://hub.docker.com/r/parallelchainlab/pchain_compile) for the build process. The docker image provides an environment with installed components:
- rustc: compiler for Rust.
- wasm-snip: WASM utility which removes functions that are never called at runtime.
- wasm-opt: WASM utility to load WebAssembly in text format and run Binaryen IR passes to optimize its size. For more information on Binaryen IR see [here](http://webassembly.github.io/binaryen/).

There are different tags of the docker image. They vary on the versions of the components. The table below describes the tags and their differences.

|Image Tag |rustc |wasm-snip |wasm-opt |
|:---|:---|:---|:---|
|0.4.3 | 1.77.1 | 0.4.0| 114|
|0.4.2 | 1.71.0 | 0.4.0 | 114 |
|mainnet01 | 1.66.1 | 0.4.0 | 109 |

To build smart contract in a specific docker environment, run with argument **use-docker-tag**. For example,

```sh
pchain_compile build --source /home/user/contract --destination /home/user/result --use-docker-tag 0.4.3
```

If **use-docker-tag** is not used, the tag being used is with the same version as `pchain_compile`.