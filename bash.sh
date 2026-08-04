#!/usr/bin/env bash
set -euo pipefail

# Find script directory and source L_lib
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$DIR"/runtests.sh

L_argparse \
  prog="bash-manager" \
  description="Manage multi-version Bash compilation and testing using Git worktrees." \
  -- -p --prefix help="Prefix to install to" default="" \
  -- version help="Bash version (e.g., 3.2, 4.0, 5.2, 5.3)." required=true \
  -- action choices="clear compile test all" help="Action to perform." default="all" \
  ---- "$@"

# Setup paths
BUILD_DIR="$DIR/build"
mkdir -p "$BUILD_DIR"

# Directory with the bash cloned repository full.
BASE_REPO="$BUILD_DIR/bash.git"
SRC_REPO=https://git.savannah.gnu.org/git/bash.git 

# 1. Initialize bare repository if not exists
if [[ ! -d "$BASE_REPO" ]]; then
  L_log "Initializing bare Bash clone in $BASE_REPO from $SRC_REPO"
  L_logrun git clone --bare "$SRC_REPO" "$BASE_REPO"
  L_logrun git -C "$BASE_REPO" fetch --all
fi

if [[ "$version" == "all" ]]; then
  for i in 3.2 4.0 4.1 4.2 4.3 4.4 5.0 5.1 5.2 5.3; do
    "$0" "$i" || exit
  done
  exit
fi

# 2. Resolve tag name
tag="bash-$version"
if [[ "$version" == "3.2" ]]; then
  tag="bash-3.2-beta"
fi

# 3. Add worktree if not exists
worktree_dir="$BUILD_DIR/bash-$version"
if [[ ! -d "$worktree_dir" ]]; then
  L_log "Creating git worktree for Bash $version in $worktree_dir"
  L_logrun git -C "$BASE_REPO" worktree add -f "$worktree_dir" "$tag"
fi

if L_args_contain action clear distclean; then
  L_logrun make -C $worktree_dir distclean
  exit
fi

if [[ -z "$prefix" ]]; then
  prefix="$BUILD_DIR/prefix-bash-$version"
fi

# 4. Compile Action
if L_args_contain "$action" compile install all; then
  if [[ ! -x "$prefix/bin/bash" ]]; then
    L_log "Configuring and compiling Bash $version in $worktree_dir"
    pushd "$worktree_dir" >/dev/null
    # Legacy versions need older standards
    export CFLAGS="-Wno-old-style-definition -Wno-implicit-function-declaration -std=gnu99 -Wno-int-conversion -w -Wno-implicit-int -Wno-implicit-function-declaration -Wno-discarded-qualifiers -D_GNU_SOURCE -Wno-return-mismatch"
    L_logrun ./configure prefix="$prefix"
    L_logrun make LOCAL_CFLAGS="$CFLAGS"
    L_logrun make install
    popd >/dev/null
  else
    L_log "Bash $version is already compiled."
  fi
fi

# 5. Test Action
if L_args_contain "$action" test all; then
  L_log "Building and testing builtin against Bash $version..."
  pushd "$DIR" >/dev/null
  L_logrun make build B="build/$version" BASH_INC="$prefix/include/bash/"
  L_logrun make test  B="build/$version" BASH_INC="$prefix/include/bash/"
  popd >/dev/null
fi
