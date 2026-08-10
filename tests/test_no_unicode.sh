_L_test_no_unicode() {
  # Source files must contain only ASCII characters (7-bit); no unicode.
  if rg -n '[^\x00-\x7F]' src/; then exit 123; fi
}