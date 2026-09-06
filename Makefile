PREFIX ?= $(HOME)/.local
CARGO  ?= cargo
BIN     = target/release/mzera
DESKTOP_ID = dev.vdm.mzera.desktop

.PHONY: build check install default uninstall run warm deps-video

build:
	$(CARGO) build --release --locked

check:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test --all-targets

install: build
	install -Dm755 $(BIN) $(PREFIX)/bin/mzera
	install -Dm644 data/mzera.desktop $(PREFIX)/share/applications/$(DESKTOP_ID)
	install -Dm644 data/dev.vdm.mzera.png $(PREFIX)/share/icons/hicolor/512x512/apps/dev.vdm.mzera.png
	-gtk-update-icon-cache -f -t $(PREFIX)/share/icons/hicolor 2>/dev/null
	rm -f $(PREFIX)/share/applications/mzera.desktop
	install -Dm755 data/nautilus-open-in-mzera "$(HOME)/.local/share/nautilus/scripts/Open in Mzera"
	rm -f "$(HOME)/.local/share/nautilus/scripts/Open in mzera"
	-update-desktop-database $(PREFIX)/share/applications 2>/dev/null
	@echo "installed: $(PREFIX)/bin/mzera"
	@echo "make default   # set mzera as the default image/WebM viewer"
	@echo "make deps-video # install GStreamer plugins for WebM playback (needs sudo)"
	@echo "See README.md for the optional Super+Alt+P binding."

default:
	xdg-mime default $(DESKTOP_ID) image/jpeg image/png image/webp image/gif image/tiff image/bmp image/avif image/heif image/heic image/jxl image/svg+xml video/webm

uninstall:
	rm -f $(PREFIX)/bin/mzera $(PREFIX)/share/applications/$(DESKTOP_ID) \
		$(PREFIX)/share/applications/mzera.desktop \
		$(PREFIX)/share/icons/hicolor/512x512/apps/dev.vdm.mzera.png \
		"$(HOME)/.local/share/nautilus/scripts/Open in Mzera" \
		"$(HOME)/.local/share/nautilus/scripts/Open in mzera"

deps-video:
	omarchy pkg add gst-plugins-good gst-plugin-dav1d

run: build
	$(BIN) $(DIR)

warm: build
	$(BIN) --warm $(DIR)
