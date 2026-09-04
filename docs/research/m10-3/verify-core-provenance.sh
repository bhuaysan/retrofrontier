#!/usr/bin/env bash
#
# M10.3 — re-verify the core provenance evidence in docs/CORE_BUILD_PROVENANCE.md
#
# Why this exists
# ---------------
# M10.3's central claim is that the Dolphin binary identifies its own source revision, and that the
# other three Release 002 cores do not. That claim should not have to be taken on trust from a
# document. This script re-derives it from a local, authenticated installation.
#
# It is RESEARCH TOOLING. It is deliberately isolated from the application:
#   - it is not referenced by the Rust or TypeScript build,
#   - it is not wired into CI or any package script,
#   - it is read-only and makes no network request,
#   - it changes nothing in the repository or the runtime.
#
# Usage:
#   docs/research/m10-3/verify-core-provenance.sh [<installation-cores-dir>]
#
# With no argument it locates the active installation from the RetroFrontier data directory.
#
# Exit status: 0 if every expected value matches, 1 otherwise.

set -uo pipefail

DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/com.retrofrontier.desktop"
FAILURES=0

note()  { printf '  %s\n' "$*"; }
pass()  { printf '  \033[32mPASS\033[0m  %s\n' "$*"; }
fail()  { printf '  \033[31mFAIL\033[0m  %s\n' "$*"; FAILURES=$((FAILURES + 1)); }

# ---------------------------------------------------------------------------
# Expected values. Established by M10.2 and re-verified by M10.3 on main f280905.
# component | relative .so path | sha256 | GNU build-id
# ---------------------------------------------------------------------------
EXPECTED=(
"nestopia|nestopia/nestopia_libretro.so|3f1b76f6d8e68c149a3269c314b406d15f806597333466b1f6a0af01afab52c7|8f18c1eed82244fe24d89783f7c3c6c7ba31f4ab"
"bsnes-mercury-balanced|bsnes-mercury-balanced/bsnes_mercury_balanced_libretro.so|06fe34874cf8fdec00801a2d22c497c477721a23a87a6e7b5cae82dc1770c5be|3843d7c2ecdd0ba55f3bda9819437801cc47aa73"
"beetle-psx|beetle-psx/mednafen_psx_libretro.so|ffc1c18a1fc41bf1f28cccaaa7e30e6677ec2aeda91c39b2d8f72d3bd4e2e641|4a982e5ed3f47f4a0e1635c0e87479f90fa16ec6"
"dolphin|dolphin/dolphin_libretro.so|c28dc9a2207ffed938810abf3e24df23dc39ef58c6a16c036fc2c58c2240ef10|0c693b7863fb713d45c41b54e7715111d77da1fb"
)

# The revision recovered from the Dolphin binary itself (docs/CORE_BUILD_PROVENANCE.md section 3.1).
DOLPHIN_REVISION="fd1aca3af7db75504ed7512406d8a4cf4187110a"
DOLPHIN_DESCRIBE="fd1aca3a"

# ---------------------------------------------------------------------------
# Locate the cores directory of the active installation.
# ---------------------------------------------------------------------------
resolve_cores_dir() {
  if [ "$#" -ge 1 ] && [ -n "${1:-}" ]; then
    printf '%s\n' "$1"
    return
  fi
  local active="$DATA_DIR/runtime/active.json"
  if [ ! -f "$active" ]; then
    printf '' ; return
  fi
  local id
  id=$(python3 -c "import json;print(json.load(open('$active'))['installation_id'])" 2>/dev/null) || return
  printf '%s\n' "$DATA_DIR/runtime/versions/$id/cores"
}

CORES_DIR=$(resolve_cores_dir "${1:-}")

echo
echo "M10.3 core provenance verification"
echo "=================================="

if [ -z "$CORES_DIR" ] || [ ! -d "$CORES_DIR" ]; then
  echo
  echo "  No installed runtime found."
  echo "  Expected the active installation under: $DATA_DIR/runtime/"
  echo "  Pass a cores directory explicitly to override."
  echo
  exit 1
