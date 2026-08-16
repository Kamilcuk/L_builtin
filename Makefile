# Makefile

###############################################################################
# ---- Default targets ----
all: test
.PHONY: all

include Makefile.bash

###############################################################################
# ---- L_builtin targets ----
# Makefile drives CMake, which drives the Rust crate via Corrosion and the C
# glue, running bindgen and producing L_builtin.so.
RELEASE ?=
CMAKE_BUILD_TYPE ?= $(if $(RELEASE),Release,Debug)
CMAKE_FLAGS = -D L_DEV=1 -D CMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE)
CMAKE_EXTRA_FLAGS ?=
BUILD = $(BUILD_DIR)/$(CMAKE_BUILD_TYPE)/$(BASH)
$(BUILD)/L_builtin.so: $(BASH_SOURCE_DIR)/bash $(wildcard src/*) ./CMakeLists.txt Cargo.toml Cargo.lock
	cmake -S . -B $(BUILD) -D BASH_SOURCE=$(BASH_SOURCE_DIR) $(CMAKE_FLAGS) $(CMAKE_EXTRA_FLAGS)
	cmake --build $(BUILD) -j $$(nproc)
build: $(BUILD)/L_builtin.so
TESTARGS ?= -Pn
test: build
	timeout -v -k 2 20 $(BASH_EXE) ./runtests.sh $(BUILD)/L_builtin.so $(ARGS) $(TESTARGS)
release-build:
	$(MAKE) CMAKE_BUILD_TYPE=Release build
release-test:
	$(MAKE) CMAKE_BUILD_TYPE=Release test
.PHONY: build test release-build release-test

# Detect OS and architecture for release asset naming
UNAME_S := $(shell uname -s | tr '[:upper:]' '[:lower:]')
UNAME_M := $(shell uname -m)
# Release asset name: L_builtin-<os>-<arch>-bash-<version>.so
RELEASE_ASSET := L_builtin-$(UNAME_S)-$(UNAME_M)-bash-$(BASH).so
DEST ?= $(BUILD_DIR)/dest
output: build
	mkdir -vp "$(DEST)"
	cp -v "$(BUILD)/L_builtin.so" "$(DEST)/$(RELEASE_ASSET)"
dockerfile:
	$(MAKE) CMAKE_BUILD_TYPE=Release output
.PHONY: output dockerfile

###############################################################################
# --- Additional targets ---

rustchecks:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features

format:
	clang-format -i src/*.c src/*.h
	cargo fix --lib -p l_builtin --allow-dirty

check-format:
	clang-format --dry-run --Werror src/*.c src/*.h

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
	cmake --build $(BUILD) --target clean 2>/dev/null || rm -rf $(BUILD)

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


