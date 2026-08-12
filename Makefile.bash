# Makefile.bash - Bash building targets (extracted for Docker caching)
# This file contains only bash version resolution, cloning, building, and trimming targets

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

###############################################################################
# ---- Bash version resolution ----
# bare repo
BASH_BARE_REPO = $(BUILD_DIR)/bash.git
$(BASH_BARE_REPO)/HEAD:
	[ -e $@ ] || git clone --branch master --single-branch --bare https://git.savannah.gnu.org/git/bash.git $(BASH_BARE_REPO)

# Resolved version file (depends on BASH spec)
BASH_RESOLVED_FILE := $(BUILD_DIR)/bash-resolved.mk

# Generate resolved version file
bash-resolve: $(BASH_RESOLVED_FILE)
$(BASH_RESOLVED_FILE): ./scripts/resolve-bash-version.sh $(BASH_BARE_REPO)/HEAD
	mkdir -vp $(dir $@)
	$< $(BASH_BARE_REPO) $(BASH) > $@

# Include resolved version (auto-regenerates Makefile on first run)
-include $(BASH_RESOLVED_FILE)

###############################################################################
# ---- Building bash ----
BASH_SOURCE_DIR = $(BUILD_DIR)/bash/$(BASH)

# git worktree depends on bare repo
$(BASH_SOURCE_DIR)/configure: $(BASH_BARE_REPO)/HEAD
	@[ -n "$(BASH_RESOLVED_COMMIT)" ] || { \
		echo "Could not resolve bash version from spec: $(BASH)"; \
		cat $(BASH_RESOLVED_FILE); \
		exit 1; \
	}
	[ -e $@ ] || git -C $(BASH_BARE_REPO) worktree add -f $(abspath $(BASH_SOURCE_DIR)) $(BASH_RESOLVED_COMMIT)

export BASH_CFLAGS = -Wno-old-style-definition -Wno-implicit-function-declaration -std=gnu99 -Wno-int-conversion -w -Wno-implicit-int -Wno-discarded-qualifiers -D_GNU_SOURCE -Wno-return-mismatch -Wno-incompatible-pointer-types -Wno-error=implicit-function-declaration

BASH_EXTRA_CONFIGURE_FLAGS ?=
BASH_CONFIGURE_FLAGS ?= --disable-nls --without-bash-malloc --prefix=$(abspath $(BASH_PREFIX)) $(BASH_EXTRA_CONFIGURE_FLAGS)

# Bash installation location.
BASH_PREFIX = $(BUILD_DIR)/prefixbash/$(BASH)

# configure depends on git files (cloned repo)
$(BASH_SOURCE_DIR)/config.status: $(BASH_SOURCE_DIR)/configure
	cd $(BASH_SOURCE_DIR) && CFLAGS="$$BASH_CFLAGS" ./configure $(BASH_CONFIGURE_FLAGS)

BASH_EXE=$(BASH_SOURCE_DIR)/bash

# bash binary depends on Makefile
$(BASH_EXE): $(BASH_SOURCE_DIR)/config.status
	make -C $(BASH_SOURCE_DIR) -j$$(nproc) LOCAL_CFLAGS="$$BASH_CFLAGS"

# install target depends on bash binary
bash-install: $(BASH_PREFIX)/bin/bash
$(BASH_PREFIX)/bin/bash: $(BASH_SOURCE_DIR)/bash
	make -C $(BASH_SOURCE_DIR) install

bash-build: $(BASH_EXE)

bash-distclean:
	rm -rf $(BASH_SOURCE_DIR) $(BASH_PREFIX)

bash-clean:
	$(MAKE) -C $(BASH_SOURCE_DIR) clean
	rm -rf $(BASH_PREFIX)

BASH_TRIM = find $(BASH_SOURCE_DIR) -type f \
	! -name '*.[hc]' \
	! -name 'bash' \
	! -path '*/.git/*' \
	! -name '.git' \
	! -name 'configure' \
	! -name 'config.*' \
	! -name 'install-sh' \
	#

bash-trim-print: ; $(BASH_TRIM) -print ; $(BASH_TRIM) -exec du -ch {} + | tail -n1

bash-trim-delete: ; $(BASH_TRIM) -print -delete

.PHONY: bash-build bash-distclean bash-clean bash-trim-print bash-trim-delete

###############################################################################

.PHONY: bash-dockerfile
bash-dockerfile: bash-resolve bash-install bash-trim-delete
