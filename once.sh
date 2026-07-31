#!/usr/bin/env -S /bin/bash --norc --noprofile
set -euo pipefail
f="$(dirname "$(readlink -f "$0")")"/L_builtin.so
set -x
enable -f "$f" L_builtin
"$@"
