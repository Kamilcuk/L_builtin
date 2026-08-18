
_L_test_no_cstring() {
  # The code is allowed no dynamic allocations, except where C-strings are
  # required by the shared-memory database model (vardb.rs) and the FFI /
  # bash-boundary modules.
  if rg '\<CString\>' src/ -g '!src/cmd_shm.rs' -g "!src/cmd_barrier.rs" -g '!src/cmd_mutex.rs' -g '!src/cmd_semaphore.rs' -g '!src/shared.rs' -g '!src/vardb.rs' -g '!src/handles.rs'; then exit 123; fi
}
