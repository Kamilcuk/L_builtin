# Makefile

noop=
space = $(noop) $(noop)
comma = ,

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
BASHES ?= 5.3 5.2 5.1 5.0 4.4

BASHES_U := $(subst .,_,$(BASHES))

.PHONY: bash-build-all
bash-build-all:
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) bash-build$(NL))

.PHONY: bash-trim-delete-all
bash-trim-delete-all:
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) bash-trim-delete$(NL))

.PHONY: bash-install-all
bash-install-all:
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) $(BASH_PREFIX)/bin/bash$(NL))

###############################################################################

$(BUILD_DIR)/rust/%/Cargo.toml: ./l_builtin/Cargo.tmpl.toml ./l_builtin/Cargo.lock ./l_builtin/src # Makefile
	@mkdir -p $(dir $@)
	ln -svfr ./l_builtin/Cargo.lock $(dir $@)Cargo.lock
	ln -nsvfr ./l_builtin/src $(dir $@)src
	sed 's/%VERSION%/$*/' < ./l_builtin/Cargo.tmpl.toml > $(dir $@)Cargo.toml
.PHONY: prepare-rust-workspace
workspace-rust: $(BASHES_U:%=$(BUILD_DIR)/rust/%/Cargo.toml)
workspace-rust-trim:
	cd $(BUILD_DIR)/rust && printf "%s\n" $(BASHES_U) | sort | comm -13 - <(printf "%s\n" *) | xargs -rt rm -rf
