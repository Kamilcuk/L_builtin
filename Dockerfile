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
RUN set -eux && for BASH in $BASHES; do make -f Makefile.bash bash-dockerfile BASH=$BASH; done
COPY --parents Cargo.toml Cargo.lock .
RUN mkdir -p src && echo 'fn main() {}' > src/lib.rs && cargo build --lib && rm -rf src target

FROM bash-builder AS build
COPY --parents Makefile CMakeLists.txt src scripts third_party .
ARG MAKEARGS=
ENV MAKEARGS=${MAKEARGS}
ENV PATH=/src/build/${BASH}/prefixbash/bin:$PATH
RUN set -eux && for BASH in $BASHES; do make dockerfile BASH=$BASH DEST=/output/$BASH/; done
RUN ls /output

FROM build AS test
COPY --parents tests runtests.sh .
RUN set -eux && for BASH in $BASHES; do make test BASH=$BASH; done

FROM scratch AS output
COPY --from=test /output /
