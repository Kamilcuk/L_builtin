
_L_test_no_cstring() {
  # The code is allowed no dynamic allocations.
  if rg '\<CString\>' src/ -g '!src/cmd_shm.rs'; then exit 123; fi
}
