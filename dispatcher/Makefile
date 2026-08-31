.PHONY: all clean

define NL


endef
B = ../build/dispatcher

all:
	cmake -S . -B $(B) -DL_DISPATCHER_SO_FILES="${L_DISPATCHER_SO_FILES}"
	cmake --build $(B) --target all

clean:
	rm -rf $(B)

VERSIONS = 5.0 5.1 5.2 5.3
all-versions:
	$(foreach I, $(VERSIONS), $(MAKE) -C .. BASH=$(I) RELEASE=1 build$(NL))
	find ../build/Debug ../build/Release -type f -name L_builtin.so | \
  	sed -n 's@.*/\([0-9]\+[.][0-9]\+\)/.*@& $(B)/L_builtin-\1.so@p' | \
  	xargs -t -n 2 cmake -E copy_if_different
	$(MAKE) all L_DISPATCHER_SO_FILES="$(B)/L_builtin-*.so"
