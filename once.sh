#!/usr/bin/env -S /bin/bash --norc --noprofile
set -euo pipefail
setx() {
  local -
  set -x
  "$@"
}
if hash L_lib.sh; then
  . L_lib.sh -s -n
fi
f="$(dirname "$(readlink -f "$0")")"/L_builtin.so
setx enable -f "$f" L_builtin
if (( $# == 1 )); then
  setx eval "L_builtin $*"
else
  setx "$@"
fi
