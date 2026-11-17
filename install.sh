#!/bin/sh
# Build LinkRouter.app into ~/Applications and register it with Launch Services.
set -eu
cd "$(dirname "$0")"
cargo build --release
APP="$HOME/Applications/LinkRouter.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp Info.plist "$APP/Contents/Info.plist"
cp target/release/linkrouter "$APP/Contents/MacOS/LinkRouter"
codesign --force -s - "$APP"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$APP"
mkdir -p "$HOME/.config/linkrouter"
[ -f "$HOME/.config/linkrouter/rules" ] || cp rules.example "$HOME/.config/linkrouter/rules"
echo "Installed $APP"
