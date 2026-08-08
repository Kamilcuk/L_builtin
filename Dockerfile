FROM rust:1-slim-bookworm AS base
RUN set -x && \
      apt-get update && \
      apt-get install -y \
            git make gcc bison yacc libncurses5-dev libreadline-dev \
            cmake clang clang-format clang-tidy cppcheck \
            curl wget ca-certificates \
            autoconf \
      && \
      rm -rf /var/lib/apt/lists/*

FROM base AS bash-builder
WORKDIR /src
RUN set -x && \
      mkdir -vp build && \
      git clone --branch master --single-branch --bare https://git.savannah.gnu.org/git/bash.git build/bash.git
COPY --parents Makefile.bash scripts/resolve-bash-version.sh .
ARG BASHES="5.0 5.3"
ENV BASHES=${BASHES}
RUN set -x && for BASH in $BASHES; do make -f Makefile.bash bash-dockerfile BASH=$BASH; done

FROM bash-builder AS build
COPY --parents Makefile Makefile.bash CMakeLists.txt src scripts .
ARG BASH=5.3
ENV BASH=${BASH}
ENV PATH=/src/build/${BASH}/prefixbash/bin:$PATH
RUN set -x && make build
ENTRYPOINT ["bash", "-c"]

FROM build AS test
COPY tests runtests.sh .
RUN ./runtests.sh
