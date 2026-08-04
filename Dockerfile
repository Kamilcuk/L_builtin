FROM rust:1-slim-bookworm AS base
RUN set -x && \
      apt-get update && \
      apt-get install -y \
            git make gcc bison yacc libncurses5-dev libreadline-dev \
            cmake clang clang-format clang-tidy cppcheck \
            curl wget ca-certificates \
      && \
      rm -rf /var/lib/apt/lists/*

# Stage: Build all Bash versions (cached layer - rebuild only when bash.git changes)
FROM base AS bash-builder
WORKDIR /bash
RUN git clone --bare https://git.savannah.gnu.org/git/bash.git bash.git
COPY bash.sh .
RUN set -x && \
      mkdir -vp build && \
      mv -v bash.git build/bash.git && \
      ./bash.sh all && \
      cd .. && \
      rm -rf /bash

# Pre-build all versions

# Stage: CI runner - uses pre-built bash versions
FROM base AS ci-runner
COPY --from=bash-builder /opt/bash /opt/bash
WORKDIR /src

# Copy source
COPY . .

# Default bash version (can be overridden at runtime)
ARG BASH=5.2
ENV BASH=${BASH}
ENV BASH_PATH=/opt/bash/${BASH}/bin/bash
ENV PATH=/opt/bash/${BASH}/bin/:$PATH

# Build the project (release mode)
RUN set -x && \
      cp -v ${BASH_PATH} /usr/bin/bash && \
      cp -v ${BASH_PATH} /bin/bash && \
      cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DBASH=${BASH_PATH} -DBASH_INC=/opt/bash/${BASH}/include && \
      cmake --build build -- -j$(nproc)

# Test entrypoint
ENTRYPOINT ["/opt/bash/${BASH}/bin/bash", "-c"]
CMD ["/src/runtests.sh"]
