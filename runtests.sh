#!/usr/bin/env bash
set -euo pipefail

L_lib_pull() {
  L_lib_pull_fatal() {
	  echo "$@" >&2
	  exit 123
  }
	if [[ -z "${1:-}" ]]; then
		L_lib_pull_fatal "Usage: ${FUNCNAME[0]} <destination directory>"
	fi
	mkdir -vp "$1"
	local cachef="$1"/L_lib.sh
  local version=${2:-2.0.2}
	local url="https://github.com/Kamilcuk/L_lib/releases/download/v$version/L_lib.sh"
	if [[ -z "${L_LIB_VERSION:-}" ]]; then
		# Download L_lib.sh library
		if [[ -s "$cachef" ]]; then
			echo "${FUNCNAME[0]}: Using preexisting $cachef"
		elif hash L_lib.sh 2>/dev/null; then
			. L_lib.sh -s
			echo "${FUNCNAME[0]}: Using L_lib.sh $L_LIB_VERSION from PATH"
			return
		elif hash curl 2>/dev/null; then
			echo "${FUNCNAME[0]}: Downloading L_lib.sh from $url with curl"
			curl -sSL -o "$cachef" "$url"
		elif hash wget 2>/dev/null; then
			echo "${FUNCNAME[0]}: Downloading L_lib.sh from $url with wget"
			wget -O "$cachef" "$url"
		else
			L_lib_pull_fatal "Could not find L_lib.sh and no download method available"
		fi
		if [[ -s "$cachef" ]]; then
			. "$cachef" -s
		else
			L_lib_pull_fatal "Downloading L_lib.sh from $url has failed"
		fi
	fi
}

ulimit -c 0
export TIMEFORMAT='real=%6lR user=%6lU system=%6lS'
dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
L_lib_pull "$dir/${B:=build}"

{
	renice -n 39 $$ || :
	ionice -c 3 $$ || :
	chrt -i -p $$ || :
} >/dev/null 2>/dev/null

if L_is_main; then

	# Load L_builtin
	module="${1:-./L_builtin.so}"
	if (($#)); then shift; fi
	L_info "BASH_VERSION=$BASH_VERSION module=$module"

	if [[ ! -f $module ]]; then
    	L_panic "Error: $module not found. Run make first."
	fi
	enable -f "$module" L_builtin

	# Source all modular test files
	for f in "$dir"/tests/test_*.sh; do
    	. "$f"
	done

	L_trap_err_enable
	L_unittest_main -p _L_test_ "$@"
fi
