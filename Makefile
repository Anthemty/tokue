APP       := tokue.app
BINARY    := target/release/tokue
VERSION   := $(shell /usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" Info.plist)
DMG       := tokue-$(VERSION).dmg

.PHONY: all build app run dmg clean

all: app

build: $(BINARY)

$(BINARY): Cargo.toml build.rs app_darwin.m $(shell find src -name '*.rs')
	cargo build --release

app: $(BINARY)
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS
	cp $(BINARY) $(APP)/Contents/MacOS/
	cp Info.plist $(APP)/Contents/
	@echo "✓ $(APP) built"
	@echo "  Drag to /Applications or run: open $(APP)"

run: app
	open $(APP)

# A disk image to hand out: the app and an Applications link to drag it onto.
# Signed ad hoc (no Developer ID here), sealing Info.plist too — a download
# with a partial signature is reported as "damaged" rather than unverified.
dmg: app
	codesign --force --sign - --identifier com.tokue.app $(APP)
	codesign --verify --strict $(APP)
	rm -rf dist/dmg $(DMG)
	mkdir -p dist/dmg
	ditto $(APP) dist/dmg/$(APP)
	ln -s /Applications dist/dmg/Applications
	hdiutil create -volname "tokue $(VERSION)" -srcfolder dist/dmg -ov -format UDZO $(DMG)
	rm -rf dist
	@echo "✓ $(DMG) built"

clean:
	rm -rf $(APP) target dist tokue-*.dmg
	@echo "✓ cleaned"
