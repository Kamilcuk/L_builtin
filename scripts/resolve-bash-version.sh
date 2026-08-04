#!/usr/bin/env bash
set -euo pipefail
# This script takes two argumenst - path to bash repository and what to resturn
repo="${1:?bare repo path required}"
spec="${2:-system}"
# An array of <version>|<commit number> extracted from git essages from bash commit history.
version_info=$(
    git -C "$repo" log --all --format="%H|%s" |
    sed -n '
        /^\([^|]\+\)|[Bb]ash-\([0-9]\+\.[0-9]\+\)\(.*patch[^0-9]*\([0-9]\+\)\)\?.*/s//\2.\4.|\1/p
        #  1                  2                   3               4
        /^\([^|]\+\)|Imported.*bash-\([0-9]\+\.[0-9]\+\(\.[0-9]\+\)\?\).*/s//\2.|\1/p
        #  1                         2
    '
)
all_versions=$(sed 's/\.*|.*//' <<<"$version_info" | sort -uV | grep -A9999 4.0 | grep -v 4.1.11 | paste -sd ' ')
resolve() {
    case "$1" in
        latest) chosen="${version_info//$'\n'*}" ;;
        system) resolve "${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}.${BASH_VERSINFO[2]}" ;;
        *[0-9].*) chosen=$(grep -m1 "^$1\." <<<"$version_info") ;;
        all) echo "$version_info"; exit ;;
    esac
}
chosen=""
resolve "$spec"
if [[ -z "$chosen" ]]; then
    echo "NOT FOUND $spec" >&2
    exit 123
fi
IFS='|' read -r BASH_RESOLVED_VERSION BASH_RESOLVED_COMMIT BASH_RESOLVED_MESSAGE <<<"$chosen"
BASH_RESOLVED_VERSION=${BASH_RESOLVED_VERSION%.}
BASH_RESOLVED_VERSION=${BASH_RESOLVED_VERSION%.}
echo "BASH_RESOLVED_VERSION=$BASH_RESOLVED_VERSION"
echo "BASH_RESOLVED_COMMIT=$BASH_RESOLVED_COMMIT"
echo "BASH_RESOLVED_MESSAGE=${BASH_RESOLVED_MESSAGE//$'\n'}"
echo "BASH_RESOLVED_ALL_VERSIONS=${all_versions}"
