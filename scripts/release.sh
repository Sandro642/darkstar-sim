#!/bin/sh
# Publie une release GitHub téléchargeable par la mise à jour intégrée du jeu.
# Usage : scripts/release.sh patch|minor|major|X.Y.Z "Notes de version"
set -e
cd "$(dirname "$0")/.."
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/Rust/toolchain/rustup}"
export CARGO_HOME="${CARGO_HOME:-$HOME/Rust/toolchain/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

BUMP="${1:-patch}"
NOTES="${2:-Mise à jour}"
OLD=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
MA=$(echo "$OLD" | cut -d. -f1); MI=$(echo "$OLD" | cut -d. -f2); PA=$(echo "$OLD" | cut -d. -f3)
case "$BUMP" in
    major) NEW="$((MA + 1)).0.0" ;;
    minor) NEW="$MA.$((MI + 1)).0" ;;
    patch) NEW="$MA.$MI.$((PA + 1))" ;;
    *.*.*) NEW="$BUMP" ;;
    *) echo "bump inconnu : $BUMP"; exit 1 ;;
esac
sed -i '' "s/^version = \"$OLD\"/version = \"$NEW\"/" Cargo.toml

cargo test --release
scripts/bundle.sh

git add -A
git commit -m "Release v$NEW" -m "$NOTES" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
git tag "v$NEW"
git push origin HEAD --tags
gh release create "v$NEW" dist/Darkstar-macos.zip --title "Darkstar v$NEW" --notes "$NOTES"
echo "Release v$NEW publiée."
