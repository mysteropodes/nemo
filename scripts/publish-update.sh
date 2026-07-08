#!/bin/bash
# Publishes the current build as the latest auto-update, by pushing
# StrokeMotion.app.tar.gz + its .sig + latest.json to the private
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
ARTIFACT="$BUNDLE_DIR/StrokeMotion.app.tar.gz"
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
ARTIFACT_NAME="StrokeMotion.app.tar.gz"

cp "$ARTIFACT" "$REPO_DIR/$ARTIFACT_NAME"

node -e "
const fs=require('fs');
const manifest={
  version:'$VERSION',
  notes:'$NOTES'.replace(/'/g,\"\\\\'\"),
  pub_date:'$PUB_DATE',
  platforms:{
    'darwin-aarch64':{
      signature:\`$SIGNATURE\`,
      url:'https://api.github.com/repos/mysteropodes/strokemotion-updates/contents/$ARTIFACT_NAME'
    }
  }
};
fs.writeFileSync('$REPO_DIR/latest.json', JSON.stringify(manifest,null,2));
"

cd "$REPO_DIR"
git add "$ARTIFACT_NAME" latest.json
git commit -m "Release v$VERSION"
git push
echo "Published v$VERSION to strokemotion-updates."
