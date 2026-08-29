#!/usr/bin/env bash
#
# parallelsort.sh - a parallel quicksort built entirely on a single
# L_builtin shm array. The integers to sort live in the bash variable ITEMS,
# which is bound to an anonymous shared-memory database. Forked worker
# processes receive a range (lo, hi), partition and sort that slice of ITEMS
# locally, and write the result back into the same ITEMS one element at a time.
#
# Every write `ITEMS[i]=x` is a read-modify-write that reloads the current
# shared array and changes a single index, so workers writing DISJOINT ranges
# never clobber each other -- no mutex is needed.
#
#   parallelsort.sh sort [--threshold N] [--] NUMBERS...
#                                          sort the given integers (or stdin)
#   parallelsort.sh unittest [--count N] [--threshold M]
#                                          run the self-test
#
# The .so is located automatically (repo/L_builtin.so, then build/Debug or
# build/Release under the repo root); override with L_BUILTIN_SO=/path/to.so.

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SCRIPT="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"
REPO=$(cd "$SCRIPT_DIR/.." && pwd)

log() { echo "parallelsort: $*" >&2; }

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
  local version=${2:-2.0.4}
	local url="https://github.com/Kamilcuk/L_lib/releases/download/v$version/L_lib.sh"
	if [[ -z "${L_LIB_VERSION:-}" ]]; then
		if [[ -s "$cachef" ]]; then
			echo "${FUNCNAME[0]}: Using preexisting $cachef"
		elif hash L_lib.sh 2>/dev/null; then
			. L_lib.sh -s
			echo "${FUNCNAME[0]}: Using L_lib.sh $L_LIB_VERSION from PATH"
			return
		elif hash curl 2>/dev/null; then
			curl -sSL -o "$cachef" "$url"
		elif hash wget 2>/dev/null; then
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

L_lib_pull "$REPO/build" >/dev/null

load_builtin() {
  local so
  if [[ -n "${L_BUILTIN_SO:-}" ]]; then
    so="$L_BUILTIN_SO"
  else
    so="$REPO/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    so="$REPO/build/Debug/system/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    so="$REPO/build/Release/system/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    echo "parallelsort: error: L_builtin.so not found (run 'make build' first)." >&2
    return 1
  fi
  enable -f "$so" L_builtin 2>/dev/null || {
    echo "parallelsort: error: failed to enable L_builtin from $so" >&2
    return 1
  }
}

# ---------------------------------------------------------------------------
# Parallel, in-place quicksort of ITEMS[lo, hi).
#
# Runs in the main shell or in a forked worker. The slice is partitioned in
# place (median-of-three pivot, straight on ITEMS -- no snapshot, no temp array).
# If the slice is small (<= threshold) it is finished sequentially; otherwise its
# two sides are sorted in parallel by forking a worker for each side. Workers only
# ever write their own [lo, hi) range, and each `ITEMS[i]=x` is a read-modify-write
# that preserves the rest of the array, so no mutex is needed.
# ---------------------------------------------------------------------------

ITEMS_swap() { local _t=${ITEMS[$1]}; ITEMS[$1]=${ITEMS[$2]}; ITEMS[$2]=$_t; }

# Partition ITEMS[lo, hi) in place around a median-of-three pivot and leave the
# pivot at its final position. Reports the pivot index through the PS_PIVOT
# global (a command substitution would fork a subshell and lose the ITEMS
# writes). Uses only scalars, never a temporary array.
_ps_partition_v_p() {
  local _lo=$1 _hi=$2
  local _n=$((_hi - _lo))
  # Median-of-three of ITEMS[lo], ITEMS[lo + n/2], ITEMS[hi-1]; move it to lo.
  local _a=$_lo _b=$((_lo + _n / 2)) _c=$((_hi - 1))
  local _va=${ITEMS[_a]} _vb=${ITEMS[_b]} _vc=${ITEMS[_c]}
  local _m=$(( _va <= _vb ? _vb <= _vc ? _b : _va <= _vc ? _c : _a : _va <= _vc ? _a : _vb <= _vc ? _c : _b ))
  if ((_m != _lo)); then
    ITEMS_swap _lo _m
  fi
  local _pivot=${ITEMS[_lo]} _i=$((_lo + 1)) _j=$((_hi - 1))
  while ((_i <= _j)); do
    while ((_i <= _j && ITEMS[_i] < _pivot)); do _i=$((_i + 1)); done
    while ((_i <= _j && ITEMS[_j] > _pivot)); do _j=$((_j - 1)); done
    if ((_i <= _j)); then
      ITEMS_swap _i _j
      _i=$((_i + 1)); _j=$((_j - 1))
    fi
  done
  ITEMS_swap _lo _j
  _p=$_j
}

