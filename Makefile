# Makefile

###############################################################################
# ---- Default targets ----
all: test
.PHONY: all

###############################################################################

include Makefile.bash

bash-clean:
	$(MAKE) -C $(BASH_SOURCE_DIR) clean
	rm -rf $(BASH_PREFIX)

bash-distclean:
	rm -rf $(BASH_SOURCE_DIR) $(BASH_PREFIX)

bash-version: bash-build
	$(BASH_EXE) --version

.PHONY: bash-clean bash-distclean bash-version

# Hardcoded version list
BASH_VERSIONS := 4.0 4.2 4.3 4.4 5.0 5.1 5.2 5.3

.PHONY: bash-build-all
bash-build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) bash-build$(NL))

.PHONY: bash-trim-delete-all
bash-trim-delete-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) bash-trim-delete$(NL))

.PHONY: bash-install-all
bash-install-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) $(BASH_PREFIX)/bin/bash$(NL))

###############################################################################
# ---- L_builtin targets ----
# Makefile drives CMake, which drives the Rust crate via Corrosion and the C
# glue, running bindgen and producing L_builtin.so.
RELEASE ?=
CMAKE_BUILD_TYPE ?= $(if $(RELEASE),Release,Debug)
L_DEV ?= 1
CMAKE_FLAGS = -D L_DEV=$(L_DEV) -D CMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE)
CMAKE_EXTRA_FLAGS ?=

# Regular build: for direct bash use
BUILD = $(BUILD_DIR)/$(CMAKE_BUILD_TYPE)/$(BASH)
$(BUILD)/L_builtin.so: $(BASH_SOURCE_DIR)/bash $(wildcard src/*) ./CMakeLists.txt l_builtin/Cargo.toml l_builtin/Cargo.lock
	cmake -S . -B $(BUILD) -D BASH_SOURCE=$(BASH_SOURCE_DIR) $(CMAKE_FLAGS) $(CMAKE_EXTRA_FLAGS)
	cmake --build $(BUILD) -j $$(nproc)

# Embedded build: for dispatcher use
# Builds L_builtin.so in _embedded subdir, then copies to L_builtin_embedded.so in main build dir
$(BUILD)_embedded:
	mkdir -p $@
$(BUILD)/L_builtin_embedded.so: $(BASH_SOURCE_DIR)/bash $(wildcard src/*) ./CMakeLists.txt l_builtin/Cargo.toml l_builtin/Cargo.lock | $(BUILD)_embedded
	cmake -S . -B $(BUILD)_embedded -D BASH_SOURCE=$(BASH_SOURCE_DIR) $(CMAKE_FLAGS) $(CMAKE_EXTRA_FLAGS) -D L_BUILTIN_EMBEDDED=1
	cmake --build $(BUILD)_embedded -j $$(nproc)
	cp $(BUILD)_embedded/L_builtin.so $(BUILD)/L_builtin_embedded.so

build: $(BUILD)/L_builtin.so

# Build both regular and embedded versions
build-all: $(BUILD)/L_builtin.so $(BUILD)/L_builtin_embedded.so


# For release builds
release-build:
	$(MAKE) CMAKE_BUILD_TYPE=Release build
	$(MAKE) CMAKE_BUILD_TYPE=Release build-embedded

build-embedded: $(BUILD)/L_builtin_embedded.so

release-test:
	$(MAKE) CMAKE_BUILD_TYPE=Release test

TESTARGS ?= -Pn
test: build
	timeout -v -k 2 20 $(BASH_EXE) ./runtests.sh $(BUILD)/L_builtin.so $(ARGS) $(TESTARGS)
	@echo "=== cargo test (Rust unit tests) ==="
	cd l_builtin && cargo test
ifeq ($(L_DEV),1)
	@echo "=== L_builtin unittest (in-process, dev build only) ==="
	$(BASH_EXE) -c 'enable -f $(BUILD)/L_builtin.so L_builtin; L_builtin unittest'
endif
release-build:
	$(MAKE) CMAKE_BUILD_TYPE=Release build
	$(MAKE) CMAKE_BUILD_TYPE=Release build-embedded

build-embedded: $(BUILD)/L_builtin_embedded.so

release-test:
	$(MAKE) CMAKE_BUILD_TYPE=Release test
.PHONY: build test release-build release-test

###############################################################################
# --- Additional targets ---

rustchecks:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features

format:
	clang-format -i src/*.c src/*.h dispatcher/*.c
	cargo fix --lib -p l_builtin --allow-dirty

check-format:
	clang-format --dry-run --Werror src/*.c src/*.h dispatcher/*.c

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
	cmake --build $(BUILD)_embedded --target clean 2>/dev/null || rm -rf $(BUILD)_embedded || true

build/init.bash: build
	echo 'enable -f ./$(BUILD)/L_builtin.so L_builtin' > $@
term: build/init.bash
	bash --init-file build/init.bash $(ARGS)
gdb: build
	,gdbbatchrun bash -c "enable -f ./$(BUILD)/L_builtin.so L_builtin && L_builtin $(ARGS)"

readme: build
	uv run --with markdown-it-py scripts/gen_readme.py --so $(BUILD)/L_builtin.so --bash $(BASH_EXE)
.PHONY: all build release test check rust-checks format check-format tidy cppcheck clean sh readme

###############################################################################
# For all bash versions

.PHONY: _test tes-tall test-all test-vs-all build-all

# Run test, but also run bash exe version, so I see what failed.
_test:
	$(MAKE) test || { $(BASH_EXE) --version; exit 1; }

test-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) _test$(NL))

# Test .so file build by the last run vs all bash versions.
test-vs-all: build
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) test-vs$(NL))

test-vs: bash-build
	timeout -v -k 2 20 $(BASH_EXE) ./runtests.sh ./L_builtin.so $(ARGS) $(TESTARGS)

build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) build$(NL))
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) build-embedded$(NL))

.PHONY: test-all build-all

###############################################################################
