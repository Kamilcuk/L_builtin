# Dockerfile

ARG BASE_IMAGE=ubuntu

###############################################################################
# base - all packages installed that we need

FROM docker.io/library/ubuntu:22.04 AS ubuntu-base
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/bin:$PATH
RUN set -x && \
    apt-get update && \
    apt-get install -y --no-install-recommends \
    git make gcc bison libncurses5-dev libreadline-dev \
    clang clang-format clang-tidy cppcheck \
    curl wget ca-certificates autoconf gnupg ninja-build && \
    rm -rf /var/lib/apt/lists/*
RUN set -x && \
    CMAKE_VERSION="3.28.3" && \
    wget -q https://github.com/Kitware/CMake/releases/download/v${CMAKE_VERSION}/cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz && \
    tar --no-same-owner -xzf cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz -C /usr/local --strip-components=1 && \
    rm cmake-${CMAKE_VERSION}-linux-x86_64.tar.gz && \
    cmake --version && \
    \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain 1.98.0 && \
    cargo install bindgen-cli --locked && \
    cargo --version && \
    rustc --version

FROM docker.io/library/alpine:latest AS alpine-base
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN set -x && \
    apk add --no-cache \
        bash git make gcc musl-dev bison ncurses-dev readline-dev \
        cmake clang clang-extra-tools cppcheck \
        curl wget ca-certificates autoconf ninja && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
        sh -s -- -y --no-modify-path --default-toolchain 1.75.0 && \
    cargo install bindgen-cli --locked

FROM ${BASE_IMAGE}-base as base
RUN set -x && \
    groupadd -g 1000 build && \
    useradd -m -u 1000 -g build -s /bin/bash build
WORKDIR /a/a
# Make cargo home and cache directories writable by the `build` user so that
# the `type=cache` mounts at build-time can be populated. Without this, cargo
# (running as uid 1000) fails with "Permission denied" when trying to create
# /usr/local/cargo/{registry,git}/db/* on a cold cache.
RUN mkdir -p /a && \
    mkdir -p /home/build/.cargo && \
    chown -R build:build /a /home/build /usr/local/cargo /usr/local/rustup
USER build:build

###############################################################################
# bash-build - bash is compiled

FROM base AS bash-build
RUN set -x && \
    mkdir -vp build && \
    git clone --branch master --single-branch --bare \
        https://git.savannah.gnu.org/git/bash.git build/bash.git
COPY --parents Makefile.bash scripts/resolve-bash-version.sh .
ARG BASHES="5.3 5.2 5.1 5.0 4.4"
ENV BASHES=${BASHES}
RUN make -f Makefile.bash bash-dockerfile

###############################################################################
# build - our code is build

FROM bash-build AS build
COPY --parents \
        CMakeLists.txt \
        Cargo.lock \
        Cargo.toml \
        README.md \
        cmdargs-derive/ \
        l_builtin/ \
        scripts/ \
        third_party/ \
        llib/ \
        dispatcher/ \
        cmake/ \
        .
COPY Makefile .
ARG MAKEARGS=
ENV MAKEARGS=${MAKEARGS}
RUN --mount=type=cache,uid=1000,gid=1000,target=/a/a/build/Debug \
    --mount=type=cache,uid=1000,gid=1000,target=/a/a/build/Release \
    --mount=type=cache,uid=1000,gid=1000,target=/a/a/build/rust \
    --mount=type=cache,uid=1000,gid=1000,target=/usr/local/cargo/registry \
    --mount=type=cache,uid=1000,gid=1000,target=/usr/local/cargo/git \
    --mount=type=cache,uid=1000,gid=1000,target=/home/build/.cargo/registry \
    --mount=type=cache,uid=1000,gid=1000,target=/home/build/.cargo/git \
    make ${MAKEARGS} dockerfile-build

FROM scratch AS build-output
COPY --from=build /a/a/build/dest /

###############################################################################
# test

FROM build AS test
COPY --parents tests runtests.sh .
ARG ARGS=
ENV ARGS=${ARGS}
RUN make ${ARGS} dockerfile-test

FROM scratch AS output
COPY --from=test /a/a/build/dest /

###############################################################################
# others

FROM docker.io/library/alpine:latest AS alpine
RUN apk add --no-cache bash
