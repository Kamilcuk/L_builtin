FROM rust:1-slim-bookworm AS base
RUN set -x && \
    apt-get update && \
    apt-get install -y \
        git make gcc bison yacc libncurses5-dev libreadline-dev \
        cmake clang clang-format clang-tidy cppcheck \
        curl wget ca-certificates \
        autoconf \
    && \
    rm -rf /var/lib/apt/lists/* && \
    cargo install bindgen-cli --locked

FROM base AS bash-builder
WORKDIR /src
RUN set -x && \
    mkdir -vp build && \
    git clone --branch master --single-branch --bare https://git.savannah.gnu.org/git/bash.git build/bash.git
COPY --parents Makefile.bash scripts/resolve-bash-version.sh .
ARG BASHES="5.3"
ENV BASHES=${BASHES}
RUN set -eux && \
    for BASH in $BASHES; do \
        make -f Makefile.bash bash-dockerfile BASH=$BASH && \
        ./build/bash/$BASH/bash --version || exit; \
    done

FROM bash-builder AS build
COPY --parents Cargo.toml Cargo.lock Makefile CMakeLists.txt src scripts third_party .
ARG MAKEARGS=
ENV MAKEARGS=${MAKEARGS}
RUN --mount=type=cache,target=/src/build/Debug \
    --mount=type=cache,target=/src/build/Release \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux && \
    for BASH in $BASHES; do \
        make dockerfile BASH=$BASH DEST=/dest || exit; \
    done && \
    ls -la /dest

FROM build AS test
COPY --parents tests runtests.sh .
ARG ARGS=
ENV ARGS=${ARGS}
RUN set -eux && \
    for BASH in $BASHES; do \
        timeout -v -k 2 20 ./build/bash/$BASH/bash ./runtests.sh /dest/L_builtin-*-bash-$BASH.so ${ARGS} || exit; \
    done

FROM scratch AS output
COPY --from=test /dest /
