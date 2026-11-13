#!/bin/bash
# Capture d'écran d'essai : lance le binaire courant dans un paquet .app de test via `open`, pour
# que macOS le traite comme une application indépendante (pas de fenêtre ni d'icône fantôme
# laissée en arrière-plan à la fin), puis attend sa fermeture.
#   scripts/shot.sh <sortie.png> <image> [scénario] [VAR=valeur ...]
# Exemple : scripts/shot.sh target/shots/a.png 300 approche "DARKSTAR_START=EGLL 27L"
set -e
cd "$(dirname "$0")/.."
mkdir -p "$(dirname "$1")"
OUT="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
FRAME="$2"
shift 2
SCEN=""
if [ $# -gt 0 ] && [[ "$1" != *=* ]]; then
  SCEN="$1"
  shift
fi
APP=target/shotapp/DarkstarShot.app
mkdir -p "$APP/Contents/MacOS"
cp target/release/darkstar_sim "$APP/Contents/MacOS/darkstar_sim"
cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>darkstar_sim</string>
<key>CFBundleIdentifier</key><string>com.darkstar.shot</string>
<key>CFBundleName</key><string>DarkstarShot</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
CMD=(/usr/bin/open -n -W -g --env "DARKSTAR_SHOT=$OUT" --env "DARKSTAR_SHOT_FRAME=$FRAME")
for kv in "$@"; do
  CMD+=(--env "$kv")
done
CMD+=("$APP")
[ -n "$SCEN" ] && CMD+=(--args "$SCEN")
# Session root (Claude Code) : lancement au nom de l'utilisateur de la session graphique
if [ "$(id -u)" = "0" ]; then
  U=$(stat -f %Su /dev/console)
  chown -R "$U" target/shotapp "$(dirname "$OUT")"
  launchctl asuser "$(id -u "$U")" sudo -u "$U" "${CMD[@]}"
else
  "${CMD[@]}"
fi