workspace-rust-clean:
	rm -rf $(BUILD_DIR)/rust/*

###############################################################################
# ---- L_builtin targets ----
# Makefile drives CMake, which drives the Rust crate via Corrosion and the C
# glue, running bindgen and producing L_builtin.so.
RELEASE ?=
CMAKE_BUILD_TYPE ?= $(if $(RELEASE),Release,Debug)
L_DEV ?= 1
CMAKE_FLAGS = \
	-DL_DEV=$(L_DEV) \
	-DCMAKE_BUILD_TYPE=$(CMAKE_BUILD_TYPE) \
	$(if $(shell command -v ninja 2>/dev/null),-GNinja) \
	#
CMAKE_EXTRA_FLAGS ?=

# Per-version bash source trees live under this directory, one subdir per
# version (e.g. $(BASH_SOURCES_DIR)/5.3). Matches Makefile.bash's
# BASH_SOURCE_DIR = $(BUILD_DIR)/bash/$(BASH) layout.
BASH_SOURCES_DIR = $(BUILD_DIR)/bash

NPROC ?= $$(nproc)
VERBOSE =
CMAKE_DIR = $(BUILD_DIR)/$(CMAKE_BUILD_TYPE)

define cmake_build
	@$(MAKE) workspace-rust
	cmake -S. -B$(CMAKE_DIR) -DBASHES="$(BASHES)" -DBASH_SOURCES_DIR=$(BASH_SOURCES_DIR) $(CMAKE_FLAGS) $(CMAKE_EXTRA_FLAGS)
	cmake --build $(CMAKE_DIR) -j $(NPROC) $(if $(VERBOSE),--verbose) --target
endef


# Regular build: for direct bash use
BASH_U := $(subst .,_,$(BASH))
build:
	$(cmake_build) L_builtin_standalone_$(BASH_U)

# Embedded build: for dispatcher use
# Builds L_builtin_embedded_$(BASH).so in the main build dir
build-embedded:
	$(cmake_build) L_builtin_embedded_$(BASH_U)

TESTARGS ?= -Pn

runtests = timeout -v -k 2 -- 2m $1 ./runtests.sh $2 $(ARGS) $(TESTARGS)
test:
	$(call runtests, $(BASH_EXE), $(CMAKE_DIR)/L_builtin_standalone_$(BASH_U).so)
ANY_BASH = $(subst .,_,$(firstword $(BASHES)))
cargo-test:
	@echo "=== cargo test (Rust unit tests) ==="
	GENERATED_RUST=$(CMAKE_DIR)/generated_rust/$(ANY_BASH) cargo test -p l_builtin_$(ANY_BASH)
ifeq ($(L_DEV),1)
	@echo "=== L_builtin unittest (in-process, dev build only) ==="
	$(BASH_EXE) -c 'enable -f $(CMAKE_DIR)/L_builtin.so L_builtin; L_builtin unittest'
endif

release-build:
	$(MAKE) CMAKE_BUILD_TYPE=Release build
	$(MAKE) CMAKE_BUILD_TYPE=Release build-embedded

build-embedded-output:
	@echo $(CMAKE_DIR)/L_builtin_embedded.so

release-test:
	$(MAKE) CMAKE_BUILD_TYPE=Release test

.PHONY: build test release-build release-test build-embedded build-embedded-output

###############################################################################
# --- Additional targets ---

rustchecks:
	cd l_builtin && cargo fmt --all -- --check
	cd l_builtin && cargo clippy --all-targets --all-features -- -D warnings
	cd l_builtin && cargo test --all-features

format:
	git ls-files '*.c' '*.h' | xargs clang-format -i
	cd l_builtin && cargo fix --lib --allow-dirty

check-format:
	git ls-files '*.c' '*.h' | xargs clang-format --dry-run --Werror

check-compile-commands:
	@[ -f $(CMAKE_DIR)/compile_commands.json ] || { echo "Error: $(CMAKE_DIR)/compile_commands.json not found. Run make first.";  exit 1 }

tidy: check-compile-commands
	cp $(CMAKE_DIR)/compile_commands.json $(CMAKE_DIR)/compile_commands.bak
	sed -i 's/ -fanalyzer//g' $(CMAKE_DIR)/compile_commands.json
	clang-tidy -p $(CMAKE_DIR) src/*.c
	mv $(CMAKE_DIR)/compile_commands.bak $(CMAKE_DIR)/compile_commands.json

cppcheck: check-compile-commands
	cppcheck --project=$(CMAKE_DIR)/compile_commands.json --suppress=missingIncludeSystem

clean:
	cd "$(CMAKE_DIR)" && rm -rf ./*

build/init.bash: build
	echo 'enable -f ./$(CMAKE_DIR)/L_builtin.so L_builtin' > $@
term: build/init.bash
	bash --init-file build/init.bash $(ARGS)
gdb: build
	,gdbbatchrun bash -c "enable -f ./$(CMAKE_DIR)/L_builtin.so L_builtin && L_builtin $(ARGS)"

readme: build
	uv run --with markdown-it-py scripts/gen_readme.py --so $(CMAKE_DIR)/L_builtin.so --bash $(BASH_EXE)

###############################################################################
# ---- Dispatcher targets ----

.PHONY: dispatcher-build dispatcher-build-all dispatcher-test-all

L_D_SO = $(CMAKE_DIR)/libL_builtin_dispatcher.so
dispatcher-build:
	$(foreach BASH,$(BASHES),$(MAKE) BASH=$(BASH) build-embedded$(NL))
	$(cmake_build) dispatcher
	ls -lah $(L_D_SO)
dispatcher-test: dispatcher-build
	./once.sh $(L_D_SO) 'L_builtin -h && L_builtin -h'
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) L_BUILTIN_SO=$(L_D_SO) once-vs$(NL))
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) L_BUILTIN_SO=$(L_D_SO) test-vs$(NL))
dispatcher-clean:
	cd dispatcher && cargo clean --target-dir ../build/dispatcher/target

.PHONY: all build release test check rust-checks format check-format tidy cppcheck clean sh readme

###############################################################################
# For all bash versions

.PHONY: _test tes-tall test-all test-vs-all build-all

# Run test, but also run bash exe version, so I see what failed.
_test:
	$(MAKE) test || { $(BASH_EXE) --version; exit 1; }

test-all:
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) _test$(NL))

# Test .so file build by the last run vs all bash versions.
test-vs-all: build
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) test-vs$(NL))

L_BUILTIN_SO = ./L_builtin.so
test-vs: bash-build
	$(call runtests, $(BASH_EXE), $(L_BUILTIN_SO))
once-vs:
	timeout -v -k 2 20 $(BASH_EXE) ./once.sh $(L_BUILTIN_SO) 'L_builtin -h'

build-all:
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) build$(NL))
	$(foreach I,$(BASHES),$(MAKE) BASH=$(I) build-embedded$(NL))

.PHONY: test-all build-all

###############################################################################
# docker tests and local executions

define IMAGES_LINES
	# docker.io/library/debian:10-slim
	docker.io/library/debian:11-slim
	docker.io/library/debian:12-slim
	# docker.io/library/ubuntu:18.04
	docker.io/library/ubuntu:20.04
	docker.io/library/ubuntu:22.04
	docker.io/library/ubuntu:24.04
	$$(podman build -q --target alpine .)
	docker.io/library/fedora:latest
	docker.io/library/archlinux:latest
	# comment
endef
IMAGES = \
				 $(filter-out @@, \
				 $(filter-out @, \
				 $(filter-out #%, \
				 $(subst ^,$(space), \
				 $(subst $(space),@, \
				 $(subst $(NL),^,$(IMAGES_LINES) \
				 ))))))
define FOREACH_IMAGE
	$(foreach IMAGE,$(IMAGES), \
		$(subst %IMAGE%,$(subst @,$(space),$(IMAGE)),$1$2$3$4$5$6$7$8$9)$(NL) \
	)
endef
TTY_FLAG = $(shell [ -t 0 ] && echo "-t")
DOCKER_RUN = podman run --rm $(TTY_FLAG) -v "$(CURDIR):$(CURDIR):ro" -w "$(CURDIR)"
define DOCKER_RUNS
	$(call FOREACH_IMAGE,$(DOCKER_RUN) %IMAGE% $1$2$3$4$5$6$7$8$9)
endef
indocker-images:
	$(call FOREACH_IMAGE, @echo '%IMAGE%' )
indocker-libc-verison:
	$(call FOREACH_IMAGE, \
		@podman run --rm %IMAGE% ldd --version | head -n1 \
	)
indocker-bash-versions:
	$(call FOREACH_IMAGE, \
		@podman run --rm %IMAGE% bash --version | awk '{print "\t%IMAGE%\t",$$0;exit}' \
	)
indocker-once:
	$(call DOCKER_RUNS, bash -c './once.sh "b -h"' )
indocker-test:
	$(call DOCKER_RUNS, bash -c './once.sh "b -h"' )
.PHONY: indocker-once indocker-bash-versions indocker-images

###############################################################################
# dockerfile support

DESTDIR = ./build/dest
install:
	@mkdir -vp $(DESTDIR)
	ls -la $(CMAKE_DIR)
	cp -va $(CMAKE_DIR)/L_builtin_standalone_[0-9]*.so $(DESTDIR)
	cp -va $(CMAKE_DIR)/libL_builtin_dispatcher.so $(DESTDIR)/L_builtin.so
	ls -la $(DESTDIR)

docker-build:
	podman build --target build .
docker-test:
	podman build --target test .
docker-install:
	podman build --target build-output --output type=local,dest=$(DESTDIR) .

dockerfile-build: CMAKE_EXTRA_FLAGS = -DCARGO_LOCKED=ON
dockerfile-build:
	@echo 'BASHES=$(BASHES)'
	$(foreach BASH,$(BASHES),\
		$(MAKE) BASH=$(BASH) build CMAKE_EXTRA_FLAGS=$(CMAKE_EXTRA_FLAGS) \
	$(NL))
	$(MAKE) dispatcher-build CMAKE_EXTRA_FLAGS=-DCARGO_LOCKED=ON
	$(MAKE) install
dockerfile-test:
	$(foreach BASH, $(BASHES),\
		$(call runtests, ./build/bash/$(BASH)/bash, $(DESTDIR)/L_builtin.so)$(NL) \
		$(call runtests, ./build/bash/$(BASH)/bash, $(DESTDIR)/L_builtin_standalone_$(subst .,_,$(BASH)).so)$(NL) \
	)

###############################################################################
