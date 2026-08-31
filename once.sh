#!/usr/bin/env -S /bin/bash --norc --noprofile
set -euo pipefail
setx() {
  local -
  set -x
  "$@"
}
b() { L_builtin "$@"; }
if hash L_lib.sh; then
  . L_lib.sh -s -n
fi
if [[ -f "$1" ]]; then
  f=$1
  shift
else
  f="$(dirname "$(readlink -f "$0")")"/L_builtin.so
fi
setx enable -f "$f" L_builtin
if (( $# == 1 )); then
  setx eval "$*"
else
  setx L_builtin "$@"
fi
