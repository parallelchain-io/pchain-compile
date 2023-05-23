FROM rust:1.66.0-slim as cache

# Download binaryen sources
ADD https://github.com/WebAssembly/binaryen/archive/refs/tags/version_110.tar.gz /tmp/binaryen.tar.gz

# Extract and compile wasm-opt & install wasm-snip
RUN apt update && apt-get -y install wget && \
    wget https://github.com/WebAssembly/binaryen/releases/download/version_109/binaryen-version_109-x86_64-linux.tar.gz && \
    tar xzf binaryen-version_109-x86_64-linux.tar.gz && mv binaryen-version_109/bin/wasm-opt /usr/local/bin && \
    rm -rf binaryen-version_109*

# pchain-compile base image
FROM rust:1.66.0-slim as base-image

# Setup rust with wasm support
RUN cargo install wasm-snip && rustup target add wasm32-unknown-unknown && mkdir -p /root/bin

# Add wasm-opt
COPY --from=cache /usr/local/bin/wasm-opt /root/bin
RUN chmod +x /root/bin/wasm-opt

