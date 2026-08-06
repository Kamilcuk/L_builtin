# Makefile

# Version to build (from BASH=5.1 or default to system)
BASH ?= system
BUILD_DIR ?= build
SHELL = bash

define NL


endef

# Be nice
$(shell ionice -c 3 -p $$PPID >/dev/null 2>&1; \
	renice -n 19 -p $$PPID >/dev/null 2>&1; \
	chrt --idle -p 0 $$PPID >/dev/null 2>&1; \
	,nice -p $$PPID >/dev/null 2>&1;\
)

ARGS ?=

.SECONDARY:

###############################################################################
# ---- Default targets ----
all: test
.PHONY: all

###############################################################################
# --- resolving bash version ---
# bare repo
BASH_BARE_REPO = $(BUILD_DIR)/bash.git
$(BASH_BARE_REPO)/HEAD:
	[ -e $@ ] || git clone --branch master --single-branch --bare https://git.savannah.gnu.org/git/bash.git $(BASH_BARE_REPO)
# Resolved version file (depends on BASH spec)
BASH_RESOLVED_FILE := $(BUILD_DIR)/$(BASH)/bash-resolved.mk
# Generate resolved version file
$(BASH_RESOLVED_FILE): ./scripts/resolve-bash-version.sh $(BASH_BARE_REPO)/HEAD
	mkdir -vp $(dir $@)
	$< $(BASH_BARE_REPO) $(BASH) > $@
# Include resolved version (auto-regenerates Makefile on first run)
-include $(BASH_RESOLVED_FILE)
# Validation target - runs only when targets need the version
bash_version_resolved: $(BASH_RESOLVED_FILE)
	@[ -n "$(BASH_RESOLVED_COMMIT)" ] || { \
		echo "Could not resolve bash version from spec: $(BASH)"; \
		cat $(BASH_RESOLVED_FILE); \
		exit 1; \
	}

###############################################################################
# --- Building bash ---
BASH_SOURCE = $(BUILD_DIR)/$(BASH)/bash/
# git worktree depends on bare repo
$(BASH_SOURCE)/configure: bash_version_resolved $(BASH_BARE_REPO)/HEAD
	[ -e $@ ] || git -C $(BASH_BARE_REPO) worktree add -f $(abspath $(BASH_SOURCE)) $(BASH_RESOLVED_COMMIT)
export BASH_CFLAGS = -Wno-old-style-definition -Wno-implicit-function-declaration -std=gnu99 -Wno-int-conversion -w -Wno-implicit-int -Wno-discarded-qualifiers -D_GNU_SOURCE -Wno-return-mismatch -Wno-incompatible-pointer-types -Wno-error=implicit-function-declaration
BASH_EXTRA_CONFIGURE_FLAGS ?=
BASH_CONFIGURE_FLAGS ?= --disable-nls --prefix=$(abspath $(BASH_PREFIX)) $(BASH_EXTRA_CONFIGURE_FLAGS)
# Bash installation location.
BASH_PREFIX = $(BUILD_DIR)/$(BASH)/prefixbash/
# configure depends on git files (cloned repo)
$(BASH_SOURCE)/config.status: $(BASH_SOURCE)/configure
	cd $(BASH_SOURCE) && CFLAGS="$$BASH_CFLAGS" ./configure $(BASH_CONFIGURE_FLAGS)
BASH_EXE=$(BASH_SOURCE)/bash
# bash binary depends on Makefile
$(BASH_EXE): $(BASH_SOURCE)/config.status
	make -C $(BASH_SOURCE) -j$$(nproc) LOCAL_CFLAGS="$$BASH_CFLAGS" # SUBDIRS="builtins lib doc support"
	# I do not need .o files. I just need headers.
	find $(BASH_SOURCE) -name '*.o' -delete
# install target depends on bash binary
$(BASH_PREFIX)/bin/bash: $(BASH_SOURCE)/bash
	make -C $(BASH_SOURCE) install
bash-build: $(BASH_EXE)
.PHONY: bash-build

###############################################################################
# ---- L_builtin targets ----
CMAKE_BUILD_TYPE = Debug
CMAKE_FLAGS = -D L_DEV=1 -D CMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE)
BUILD = $(BUILD_DIR)/$(BASH)/build/
$(BUILD)/L_builtin.so: $(BASH_SOURCE)/bash $(wildcard src/*) ./CMakeLists.txt
	cmake -S . -B $(BUILD) $(CMAKE_FLAGS) \
		-D BASH_SOURCE=$(BASH_SOURCE)
	cmake --build $(BUILD) -j $$(nproc)
build: $(BUILD)/L_builtin.so
TESTARGS ?= -Pn
test: build
	timeout -v -k 2 20 $(BASH_EXE) ./runtests.sh $(BUILD)/L_builtin.so $(ARGS) $(TESTARGS)
release-build:
	$(MAKE) CMAKE_BUILD_TYPE=Release BUILD=$(BUILD_DIR)/$(BASH)/release build
release-test:
	$(MAKE) CMAKE_BUILD_TYPE=Release BUILD=$(BUILD_DIR)/$(BASH)/release test
.PHONY: build test release-build release-test

###############################################################################
# --- Additional targets ---

rustchecks:
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cargo test --all-features

format:
	clang-format -i src/*.c src/*.h
	cargo fix --lib -p l_builtin_rs --allow-dirty

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
	rm -rf $(BUILD_DIR) compile_commands.json
bash-distclean:
	rm -rf $(BASH_SOURCE) $(BASH_PREFIX)
bash-clean:
	$(MAKE) -C $(BASH_SOURCE) clean
	rm -rf $(BASH_PREFIX)
clean:
	rm -rf $(BUILD)

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

.PHONY: _test tes-tall test-all-all build-all bash-build-all
# Run test, but also run bash exe version, so I see what failed.
_test:
	$(MAKE) test || { $(BASH_EXE) --version; exit 1; }
test-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) _test$(NL))
test-all-all: bash_version_resolved
	$(foreach I,$(BASH_RESOLVED_ALL_VERSIONS),$(MAKE) BASH=$(I) _test$(NL))
build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) build$(NL))
bash-build-all:
	$(foreach I,$(BASH_VERSIONS),$(MAKE) BASH=$(I) bash-build$(NL))
.PHONY: test-all build-all


