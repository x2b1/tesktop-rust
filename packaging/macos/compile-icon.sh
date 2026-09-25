#!/bin/sh
# Compile native appearance variants; retain the pack's ICNS for older macOS.
set -eu
resources="$1"
mkdir -p "$resources"
xcrun actool packaging/macos/tesktop2.icon \
  --compile "$resources" --platform macosx --target-device mac \
  --minimum-deployment-target 13.0 --app-icon tesktop2 \
  --output-partial-info-plist "$resources/icon-info.plist"
cp packaging/macos/tesktop2.icns "$resources/tesktop2.icns"
rm "$resources/icon-info.plist"
