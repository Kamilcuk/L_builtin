function(create_bash_target target BASH_SOURCE)
  add_library(${target} INTERFACE)
  target_include_directories(${target} INTERFACE
      ${BASH_SOURCE}
      ${BASH_SOURCE}/include
      ${BASH_SOURCE}/builtins
  )
  target_compile_definitions(${target} INTERFACE
      HAVE_CONFIG_H
      SHELL
      _GNU_SOURCE=1
  )

endfunction()
