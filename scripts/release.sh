#!/usr/bin/env bash
# Bump version, commit, tag and push. The pushed tag triggers .github/workflows/release.yml.
#
#   scripts/release.sh [patch|minor|major|X.Y.Z] [--dry-run] [--force] [--no-push]
#
#   --dry-run  show what would happen; change nothing
#   --force    allow re-using an existing tag: move it to the new commit and
#              force-push it (and the branch, with --force-with-lease)
#   --no-push  commit and tag locally only
#
# Default bump is patch. Run from a clean `main`.
set -euo pipefail
cd "$(dirname "$0")/.."

BUMP=patch DRY=0 FORCE=0 PUSH=1
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY=1 ;;
    --force) FORCE=1 ;;
    --no-push) PUSH=0 ;;
    -h | --help) sed -n '2,12p' "$0"; exit 0 ;;
    patch | minor | major | [0-9]*.[0-9]*.[0-9]*) BUMP="$arg" ;;
    *) echo "Unknown argument: $arg (see --help)" >&2; exit 1 ;;
  esac
done

CUR=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -n1)
[ -n "$CUR" ] || { echo "Cannot read current version" >&2; exit 1; }

IFS=. read -r MA MI PA <<<"$CUR"
case "$BUMP" in
  patch) NEW="$MA.$MI.$((PA + 1))" ;;
  minor) NEW="$MA.$((MI + 1)).0" ;;
  major) NEW="$((MA + 1)).0.0" ;;
  *) NEW="$BUMP" ;;
esac
TAG="v$NEW"

TAG_EXISTS=0
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && TAG_EXISTS=1
git ls-remote --exit-code --tags origin "refs/tags/$TAG" >/dev/null 2>&1 && TAG_EXISTS=1

fail() { echo "✗ $*" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || fail "Working tree is not clean."
[ "$(git branch --show-current)" = "main" ] || fail "Releases are cut from main."
if [ "$TAG_EXISTS" = 1 ] && [ "$FORCE" = 0 ]; then fail "Tag $TAG already exists (use --force to override)."; fi

PUSH_DESC="git push --force-with-lease origin HEAD && git push --force origin $TAG"
[ "$FORCE" = 1 ] || PUSH_DESC="git push origin HEAD $TAG"

if [ "$DRY" = 1 ]; then
  echo "Dry run: $CUR → $NEW ($TAG)"
  echo "  would edit  src-tauri/Cargo.toml, src-tauri/Cargo.lock, src-tauri/tauri.conf.json"
  echo "  would run   git commit -m \"chore(release): $TAG\" && git tag -a $TAG$([ "$TAG_EXISTS" = 1 ] && echo ' -f')"
  [ "$PUSH" = 1 ] && echo "  would run   $PUSH_DESC" || echo "  would not push (--no-push)"
  [ "$TAG_EXISTS" = 1 ] && echo "  note        tag $TAG exists and would be overridden (--force)"
  exit 0
fi

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
# Re-releasing the same version leaves nothing to commit: just move the tag.
git diff --cached --quiet || git commit -m "chore(release): $TAG"
TAGFLAGS=(-a)
[ "$FORCE" = 1 ] && TAGFLAGS+=(-f)
git tag "${TAGFLAGS[@]}" "$TAG" -m "scrme $TAG"

if [ "$PUSH" = 1 ]; then
  if [ "$FORCE" = 1 ]; then
    git push --force-with-lease origin HEAD
    git push --force origin "$TAG"
  else
    git push origin HEAD "$TAG"
  fi
  echo "✓ Pushed $TAG — release workflow will build and publish."
else
  echo "✓ Committed and tagged $TAG locally (--no-push). Push with: $PUSH_DESC"
fi
