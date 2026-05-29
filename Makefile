APP := teletipo
DIST := dist
MAC_BUNDLE := Teletipo.app
MAC_ICONSET := $(DIST)/Teletipo.iconset
MAC_ICNS := $(DIST)/Teletipo.icns
MAC_ARM_TARGET := aarch64-apple-darwin
MAC_X64_TARGET := x86_64-apple-darwin
LINUX_TARGET := x86_64-unknown-linux-gnu
WINDOWS_TARGET := x86_64-pc-windows-gnu
INSTALLER_DIR := scripts/install

.PHONY: release release-macos release-linux release-windows release-installers verify-installers clean test lint perf

release: release-macos release-linux release-windows release-installers

release-macos:
	mkdir -p $(DIST)
	rustup target add $(MAC_ARM_TARGET) $(MAC_X64_TARGET)
	cargo build --release -p $(APP) --target $(MAC_ARM_TARGET)
	cargo build --release -p $(APP) --target $(MAC_X64_TARGET)
	lipo -create \
		target/$(MAC_ARM_TARGET)/release/$(APP) \
		target/$(MAC_X64_TARGET)/release/$(APP) \
		-output $(DIST)/$(APP)-macos-universal
	rm -rf $(MAC_ICONSET) $(MAC_ICNS) $(DIST)/$(MAC_BUNDLE)
	mkdir -p $(MAC_ICONSET)
	sips -z 16 16 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_16x16.png >/dev/null
	sips -z 32 32 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_16x16@2x.png >/dev/null
	sips -z 32 32 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_32x32.png >/dev/null
	sips -z 64 64 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_32x32@2x.png >/dev/null
	sips -z 128 128 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_128x128.png >/dev/null
	sips -z 256 256 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_128x128@2x.png >/dev/null
	sips -z 256 256 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_256x256.png >/dev/null
	sips -z 512 512 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_256x256@2x.png >/dev/null
	sips -z 512 512 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_512x512.png >/dev/null
	sips -z 1024 1024 docs/teletipo128x128.png --out $(MAC_ICONSET)/icon_512x512@2x.png >/dev/null
	iconutil -c icns $(MAC_ICONSET) -o $(MAC_ICNS)
	mkdir -p $(DIST)/$(MAC_BUNDLE)/Contents/MacOS
	mkdir -p $(DIST)/$(MAC_BUNDLE)/Contents/Resources
	cp $(DIST)/$(APP)-macos-universal $(DIST)/$(MAC_BUNDLE)/Contents/MacOS/$(APP)
	chmod +x $(DIST)/$(MAC_BUNDLE)/Contents/MacOS/$(APP)
	cp $(MAC_ICNS) $(DIST)/$(MAC_BUNDLE)/Contents/Resources/Teletipo.icns
	printf '%s\n' \
		'<?xml version="1.0" encoding="UTF-8"?>' \
		'<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
		'<plist version="1.0">' \
		'<dict>' \
		'  <key>CFBundleDevelopmentRegion</key><string>en</string>' \
		'  <key>CFBundleDisplayName</key><string>Teletipo</string>' \
		'  <key>CFBundleExecutable</key><string>teletipo</string>' \
		'  <key>CFBundleIconFile</key><string>Teletipo</string>' \
		'  <key>CFBundleIdentifier</key><string>dev.teletipo.app</string>' \
		'  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>' \
		'  <key>CFBundleName</key><string>Teletipo</string>' \
		'  <key>CFBundlePackageType</key><string>APPL</string>' \
		'  <key>CFBundleShortVersionString</key><string>0.1.0</string>' \
		'  <key>CFBundleVersion</key><string>0.1.0</string>' \
		'  <key>LSMinimumSystemVersion</key><string>11.0</string>' \
		'  <key>NSHighResolutionCapable</key><true/>' \
		'</dict>' \
		'</plist>' > $(DIST)/$(MAC_BUNDLE)/Contents/Info.plist
	cp $(DIST)/$(APP)-macos-universal $(DIST)/$(APP)
	mkdir -p $(DIST)/$(APP)-macos-universal
	cp $(DIST)/$(APP) $(DIST)/$(APP)-macos-universal/$(APP)
	cp $(INSTALLER_DIR)/install.sh $(DIST)/$(APP)-macos-universal/install.sh
	cp $(INSTALLER_DIR)/uninstall.sh $(DIST)/$(APP)-macos-universal/uninstall.sh
	cp $(INSTALLER_DIR)/README-install.txt $(DIST)/$(APP)-macos-universal/README-install.txt
	tar -czf $(DIST)/$(APP)-macos-universal.tar.gz -C $(DIST) $(APP)-macos-universal
	rm -f $(DIST)/$(APP)
	mkdir -p $(DIST)/$(APP)-macos-app
	cp -R $(DIST)/$(MAC_BUNDLE) $(DIST)/$(APP)-macos-app/$(MAC_BUNDLE)
	cp $(INSTALLER_DIR)/install.sh $(DIST)/$(APP)-macos-app/install.sh
	cp $(INSTALLER_DIR)/uninstall.sh $(DIST)/$(APP)-macos-app/uninstall.sh
	cp $(INSTALLER_DIR)/README-install.txt $(DIST)/$(APP)-macos-app/README-install.txt
	tar -czf $(DIST)/$(APP)-macos-app.tar.gz -C $(DIST) $(APP)-macos-app

