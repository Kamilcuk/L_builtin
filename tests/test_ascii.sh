#!/usr/bin/env bash
# Test that README.md contains only ASCII characters and no markdown horizontal rules

_L_test_ascii_readme() {
    local file="$dir/README.md"
    if LC_ALL=C grep -q '[^[:print:][:space:]]' "$file"; then
        echo "FAIL: README.md contains non-ASCII characters" >&2
        LC_ALL=C grep -n '[^[:print:][:space:]]' "$file" | head -20 >&2
        return 1
    fi
    return 0
}

_L_test_no_horizontal_rules() {
    local file="$dir/README.md"
    if grep -q '^---$' "$file"; then
        echo "FAIL: README.md contains horizontal rules (---)" >&2
        grep -n '^---$' "$file" >&2
        return 1
    fi
    return 0
}