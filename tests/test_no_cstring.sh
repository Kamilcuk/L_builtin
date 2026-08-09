
_L_test_no_cstring() {
  # The code is allowed no dynamic allocations.
  if rg '\<CString\>' src/; then exit 123; fi
}