release-linux:
	mkdir -p $(DIST)
	cross build --release -p $(APP) --target $(LINUX_TARGET)
	mkdir -p $(DIST)/$(APP)-linux-x86_64
	cp target/$(LINUX_TARGET)/release/$(APP) $(DIST)/$(APP)-linux-x86_64/$(APP)
	cp docs/teletipo128x128.png $(DIST)/$(APP)-linux-x86_64/teletipo.png
	mkdir -p $(DIST)/$(APP)-linux-x86_64/share/icons/hicolor/128x128/apps
	mkdir -p $(DIST)/$(APP)-linux-x86_64/share/applications
	cp docs/teletipo128x128.png $(DIST)/$(APP)-linux-x86_64/share/icons/hicolor/128x128/apps/teletipo.png
	printf '%s\n' \
		'[Desktop Entry]' \
		'Type=Application' \
		'Name=Teletipo' \
		'Comment=Teletipo Terminal' \
		'Exec=teletipo' \
		'Icon=teletipo' \
		'Terminal=false' \
		'Categories=System;TerminalEmulator;' \
		> $(DIST)/$(APP)-linux-x86_64/share/applications/teletipo.desktop
	cp $(INSTALLER_DIR)/install.sh $(DIST)/$(APP)-linux-x86_64/install.sh
	cp $(INSTALLER_DIR)/uninstall.sh $(DIST)/$(APP)-linux-x86_64/uninstall.sh
	cp $(INSTALLER_DIR)/README-install.txt $(DIST)/$(APP)-linux-x86_64/README-install.txt
	tar -czf $(DIST)/$(APP)-linux-x86_64.tar.gz -C $(DIST) $(APP)-linux-x86_64

release-windows:
	mkdir -p $(DIST)
	cross build --release -p $(APP) --target $(WINDOWS_TARGET)
	mkdir -p $(DIST)/$(APP)-windows-x86_64
	cp target/$(WINDOWS_TARGET)/release/$(APP).exe $(DIST)/$(APP)-windows-x86_64/$(APP).exe
	cp $(INSTALLER_DIR)/install.ps1 $(DIST)/$(APP)-windows-x86_64/install.ps1
	cp $(INSTALLER_DIR)/uninstall.ps1 $(DIST)/$(APP)-windows-x86_64/uninstall.ps1
	cp $(INSTALLER_DIR)/README-install.txt $(DIST)/$(APP)-windows-x86_64/README-install.txt
	cd $(DIST)/$(APP)-windows-x86_64 && zip -9 $(PWD)/$(DIST)/$(APP)-windows-x86_64.zip $(APP).exe install.ps1 uninstall.ps1 README-install.txt

release-installers:
	mkdir -p $(DIST)
	cp $(INSTALLER_DIR)/install.sh $(DIST)/install.sh
	cp $(INSTALLER_DIR)/install.ps1 $(DIST)/install.ps1
	cp $(INSTALLER_DIR)/README-install.txt $(DIST)/README-install.txt

verify-installers:
	sh -n $(INSTALLER_DIR)/install.sh
	sh -n $(INSTALLER_DIR)/uninstall.sh
	@if command -v pwsh >/dev/null 2>&1; then \
		pwsh -NoProfile -Command "Get-Command Invoke-ScriptAnalyzer -ErrorAction SilentlyContinue | Out-Null; if ($$?) { Invoke-ScriptAnalyzer -Path '$(INSTALLER_DIR)/install.ps1' -EnableExit; Invoke-ScriptAnalyzer -Path '$(INSTALLER_DIR)/uninstall.ps1' -EnableExit } else { Write-Host 'PSScriptAnalyzer not installed; skipping' }"; \
	else \
		echo "pwsh not installed; skipping PowerShell analysis"; \
	fi

clean:
	rm -rf $(DIST)

test:
	cargo test --workspace

lint:
	cargo clippy --all -- -D warnings

fmt:
	cargo fmt --all -- --check

perf:
	cargo bench --workspace
