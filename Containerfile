ARG RUST_IMAGE=rust:1.89-bookworm@sha256:948f9b08a66e7fe01b03a98ef1c7568292e07ec2e4fe90d88c07bb14563c84ff

FROM ${RUST_IMAGE}

ENV CARGO_TERM_COLOR=always \
    CARGO_BUILD_JOBS=4 \
    DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        make \
    && rustup component add rustfmt clippy rust-analyzer llvm-tools-preview \
    && cargo install cargo-llvm-cov --version 0.8.7 --locked \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY . .

CMD ["make", "verify"]
