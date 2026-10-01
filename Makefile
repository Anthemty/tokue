APP       := tokue.app
BINARY    := target/release/tokue
VERSION   := $(shell /usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" Info.plist)
DMG       := tokue-$(VERSION).dmg

.PHONY: all build app run dmg icon clean

all: app

build: $(BINARY)

$(BINARY): Cargo.toml build.rs app_darwin.m $(shell find src -name '*.rs')
	cargo build --release

app: $(BINARY)
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS
	cp $(BINARY) $(APP)/Contents/MacOS/
	cp Info.plist $(APP)/Contents/
	mkdir -p $(APP)/Contents/Resources
	cp assets/AppIcon.icns $(APP)/Contents/Resources/
	@echo "✓ $(APP) built"
	@echo "  Drag to /Applications or run: open $(APP)"

run: app
	open $(APP)

# Redraw assets/AppIcon.icns from the logo's geometry (assets/make-icon.swift).
icon:
	rm -rf dist/AppIcon.iconset
	swift assets/make-icon.swift dist/AppIcon.iconset
	iconutil -c icns dist/AppIcon.iconset -o assets/AppIcon.icns
	rm -rf dist/AppIcon.iconset
	@echo "✓ assets/AppIcon.icns drawn"

# A disk image to hand out: the app and an Applications link to drag it onto,
# with the app's icon on the volume too. Signed ad hoc (no Developer ID here),
# sealing Info.plist and the icon — a download with a partial signature is
# reported as "damaged" rather than unverified. The volume icon is a file plus
# a Finder flag on the mounted volume, so the image is built writable, given
# them, then compressed.
dmg: app
	codesign --force --sign - --identifier com.tokue.app $(APP)
	codesign --verify --strict $(APP)
	rm -rf dist $(DMG)
	mkdir -p dist/dmg dist/mnt
	ditto $(APP) dist/dmg/$(APP)
	ln -s /Applications dist/dmg/Applications
	hdiutil create -volname "tokue $(VERSION)" -srcfolder dist/dmg -ov -format UDRW dist/rw.dmg
	hdiutil attach dist/rw.dmg -nobrowse -mountpoint dist/mnt
	cp assets/AppIcon.icns dist/mnt/.VolumeIcon.icns
	SetFile -a C dist/mnt
	hdiutil detach dist/mnt
	hdiutil convert dist/rw.dmg -format UDZO -o $(DMG)
	rm -rf dist
	@echo "✓ $(DMG) built"

clean:
	rm -rf $(APP) target dist tokue-*.dmg
	@echo "✓ cleaned"