fi

note "cores directory: $CORES_DIR"

# ---------------------------------------------------------------------------
# 1. Binary identity: SHA-256 and GNU build-id.
# ---------------------------------------------------------------------------
echo
echo "1. Binary identity (docs/CORE_BUILD_PROVENANCE.md section 2)"
echo

for row in "${EXPECTED[@]}"; do
  IFS='|' read -r component relpath want_sha want_build_id <<< "$row"
  path="$CORES_DIR/$relpath"

  if [ ! -f "$path" ]; then
    fail "$component: missing at $relpath"
    continue
  fi

  got_sha=$(sha256sum "$path" | cut -d' ' -f1)
  got_build_id=$(readelf -n "$path" 2>/dev/null \
    | grep -i 'Build ID:' | head -1 | tr -d ' \t' | sed 's/^BuildID://I')

  if [ "$got_sha" = "$want_sha" ]; then
    pass "$component sha256 $got_sha"
  else
    fail "$component sha256 expected $want_sha, got $got_sha"
  fi

  if [ "$got_build_id" = "$want_build_id" ]; then
    pass "$component build-id $got_build_id"
  else
    fail "$component build-id expected $want_build_id, got $got_build_id"
  fi
done

# ---------------------------------------------------------------------------
# 2. Toolchain fingerprint (section 2.1). Reported, not asserted: it corroborates
#    which libretro CI image built each binary, and is not a source revision.
# ---------------------------------------------------------------------------
echo
echo "2. Toolchain fingerprint (section 2.1) — informational"
echo

for row in "${EXPECTED[@]}"; do
  IFS='|' read -r component relpath _ _ <<< "$row"
  path="$CORES_DIR/$relpath"
  [ -f "$path" ] || continue
  comp=$(readelf -p .comment "$path" 2>/dev/null | grep -oE '(GCC|clang)[^$]*' | sort -u | paste -sd'; ' -)
  note "$component: ${comp:-<none>}"
done

# ---------------------------------------------------------------------------
# 3. Embedded source revision.
#    Dolphin must carry its scm_rev constants; the other three must carry none.
# ---------------------------------------------------------------------------
echo
echo "3. Embedded source revision (section 3)"
echo

