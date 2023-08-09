# ParallelChain Mainnet Contract Compiler (pchain_compile) 

`pchain_compile` is a command line tool for reproducibly building Rust code into compact, gas-efficient WebAssembly [ParallelChain Mainnet Smart Contracts](https://github.com/parallelchain-io/parallelchain-protocol/blob/master/Contracts.md).

## Pre-Requisites

`pchain_compile` builds the source code in a docker environment. To know more about Docker and install it, refer to the [official instructions](https://docs.docker.com/get-docker/).

## Installation

Prebuilt binaries can be downloaded from assets in Github [releases page](https://github.com/parallelchain-io/pchain-compile/releases). Alternatively, you can install by `cargo install` if Rust has been installed already.

```sh
cargo install pchain_compile
```

## Build the Source Code

Suppose you have the source code of smart contract in the folder `contract` under your home directory. 

```text
/home/
|- user/
   |- contract/
      |- src/
         |- lib.rs
      |- Cargo.toml
```

To build smart contract into WebAssembly bytecode (file extension `.wasm`), you can simply run the program by specifying the arguments **source** and **destination**.

On a Linux Bash Shell:
      
```sh
$ ./pchain_compile build --source /home/user/contract --destination /home/user/result
Build process started. This could take several minutes for large contracts.

Finished compiling. ParallelChain Mainnet smart contract(s) ["contract.wasm"] are saved at (/home/user/result).
```

On a Windows Shell:
```powershell
$ .\pchain_compile.exe build --source 'C:\Users\user\contract' --destination 'C:\Users\user\result'
Build process started. This could take several minutes for large contracts.

Finished compiling. ParallelChain Mainnet smart contract(s) ["contract.wasm"] are saved at (C:\Users\user\result).
```

Explanation about the command and its arguments can be displayed by appending "help" or "--help" to `pchain_compile`.

## Toolchain

`pchain_compile` utilizes a docker_image hosted on a public DockerHub repository of ParallelChain see [here](https://hub.docker.com/r/parallelchainlab/pchain_compile) for the build process. Required components include:
- rustc: compiler for Rust.
- wasm-snip: WASM utility which removes functions that are never called at runtime.
- wasm-opt: WASM utility to load WebAssembly in text format and run Binaryen IR passes to optimize its size. For more information on Binaryen IR see [here](http://webassembly.github.io/binaryen/).

The docker images utilize a toolchain whose versions of each component are shown in the following table:

|Image Tag |rustc |wasm-snip |wasm-opt |
|:---|:---|:---|:---|
|0.4.2 |1.71.0 |0.4.0 |114 |
|mainnet01 |1.66.1 |0.4.0 |109 |
