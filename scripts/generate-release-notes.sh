#!/bin/bash
# Script to generate release notes
# Usage: ./generate-release-notes.sh <current_ref> [<previous_ref>]

set -euo pipefail

CURRENT_REF="$1"
PREVIOUS_REF="${2:-}"

# Get repository owner/name from git remote
REPO_URL=$(git remote get-url origin 2>/dev/null || true)
if [[ "$REPO_URL" =~ github.com[:/]([^/]+)/([^/.]+)(\.git)?$ ]]; then
  REPO_OWNER="${BASH_REMATCH[1]}"
  REPO_NAME="${BASH_REMATCH[2]}"
  REPO_PATH="${REPO_OWNER}/${REPO_NAME}"
else
  REPO_PATH="${GITHUB_REPOSITORY:-}"
fi

echo "## Changes"
echo
if [ -z "$PREVIOUS_REF" ]; then
  range="${CURRENT_REF}"
else
  range="${PREVIOUS_REF}..${CURRENT_REF}"
fi
echo "Range: ${range}"
git --no-pager log "${range}" --no-merges --pretty=format:'- %h %s'
echo
if [ -n "$PREVIOUS_REF" ]; then
  echo
  echo "**Full Changelog**: https://github.com/${REPO_PATH}/compare/${PREVIOUS_REF}...${CURRENT_REF}"
fi