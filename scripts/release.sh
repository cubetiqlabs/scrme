#!/usr/bin/env bash
# Bump version, commit, tag and push. The pushed tag triggers .github/workflows/release.yml.
#
#   scripts/release.sh [patch|minor|major|X.Y.Z] [--no-push]
#
# Default bump is patch. Run from a clean `main`.
set -euo pipefail
cd "$(dirname "$0")/.."

BUMP="${1:-patch}"
PUSH=1
[ "${2:-}" = "--no-push" ] && PUSH=0

CUR=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -n1)
[ -n "$CUR" ] || { echo "Cannot read current version" >&2; exit 1; }

IFS=. read -r MA MI PA <<<"$CUR"
case "$BUMP" in
  patch) NEW="$MA.$MI.$((PA + 1))" ;;
  minor) NEW="$MA.$((MI + 1)).0" ;;
  major) NEW="$((MA + 1)).0.0" ;;
  [0-9]*.[0-9]*.[0-9]*) NEW="$BUMP" ;;
  *) echo "Usage: $0 [patch|minor|major|X.Y.Z] [--no-push]" >&2; exit 1 ;;
esac
TAG="v$NEW"

[ -z "$(git status --porcelain)" ] || { echo "Working tree is not clean." >&2; exit 1; }
[ "$(git branch --show-current)" = "main" ] || { echo "Releases are cut from main." >&2; exit 1; }
git rev-parse "$TAG" >/dev/null 2>&1 && { echo "Tag $TAG already exists." >&2; exit 1; }

echo "Releasing $CUR → $NEW"
python3 - "$NEW" <<'PY'
import json, re, sys
new = sys.argv[1]
p = "src-tauri/Cargo.toml"
s = open(p).read()
s = re.sub(r'(?m)^version = ".*"$', f'version = "{new}"', s, count=1)
open(p, "w").write(s)
p = "src-tauri/tauri.conf.json"
c = json.load(open(p))
c["version"] = new
open(p, "w").write(json.dumps(c, indent=2) + "\n")
PY
cargo update -w --manifest-path src-tauri/Cargo.toml >/dev/null 2>&1 || true # sync Cargo.lock

git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/tauri.conf.json
git commit -m "chore(release): $TAG"
git tag -a "$TAG" -m "scrme $TAG"

if [ "$PUSH" = 1 ]; then
  git push origin HEAD "$TAG"
  echo "✓ Pushed $TAG — release workflow will build and publish."
else
  echo "✓ Committed and tagged $TAG locally (--no-push). Push with: git push origin HEAD $TAG"
fi
