#!/usr/bin/env bash
# Every place that names the release agrees with Cargo.toml: the lock file,
# the README and example workflow (`@vX.Y.Z`) and a CHANGELOG entry. With a
# tag (the release workflow passes it), the tag must be `v` + that version.
#   scripts/check-version.sh [vX.Y.Z]
set -uo pipefail
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
status=0
fail() {
  echo "::error::$1"
  status=1
}
if [ $# -gt 0 ] && [ "$1" != "v$version" ]; then
  fail "tag $1 does not match the Cargo.toml version $version"
fi
# Exact matches: the version is text, not a pattern, and 0.2.3 is not 0.2.30.
grep -A1 '^name = "gitbonsai"' Cargo.lock | grep -qxF "version = \"$version\"" ||
  fail "Cargo.lock is not at $version; run cargo build"
for f in README.md examples/profile-workflow.yml; do
  grep -oE 'StevenBtw/gitbonsai@v[0-9A-Za-z.+-]+' "$f" | grep -qxF "StevenBtw/gitbonsai@v$version" ||
    fail "$f does not use StevenBtw/gitbonsai@v$version"
done
if ! grep -qF "## [$version] - " CHANGELOG.md; then
  fail "CHANGELOG.md has no ## [$version] entry"
elif [ $# -gt 0 ] && grep -qxF "## [$version] - Unreleased" CHANGELOG.md; then
  echo "::warning::CHANGELOG.md still lists $version as Unreleased"
fi
[ $status -eq 0 ] && echo "version $version agrees everywhere"
exit $status
