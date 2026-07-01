# fsmp — Makefile
#
# Common developer tasks. `make install` builds a release binary and installs it
# to $(BINDIR). Run `make help` to list targets.

CARGO ?= cargo
BIN    = fsmp

# Mirror the runtime layout: the binary installs alongside state/ under the
# fsmp home dir. FSMP_HOME overrides ~/.fsmp for both install and runtime.
FSMP_HOME ?= $(HOME)/.fsmp
BINDIR     = $(FSMP_HOME)/bin

RELEASE_BIN = target/release/$(BIN)

.PHONY: all build release test fmt fmt-check clippy check run install uninstall clean help

all: build

## build: debug build
build:
	$(CARGO) build

## release: optimized release build
release:
	$(CARGO) build --release

## test: run unit + integration tests
test:
	$(CARGO) test

## fmt: format the code in place
fmt:
	$(CARGO) fmt

## fmt-check: verify formatting without modifying files
fmt-check:
	$(CARGO) fmt --check

## clippy: lint with warnings treated as errors
clippy:
	$(CARGO) clippy --all-targets -- -D warnings

## check: fmt-check + clippy + test (the CI aggregate)
check: fmt-check clippy test

## run: run the debug binary; pass arguments via ARGS="..."
run:
	$(CARGO) run -- $(ARGS)

## install: build release and install to $(BINDIR)
install: release
	install -d "$(BINDIR)"
	install -m 755 "$(RELEASE_BIN)" "$(BINDIR)/$(BIN)"
	@echo "installed $(BIN) -> $(BINDIR)/$(BIN)"
	@echo "ensure $(BINDIR) is on your PATH, e.g. add to your shell profile:"
	@printf '    export PATH="%s:$$PATH"\n' "$(BINDIR)"

## uninstall: remove the installed binary
uninstall:
	rm -f "$(BINDIR)/$(BIN)"
	@echo "removed $(BINDIR)/$(BIN)"

## clean: remove build artifacts
clean:
	$(CARGO) clean

## help: list available targets
help:
	@grep -E '^## ' $(MAKEFILE_LIST) | sed -e 's/^## /  /'