psort() {
  local _lo=$1 _hi=$2 _p=""
  local _len=$((_hi - _lo))
  ((_len <= 1)) && return
  _ps_partition_v_p "$_lo" "$_hi"
  if ((_len <= threshold)); then
    echo "Sorting pid=$BASHPID from $1 to $2"
    psort "$_lo" "$_p"
    psort "$((_p + 1))" "$_hi"
  else
    local _lpid _rpid
    ((_p > _lo)) && L_with_process_into _lpid psort "$_lo" "$_p"
    ((_hi > _p + 1)) && L_with_process_into _rpid psort "$((_p + 1))" "$_hi"
    [[ -n "${_lpid:-}" ]] && wait "$_lpid"
    [[ -n "${_rpid:-}" ]] && wait "$_rpid"
  fi
}

# ---------------------------------------------------------------------------
# `sort` subcommand: read integers from args or stdin, sort them in parallel
# using the shared array, print the result one element per line.
# ---------------------------------------------------------------------------
do_sort() {
  local IFS=" "
  if [[ ! -v ITEMS[@] ]]; then
    local in=$(cat)
    ITEMS=(${in//[^0-9]/ })
  fi
  L_builtin shm bind -p ITEMS
  L_finally L_builtin shm unbind ITEMS
  local _ds_n=${#ITEMS[@]}
  if ((_ds_n > 1)); then
    psort 0 "$_ds_n"
  fi
  printf '%s\n' "${ITEMS[*]}"
}

# ---------------------------------------------------------------------------
# `unittest` subcommand: sort a random array in parallel and assert the result
# matches 'sort -n'.
# ---------------------------------------------------------------------------
run_unittest() {
  local _ut_n=$count _ut_i input=() _ut_expected _ut_got IFS=$'\n'
  for ((_ut_i = 0; _ut_i < _ut_n; _ut_i++)); do
    input+=${input:+$'\n'}$((RANDOM % 100000))
  done
  _ut_expected=$(sort -n <<<"$input")
  _ut_got=$(do_sort <<<"$input")
  if [[ "$_ut_got" == "$_ut_expected" ]]; then
    log "unittest OK ($_ut_n elements, threshold=$threshold)"
  else
    echo "parallelsort: unittest FAILED" >&2
    diff <(echo "$_ut_expected") <(echo "$_ut_got") | head >&2 || true
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Main dispatch via L_argparse.
# ---------------------------------------------------------------------------
L_argparse \
  show_default=1 \
  description="\
A parallel quicksort built entirely on a single L_builtin shm array. The \
integers live in the bash variable ITEMS, bound to an anonymous shared-memory \
database. Forked workers receive a range (lo, hi), partition and sort that slice \
of ITEMS locally, and write the result back into ITEMS one element at a time. \
Because each ITEMS[i]=x write preserves the rest of the array, workers writing \
disjoint ranges never clobber each other -- no mutex is needed.

  * sort      - sort the integers given on the command line (or stdin, one per \
line) and print them sorted, one per line.
  * unittest  - sort a randomly-generated array in parallel and assert the \
result matches 'sort -n'.

The L_builtin .so is located automatically (repo/L_builtin.so, then \
build/Debug or build/Release under the repo root); override with \
L_BUILTIN_SO=/path/to.so. Run 'make build' first if the builtin is missing. \
The --threshold option controls the leaf size below which a slice is sorted \
sequentially; smaller thresholds spawn more parallel workers." \
  epilog="\
Examples:
  parallelsort.sh sort 5 3 8 1 2
  seq 1000 | shuf | parallelsort.sh sort
  parallelsort.sh unittest --count 200 --threshold 32" \
  -- call=subparser dest=cmd \
  { \
    name=sort help="parallel merge sort of integers (from args or stdin)" \
    -- -t --threshold help="leaf size sorted sequentially (smaller => more parallelism)" type=int default=4 \
    -- ITEMS nargs='*' metavar=NUMBERS help="integers to sort; if omitted, read from stdin" \
  } \
  { \
    name=unittest help="run the self-test (random array vs sort -n)" \
    -- --count help="number of random elements" type=int default=200 \
    -- --threshold help="leaf size sorted sequentially" type=int default=32 \
  } \
  ---- "$@"

# Enable the L_builtin builtin once, up front. Every function below (and the
# children started via L_with_process_into, which are forks of this shell,
# inheriting its enabled-builtin state and the bound ITEMS) then has it
# available.
load_builtin

case "$cmd" in
  sort) do_sort "$@" ;;
  unittest) run_unittest ;;
esac
