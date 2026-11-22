#!/usr/bin/env bash
# Kiroween submission guard.
# Rules: the repo must contain /.kiro at the root (specs, hooks, steering) and must
# NEVER add /.kiro (or sub-folders) to .gitignore. Also requires a detectable LICENSE.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
err() { echo "FAIL: $*" >&2; fail=1; }
ok() { echo "ok:   $*"; }

[ -d .kiro ] && ok ".kiro directory exists" || err ".kiro directory is missing"

for d in specs steering hooks; do
  if [ -d ".kiro/$d" ] && [ -n "$(find ".kiro/$d" -type f | head -1)" ]; then
    ok ".kiro/$d has content"
  else
    err ".kiro/$d is missing or empty"
  fi
done

# Any non-comment .gitignore line that mentions .kiro is a violation.
if grep -RIn --include=.gitignore -E '^[^#]*\.kiro' . 2>/dev/null | grep -v node_modules >/dev/null; then
  grep -RIn --include=.gitignore -E '^[^#]*\.kiro' . | grep -v node_modules >&2 || true
  err ".kiro appears in a .gitignore (this can disqualify the submission)"
else
  ok ".kiro is not mentioned in any .gitignore"
fi

# Ask git too (covers global ignores and info/exclude), only if inside a git repo.
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  if git check-ignore -q .kiro/steering/product.md 2>/dev/null; then
    err "git ignores .kiro files (check global gitignore / .git/info/exclude)"
  else
    ok "git does not ignore .kiro files"
  fi
fi

[ -f LICENSE ] && grep -q "MIT License" LICENSE && ok "MIT LICENSE present" || err "MIT LICENSE missing"

# Cheap secret scan on tracked + untracked (non-ignored) files.
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  if git ls-files -co --exclude-standard | grep -v -E '^scripts/check-kiro\.sh$' \
      | xargs grep -I -n -E '(sk-[A-Za-z0-9]{20,}|AKIA[0-9A-Z]{16}|ghp_[A-Za-z0-9]{30,}|-----BEGIN [A-Z ]*PRIVATE KEY-----)' 2>/dev/null; then
    err "possible secret found in repository files"
  else
    ok "no obvious secrets in repository files"
  fi
fi

exit $fail