for row in "${EXPECTED[@]}"; do
  IFS='|' read -r component relpath _ _ <<< "$row"
  path="$CORES_DIR/$relpath"
  [ -f "$path" ] || continue

  if [ "$component" = "dolphin" ]; then
    # Byte-exact extraction.
    #
    # Dolphin's build runs `git rev-parse HEAD` into DOLPHIN_WC_REVISION,
    # scmrev.h.in emits it as SCM_REV_STR, and Version.cpp exposes it through
    # GetScmRevGitStr(). The full 40-character value IS present in the binary,
    # but the compiler splits its construction: characters 0..31 are copied from
    # .rodata by two 16-byte SSE moves, and characters 32..39 are written from an
    # inline `movabs` immediate. A contiguous search therefore finds only 32 of
    # the 40 characters, which is why both halves are checked here.
    #
    # The other scm constants are stored as (pointer, length) pairs in
    # .data.rel.ro, so their boundaries are read from the data rather than
    # guessed from `strings` output.
    result=$(python3 - "$path" "$DOLPHIN_REVISION" "$DOLPHIN_DESCRIBE" <<'PY'
import sys
path, revision, describe = sys.argv[1], sys.argv[2], sys.argv[3]
data = open(path, 'rb').read()

findings = []
for literal in (describe.encode(), b'HEAD', b'None', describe.encode() + b' Lin'):
    if b'\x00' + literal + b'\x00' in data:
        findings.append(literal.decode())

head32 = revision[:32].encode()          # .rodata half
tail8  = revision[32:].encode()          # written by the movabs immediate
# The immediate is stored little-endian in the instruction stream.
tail8_le = tail8[::-1]

has_head = head32 in data
has_tail = (tail8 in data) or (tail8_le in data)

print("constants=" + ",".join(findings))
print("head32=" + ("yes" if has_head else "no"))
print("tail8=" + ("yes" if has_tail else "no"))
print("full=" + ("yes" if (has_head and has_tail) else "no"))
print("revstr=" + ("yes" if b'Dolphin [HEAD] ' in data else "no"))
PY
)
    constants=$(printf '%s\n' "$result" | sed -n 's/^constants=//p')
    head32=$(printf '%s\n' "$result" | sed -n 's/^head32=//p')
    tail8=$(printf '%s\n' "$result" | sed -n 's/^tail8=//p')
    full=$(printf '%s\n' "$result" | sed -n 's/^full=//p')
    revstr=$(printf '%s\n' "$result" | sed -n 's/^revstr=//p')

    note "dolphin scm constants found: ${constants:-<none>}"
    note "SCM_REV_STR halves: .rodata[0..31]=$head32  movabs[32..39]=$tail8"

    if [ "$full" = "yes" ]; then
      pass "dolphin embeds the FULL 40-char SCM_REV_STR $DOLPHIN_REVISION"
    else
      fail "dolphin does NOT embed the full 40-char SCM_REV_STR $DOLPHIN_REVISION"
    fi

    if [ "$revstr" = "yes" ]; then
      pass "dolphin embeds the 'Dolphin [HEAD] ' scm_rev string"
    else
      fail "dolphin does NOT embed the 'Dolphin [HEAD] ' scm_rev string"
    fi

    note "NOTE: this verifies the TOP-LEVEL revision only. The 33 submodule pins"
    note "      recorded in dolphin-submodules-fd1aca3a.txt are NOT verifiable from"
    note "      the binary and are not asserted as verified here."
  else
    # These three must contain no 40-character hexadecimal revision.
    #
    # A naive [0-9a-f]{40} scan also matches numeric tables and byte-lookup
    # tables that happen to fall in the ASCII hex range. Those are excluded on a
    # stated, mechanical criterion rather than by hand: a git object id is
    # effectively random, so it changes character at nearly every position,
    # whereas a table is made of long runs. Runs with fewer than 20 adjacent
    # character transitions are reported as excluded; anything else fails and
    # must be resolved against the core's own repository before it is believed.
    hits=$(python3 - "$path" <<'PY'
import re, sys
data = open(sys.argv[1], 'rb').read()
seen = set()
for m in re.finditer(rb'[0-9a-f]{40}', data):
    s = m.group().decode()
    if s in seen:
        continue
    seen.add(s)
    transitions = sum(1 for a, b in zip(s, s[1:]) if a != b)
    print(('PLAUSIBLE' if transitions >= 20 else 'TABLE'), transitions, s)
PY
)
    plausible=$(printf '%s\n' "$hits" | grep '^PLAUSIBLE' || true)
    tables=$(printf '%s\n' "$hits" | grep -c '^TABLE' || true)

    if [ -z "$plausible" ]; then
      pass "$component embeds no source revision (as expected; CI strips these binaries)"
      [ "${tables:-0}" -gt 0 ] && note "  ($tables low-entropy 40-char run(s) excluded as data tables, not revisions)"
    else
      fail "$component contains a revision-shaped 40-hex run:"
      printf '%s\n' "$plausible" | while read -r _ t s; do note "    $s (transitions=$t)"; done
      note "  investigate before treating as a revision — it must resolve in the core's own repository"
    fi
  fi
done

# ---------------------------------------------------------------------------
echo
if [ "$FAILURES" -eq 0 ]; then
  echo "All checks passed."
  echo
  echo "Reminder: only dolphin's TOP-LEVEL revision is PROVEN, and its submodule closure is"
  echo "not verified here. For nestopia, bsnes-mercury and beetle-psx the exact source"
  echo "revision is UNKNOWN; docs/CORE_BUILD_PROVENANCE.md records a public-CI candidate"
  echo "for each, awaiting libretro confirmation. A candidate must never be recorded as"
  echo "corresponding source, and does not become one by staying unchanged over time."
  echo
  exit 0
fi

echo "$FAILURES check(s) failed."
echo
exit 1
