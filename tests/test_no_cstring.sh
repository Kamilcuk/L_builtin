
_L_test_no_cstring() {
  # The code is allowed no dynamic allocations.
  if rg '\<CString\>' src/ -g '!src/cmd_shm.rs' -g "!src/cmd_barrier.rs" -g '!src/cmd_mutex.rs' -g '!src/cmd_semaphore.rs' -g '!src/shared.rs'; then exit 123; fi
}
