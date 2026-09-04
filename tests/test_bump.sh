# Tests for release tooling: .github/bump.yml must point at the line in
# Cargo.toml that contains the package version.

_L_test_bump_points_at_version_line() {
    local bumpyml=./.github/bump.yml cargotoml=./Cargo.toml
    local line_no entry
    entry=$(sed -n '/file-path: .Cargo.toml./{n;p;}' "$bumpyml")
    line_no=${entry##*line: }
    line_no=${line_no// /}
    local actual
    actual=$(sed -n "${line_no}p" "$cargotoml")
    [[ "$actual" =~ ^version[[:space:]]*=[[:space:]]*\"[0-9]+\.[0-9]+\.[0-9]+(\-[0-9A-Za-z.\-]+)?\"$ ]] \
        || L_unittest_fail "line ${line_no} of Cargo.toml is not version = \"X\": $actual"
}