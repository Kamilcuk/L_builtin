.PHONY: all clean

define NL


endef
B = ../build/dispatcher

.PHONY: build all clean test-all

all: build

build:
	L_DISPATCHER_SO_FILES="${L_DISPATCHER_SO_FILES}" cargo build --release
	@mkdir -p $(B)
	cp target/release/libL_builtin.so $(B)/L_builtin.so

clean:
	rm -rf $(B) target

VERSIONS = 5.2 5.3
build-all:
	$(foreach I, $(VERSIONS), $(MAKE) -C .. BASH=$(I) RELEASE=1 build-embedded$(NL))
	$(MAKE) all L_DISPATCHER_SO_FILES="$$(for I in $(VERSIONS); do $(MAKE) -s -C .. BASH=$$I RELEASE=1 build-embedded-output; done | paste -sd';')"
	ls -lh $(B)/L_builtin.so

test-all:
	ln -svf $(abspath $(B)/L_builtin.so) ../L_builtin.so
	$(foreach I, $(VERSIONS),$(MAKE) -C .. BASH=$(I) test-vs$(NL))
