.PHONY: all build release clean install uninstall man test help

PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
MANDIR = $(PREFIX)/../share/man/man1

all: release man

help:
	@echo "vfio-tool Makefile"
	@echo ""
	@echo "Targets:"
	@echo "  build       - Build debug version"
	@echo "  release     - Build release version (default)"
	@echo "  man         - Generate man page"
	@echo "  test        - Run tests"
	@echo "  install     - Install binary and man page (requires sudo)"
	@echo "  uninstall   - Uninstall binary and man page (requires sudo)"
	@echo "  clean       - Clean build artifacts"
	@echo ""
	@echo "Example:"
	@echo "  make release"
	@echo "  sudo make install"

build:
	cargo build

release:
	cargo build --release

man: vfio-tool.1

vfio-tool.1:
	cargo run --quiet --release --bin generate-man > vfio-tool.1

test:
	cargo test

install: release man
	@if [ "$$(id -u)" != "0" ]; then \
		echo "Error: Installation requires root privileges. Run: sudo make install"; \
		exit 1; \
	fi
	install -D -m 755 target/release/vfio-tool $(DESTDIR)$(BINDIR)/vfio-tool
	install -D -m 644 vfio-tool.1 $(DESTDIR)$(MANDIR)/vfio-tool.1
	@if command -v mandb >/dev/null 2>&1; then \
		mandb -q 2>/dev/null || true; \
	fi
	@echo "✓ Installation complete!"
	@echo "  Binary: $(DESTDIR)$(BINDIR)/vfio-tool"
	@echo "  Man page: $(DESTDIR)$(MANDIR)/vfio-tool.1"

uninstall:
	@if [ "$$(id -u)" != "0" ]; then \
		echo "Error: Uninstallation requires root privileges. Run: sudo make uninstall"; \
		exit 1; \
	fi
	rm -f $(DESTDIR)$(BINDIR)/vfio-tool
	rm -f $(DESTDIR)$(MANDIR)/vfio-tool.1
	@if command -v mandb >/dev/null 2>&1; then \
		mandb -q 2>/dev/null || true; \
	fi
	@echo "✓ Uninstallation complete!"

clean:
	cargo clean
	rm -f vfio-tool.1
