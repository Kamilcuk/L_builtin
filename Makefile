# Makefile

###############################################################################
# ---- Default targets ----
all: test
.PHONY: all

include Makefile.bash

###############################################################################
# ---- L_builtin targets ----
# Cargo build modes (replaces CMake)
CARGO_PROFILE ?= debug
CARGO_FEATURES ?= dev

# Output paths
TARGET_DIR = target
L_BUILTIN_SO_DEBUG = $(TARGET_DIR)/debug/libL_builtin.so
L_BUILTIN_SO_RELEASE = $(TARGET_DIR)/release/libL_builtin.so

# Select binary based on profile
ifeq ($(CARGO_PROFILE),release)
L_BUILTIN_SO = $(L_BUILTIN_SO_RELEASE)
else
L_BUILTIN_SO = $(L_BUILTIN_SO_DEBUG)
endif

CARGO = BASH_SOURCE_DIR=$(BASH_SOURCE_DIR) cargo

$(L_BUILTIN_SO): $(wildcard src/*) build.rs Cargo.toml
	$(CARGO) build $(if $(filter release,$(CARGO_PROFILE)),--release,) --features $(CARGO_FEATURES)
	ln -vfs $(L_BUILTIN_SO) L_builtin.so
build: $(L_BUILTIN_SO)
TESTARGS ?= -Pn
test: build
	timeout -v -k 2 20 $(BASH_EXE) ./runtests.sh $(L_BUILTIN_SO) $(ARGS) $(TESTARGS)
release-build:
	$(MAKE) CARGO_PROFILE=release CARGO_FEATURES="" build
release-test:
	$(MAKE) CARGO_PROFILE=release CARGO_FEATURES="" test
.PHONY: build test release-build release-test

###############################################################################
# --- Additional targets ---

rustchecks:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets --all-features -- -D warnings
	$(CARGO) test --all-features

format:
	$(CARGO) fix --lib -p L_builtin --allow-dirty

check-compile-commands:
	@[ -f $(BUILD)/compile_commands.json ] || { echo "Error: $(BUILD)/compile_commands.json not found. Run make first.";  exit 1 }

tidy: check-compile-commands
	cp $(BUILD)/compile_commands.json $(BUILD)/compile_commands.bak
	sed -i 's/ -fanalyzer//g' $(BUILD)/compile_commands.json
	clang-tidy -p $(BUILD) src/*.c
	mv $(BUILD)/compile_commands.bak $(BUILD)/compile_commands.json

cppcheck: check-compile-commands
	cppcheck --project=$(BUILD)/compile_commands.json --suppress=missingIncludeSystem

distclean:
	rm -rf $(BUILD) compile_commands.json
clean:
	$(CARGO) clean

build/init.bash: build
	echo 'enable -f ./$(BUILD)/L_builtin.so L_builtin' > $@
term: build/init.bash
	bash --init-file build/init.bash $(ARGS)
gdb: build
	,gdbbatchrun bash -c "enable -f ./$(BUILD)/L_builtin.so L_builtin && L_builtin $(ARGS)"

.PHONY: all build release test check rust-checks format check-format tidy cppcheck clean sh

###############################################################################
# For all bash versions

# Hardcoded version list
BASH_VERSIONS := 4.0 4.1 4.2 4.3 4.4 5.0 5.1 5.2 5.3

.PHONY: bash-build-all
bash-build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) bash-build$(NL))

.PHONY: bash-trim-delete-all
bash-trim-delete-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) bash-trim-delete$(NL))

.PHONY: bash-install-all
bash-install-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) $(BASH_PREFIX)/bin/bash$(NL))


.PHONY: _test tes-tall test-all-all build-all

# Run test, but also run bash exe version, so I see what failed.
_test:
	$(MAKE) test || { $(BASH_EXE) --version; exit 1; }

test-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) _test$(NL))

test-all-all: bash-version-resolved
	$(foreach I,$(BASH_RESOLVED_ALL_VERSIONS),$(MAKE) BASH=$(I) _test$(NL))

build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) build$(NL))

.PHONY: test-all build-all


