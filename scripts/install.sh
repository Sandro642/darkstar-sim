#!/bin/sh
# Installe dist/Darkstar.app dans /Applications (remplace l'ancienne version).
set -e
cd "$(dirname "$0")/.."
OWNER="${SUDO_USER:-$(stat -f %Su "$HOME")}"
rm -rf /Applications/Darkstar.app
ditto dist/Darkstar.app /Applications/Darkstar.app
chown -R "$OWNER":staff /Applications/Darkstar.app
xattr -dr com.apple.quarantine /Applications/Darkstar.app 2>/dev/null || true
echo "Installé : /Applications/Darkstar.app"
