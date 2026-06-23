APP       := OCGTool.app
BINARY    := target/release/ocg

.PHONY: all build app run clean

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

clean:
	rm -rf $(APP) target
	@echo "✓ cleaned"
