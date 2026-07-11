#!/bin/bash
# Publishes the current build as the latest auto-update, by pushing
# <productName>.app.tar.gz + its .sig + latest.json to the private
# strokemotion-updates GitHub repo (consumed via the Contents API — see
# the comment in src-tauri/src/lib.rs for why, not GitHub Releases).
#
# One-time setup:
#   1. Create a fine-grained PAT at https://github.com/settings/tokens?type=beta
#      - Resource owner: your account
#      - Only select repository: strokemotion-updates
#      - Repository permissions: Contents = Read and write (write needed to
#        push updates from CI/this script; the app itself only ever reads)
#   2. Two separate uses for this token:
#      - Embedded in the shipped app (read-only behavior, just GETs files):
#        export STROKEMOTION_UPDATER_TOKEN=<token> before `npm run tauri build`
#      - Used by THIS script to push new releases (needs write, so re-use the
#        same token or mint a second one — your call):
#        export STROKEMOTION_PUBLISH_TOKEN=<token>
#   3. clone the repo once next to this project:
#        git clone https://github.com/mysteropodes/strokemotion-updates.git ../strokemotion-updates
#
# Usage: ./scripts/publish-update.sh "Release notes here"
set -euo pipefail
cd "$(dirname "$0")/.."

NOTES="${1:-No release notes provided.}"
REPO_DIR="../strokemotion-updates"
BUNDLE_DIR="src-tauri/target/release/bundle/macos"
# Bundle file name follows tauri.conf.json's productName (was "StrokeMotion",
# renamed to "Nemo") — hardcoding the old name here silently broke this
# script the moment the app got renamed, since the build now produces
# Nemo.app.tar.gz and the old name never lands on disk anymore.
PRODUCT_NAME=$(node -p "require('./src-tauri/tauri.conf.json').productName")
ARTIFACT="$BUNDLE_DIR/$PRODUCT_NAME.app.tar.gz"
SIG_FILE="$ARTIFACT.sig"
VERSION=$(node -p "require('./package.json').version")

if [ ! -f "$ARTIFACT" ]; then
  echo "Missing $ARTIFACT — run 'npm run tauri build' first (with TAURI_SIGNING_PRIVATE_KEY set so it gets signed)." >&2
  exit 1
fi
if [ ! -f "$SIG_FILE" ]; then
  echo "Missing $SIG_FILE — the build wasn't signed. Set TAURI_SIGNING_PRIVATE_KEY (and TAURI_SIGNING_PRIVATE_KEY_PASSWORD if the key has one) before building, or the app's updater will refuse this artifact (signature is mandatory verification, not optional)." >&2
  exit 1
fi
if [ ! -d "$REPO_DIR" ]; then
  echo "Expected the update-channel repo checked out at $REPO_DIR — see this script's header comment." >&2
  exit 1
fi

SIGNATURE=$(cat "$SIG_FILE")
PUB_DATE=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
ARTIFACT_NAME="$PRODUCT_NAME.app.tar.gz"

cp "$ARTIFACT" "$REPO_DIR/$ARTIFACT_NAME"

# Release notes are free-form text (apostrophes, accents, quotes — this IS
# French prose) — shell-interpolating them directly into a JS string
# literal broke the moment notes contained a literal ' (e.g. "captures
# d'écran"), since bash pastes the raw character in BEFORE Node ever
# parses the source, closing the string literal early regardless of the
# .replace() meant to escape it (that runs too late — on already-invalid
# JS). Environment variables sidestep this entirely: process.env.X holds
# the exact bytes verbatim, no shell/JS quoting interaction at all.
SM_VERSION="$VERSION" SM_NOTES="$NOTES" SM_PUB_DATE="$PUB_DATE" SM_SIGNATURE="$SIGNATURE" SM_ARTIFACT_NAME="$ARTIFACT_NAME" SM_REPO_DIR="$REPO_DIR" node -e "
const fs=require('fs');
const manifest={
  version: process.env.SM_VERSION,
  notes: process.env.SM_NOTES,
  pub_date: process.env.SM_PUB_DATE,
  platforms:{
    'darwin-aarch64':{
      signature: process.env.SM_SIGNATURE,
      url:'https://api.github.com/repos/mysteropodes/strokemotion-updates/contents/'+process.env.SM_ARTIFACT_NAME
    }
  }
};
fs.writeFileSync(process.env.SM_REPO_DIR+'/latest.json', JSON.stringify(manifest,null,2));
"

cd "$REPO_DIR"
git add "$ARTIFACT_NAME" latest.json
git commit -m "Release v$VERSION"
git push
echo "Published v$VERSION to strokemotion-updates."
