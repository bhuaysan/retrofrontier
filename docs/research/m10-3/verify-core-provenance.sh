#!/usr/bin/env bash
#
# M10.3 — re-verify the core provenance evidence in docs/CORE_BUILD_PROVENANCE.md
#
# Why this exists
# ---------------
# M10.3's central claim is that the Dolphin binary identifies its own source revision. That claim
# should not have to be taken on trust from a document. This script re-derives it from a local,
# authenticated installation, at instruction level: it locates the construction site of Dolphin's
# static SCM revision string and reassembles the 40 characters from the operands that site actually
# uses, rather than searching the file for bytes that happen to be present somewhere.
#
# For the other three cores the script asserts only the narrow, falsifiable negative the document
# records: the documented public-CI candidate revision does not occur in the binary as an embedded
# revision identifier. Its generic 40-hex scan is DIAGNOSTIC ONLY and establishes no conclusion.
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

# Public-CI candidate revisions for the other three cores (section 3.2). These are CANDIDATES, never
# corresponding source. They are listed here only so the script can re-assert the document's negative
# finding: none of them occurs in its binary as an embedded revision identifier.
CANDIDATES=(
"nestopia|5deada54077fae87e2873f5ad9ef77e3ab7af5e1"
"bsnes-mercury-balanced|0f35d044bf2f2b879018a0500e676447e93a1db1"
"beetle-psx|d6383bff89a93e02aad10a586e804829861c3de1"
)

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
# 2. Toolchain fingerprint (section 2.1). Reported, not asserted. It corroborates
#    compatibility with the documented libretro CI toolchain. It does not identify
#    or authenticate the producing CI job, and is not a source revision.
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
# 3. Dolphin: instruction-level recovery of the full 40-character SCM_REV_STR.
#
#    Dolphin's build runs `git rev-parse HEAD` into DOLPHIN_WC_REVISION, scmrev.h.in emits it as
#    SCM_REV_STR, and Version.cpp exposes it through GetScmRevGitStr() as a static std::string.
#    The compiler materialises that 40-character string in pieces: 32 characters are copied from
#    .rodata by two 16-byte SSE moves and the final 8 are written from an inline `movabs`
#    immediate, so no contiguous 40-character run exists in the file.
#
#    A search for the two fragments anywhere in the binary would NOT prove the document's claim:
#    two unrelated occurrences would satisfy it. This check therefore works from the disassembly.
#    It locates every 41-byte std::string character-buffer construction site (`operator new(41)`),
#    follows the operands of that site, reads the .rodata bytes the site's own loads name, decodes
#    the site's own movabs immediate, concatenates them in store order, and only then compares the
#    result with the expected revision. The two fragments are bound to one construction site by
#    construction; they are never searched for independently.
#
#    The proof-critical chain is: authenticated binary identity; one isolated 41-byte construction
#    site; bytes 0..31 from that site's own .rodata operands; bytes 32..39 from that site's own
#    movabs immediate; the reconstructed value equalling the expected revision exactly; the NUL
#    terminator at offset 40 through the same character-buffer base register; and the scm_rev
#    literal context around the .rodata region. The std::string LENGTH field is deliberately NOT
#    in that chain: it is separate state from the character buffer, and this tool does not bind
#    a length store to this construction. It is reported as corroboration only.
# ---------------------------------------------------------------------------
echo
echo "3. Dolphin embedded source revision (section 3.1) — instruction-level"
echo

dolphin_path=""
for row in "${EXPECTED[@]}"; do
  IFS='|' read -r component relpath _ _ <<< "$row"
  [ "$component" = "dolphin" ] && dolphin_path="$CORES_DIR/$relpath"
done

if [ ! -f "$dolphin_path" ]; then
  fail "dolphin binary missing; cannot verify the construction site"
elif ! command -v objdump >/dev/null 2>&1; then
  fail "objdump not available; cannot verify the construction site"
else
  scm=$(python3 - "$dolphin_path" "$DOLPHIN_REVISION" "$DOLPHIN_DESCRIBE" <<'PY'
import re, struct, subprocess, sys

path, expected, describe = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path, 'rb') as f:
    BUF = f.read()

# --- minimal ELF section table (address <-> file offset) --------------------
def sections():
    if BUF[:4] != b'\x7fELF' or BUF[4] != 2:
        raise SystemExit('not a 64-bit ELF')
    e_shoff, = struct.unpack_from('<Q', BUF, 0x28)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from('<HHH', BUF, 0x3a)
    def sh(i):
        o = e_shoff + i * e_shentsize
        name, _typ, _flags, addr, off, size = struct.unpack_from('<IIQQQQ', BUF, o)
        return name, addr, off, size
    stroff = sh(e_shstrndx)[2]
    out = {}
    for i in range(e_shnum):
        name, addr, off, size = sh(i)
        end = BUF.index(b'\0', stroff + name)
        out[BUF[stroff + name:end].decode()] = (addr, off, size)
    return out

SEC = sections()
if '.text' not in SEC or '.rodata' not in SEC:
    raise SystemExit('missing .text/.rodata')
TEXT_A, TEXT_O, TEXT_S = SEC['.text']

def read_ro(addr, n):
    """Read n bytes at a virtual address, only from read-only constant sections."""
    for name in ('.rodata', '.data.rel.ro', '.rodata.cst16'):
        if name not in SEC:
            continue
        a, off, size = SEC[name]
        if a <= addr and addr + n <= a + size:
            return BUF[off + addr - a: off + addr - a + n], name
    return None, None

# --- disassembly of a single window ----------------------------------------
LINE = re.compile(r'^\s+([0-9a-f]+):\t[0-9a-f ]+\t(\S+)\s*(.*?)\s*$')
MEM = re.compile(r'(?:(0x[0-9a-f]+))?\((%r[a-z0-9]+)\)$')
RIP = re.compile(r'^\s*([0-9a-f]+)')

def disasm(lo, hi):
    out = subprocess.run(
        ['objdump', '-d', '--start-address=0x%x' % lo, '--stop-address=0x%x' % hi, path],
        capture_output=True, text=True).stdout
    return [(int(m.group(1), 16), m.group(2), m.group(3))
            for m in (LINE.match(l) for l in out.splitlines()) if m]

def store_target(ops):
    """'%xmm0,0x10(%rax)' -> (0x10, '%rax'); None if the destination is not [base+disp]."""
    if ',' not in ops:
        return None
    m = MEM.match(ops.rsplit(',', 1)[1].strip())
    if not m:
        return None
    return (int(m.group(1), 16) if m.group(1) else 0), m.group(2)

# --- candidate sites: `mov $0x29,%edi` == operator new(41) ------------------
text = BUF[TEXT_O:TEXT_O + TEXT_S]
candidates = [TEXT_A + m.start() for m in re.finditer(rb'\xbf\x29\x00\x00\x00', text)]

sites = []
for site in candidates:
    xmm, imm, stores, bases = {}, {}, {}, set()
    alloc = nul40 = len_imm_seen = False
    for _addr, mnem, raw in disasm(site, min(TEXT_A + TEXT_S, site + 0x100)):
        comment = raw.split('#', 1)[1] if '#' in raw else ''
        ops = raw.split('#', 1)[0].strip()

        if mnem == 'call' and '<_Znwm' in ops:          # operator new(unsigned long)
            alloc = True
        elif mnem in ('movdqa', 'movdqu', 'movaps', 'movups') and '(%rip)' in ops and ',%xmm' in ops:
            m = RIP.match(comment)                       # rip-relative load from .rodata
            if m:
                xmm[ops.rsplit(',', 1)[1].strip()] = int(m.group(1), 16)
        elif mnem in ('movdqa', 'movdqu', 'movaps', 'movups') and ops.startswith('%xmm'):
            src, tgt = ops.split(',', 1)[0], store_target(ops)
            if tgt and src in xmm:                       # 16-byte store into the new buffer
                stores[tgt[0]] = ('rodata', xmm[src])
                bases.add(tgt[1])
        elif mnem == 'movabs' and ops.startswith('$0x'):
            v, reg = ops.split(',', 1)
            imm[reg.strip()] = int(v[1:], 16)
        elif mnem == 'mov' and ops.startswith('%r'):
            src, tgt = ops.split(',', 1)[0], store_target(ops)
            if tgt and src in imm:                       # 8-byte immediate store
                stores[tgt[0]] = ('imm', imm[src])
                bases.add(tgt[1])
        elif ops.startswith('$0x0,'):
            tgt = store_target(ops)
            if tgt and tgt[0] == 40:                     # NUL terminator at offset 40
                nul40 = True
                bases.add(tgt[1])
        elif ops.startswith('$0x28,'):
            # A $0x28 immediate store appears in the window. Dolphin's std::string length field
            # is 40, so this is consistent with the construction — but the length lives in the
            # string OBJECT, not in the character buffer, and this check does not bind the store
            # to this construction: any $0x28 immediate in the window sets it. It is therefore
            # reported as CORROBORATIVE ONLY and is not part of the proof-critical chain.
            len_imm_seen = True

    # The site must write bytes 0..15 and 16..31 from .rodata and 32..39 from an immediate,
    # and NUL-terminate at offset 40, all through the same CHARACTER-BUFFER base register,
    # into one 41-byte allocation. The length field is separate state and is not asserted.
    if not alloc or len(bases) != 1:
        continue
    if {0, 16, 32} - set(stores):
        continue
    if (stores[0][0], stores[16][0], stores[32][0]) != ('rodata', 'rodata', 'imm'):
        continue

    lo16, sec_lo = read_ro(stores[0][1], 16)
    hi16, sec_hi = read_ro(stores[16][1], 16)
    if lo16 is None or hi16 is None:
        continue
    sites.append(dict(site=site, base=bases.pop(),
                      a1=stores[0][1], a2=stores[16][1], sec=(sec_lo, sec_hi),
                      imm=stores[32][1],
                      value=lo16 + hi16 + stores[32][1].to_bytes(8, 'little'),
                      nul40=nul40, len_imm_seen=len_imm_seen))

# Only sites assembling a 40-character lowercase hex string are revision-shaped.
hexish = [s for s in sites if re.fullmatch(rb'[0-9a-f]{40}', s['value'])]

print('new41_sites=%d' % len(candidates))
print('string41_sites=%d' % len(sites))
print('revision_sites=%d' % len(hexish))

if len(hexish) != 1:
    print('status=ambiguous')
    for s in hexish:
        print('site=0x%x value=%s' % (s['site'], s['value'].decode()))
    raise SystemExit(0)

s = hexish[0]
value = s['value'].decode()
print('status=resolved')
print('site=0x%x' % s['site'])
print('base=%s' % s['base'])
print('rodata_lo=0x%x' % s['a1'])
print('rodata_hi=0x%x' % s['a2'])
print('rodata_sections=%s' % ','.join(s['sec']))
print('chars_0_31=%s' % s['value'][:32].decode())
print('movabs_imm=0x%016x' % s['imm'])
print('chars_32_39=%s' % s['value'][32:].decode())
print('assembled=%s' % value)
print('matches_expected=%s' % ('yes' if value == expected else 'no'))
print('alloc41=yes')
print('nul_at_40=%s' % ('yes' if s['nul40'] else 'no'))
print('length_0x28_seen=%s' % ('yes' if s['len_imm_seen'] else 'no'))

# Context: the 32 .rodata characters must sit inside Dolphin's scm_rev literal pool, i.e. be
# preceded by the "Dolphin [HEAD] " revision-string literal and followed by SCM_DESC_STR
# ("Dolphin/" + git describe). This binds the region to scm_rev rather than to any 32 bytes.
lo_off = SEC['.rodata'][1] + s['a1'] - SEC['.rodata'][0]
before = BUF[max(0, lo_off - 64):lo_off]
after = BUF[lo_off + 32:lo_off + 32 + 64]
ctx_before = b'Dolphin [HEAD] ' in before
ctx_after = after.startswith(b'Dolphin/' + describe.encode())
print('ctx_revstr_before=%s' % ('yes' if ctx_before else 'no'))
print('ctx_descstr_after=%s' % ('yes' if ctx_after else 'no'))
PY
)
  field() { printf '%s\n' "$scm" | sed -n "s/^$1=//p"; }

  note "41-byte allocation sites examined: $(field new41_sites)"
  note "41-byte std::string construction sites matched: $(field string41_sites)"
  note "of which assemble a 40-char lowercase hex value: $(field revision_sites)"

  if [ "$(field status)" != "resolved" ]; then
    fail "could not isolate a single SCM revision construction site (instruction-level association not established)"
  else
    note "construction site: $(field site) (base $(field base))"
    note "  operator new(41) → allocation of 40 characters + NUL"
    note "  chars 0..31  from $(field rodata_sections) at $(field rodata_lo) / $(field rodata_hi)"
    note "               = $(field chars_0_31)"
    note "  chars 32..39 from movabs immediate $(field movabs_imm) (little-endian)"
    note "               = $(field chars_32_39)"
    note "  assembled    = $(field assembled)"

    if [ "$(field matches_expected)" = "yes" ]; then
      pass "dolphin SCM_REV_STR reassembled from one construction site = $DOLPHIN_REVISION"
    else
      fail "reassembled SCM_REV_STR is $(field assembled), expected $DOLPHIN_REVISION"
    fi

    [ "$(field nul_at_40)" = "yes" ] \
      && pass "same site NUL-terminates at offset 40" \
      || fail "same site does not NUL-terminate at offset 40"

    # CORROBORATIVE, not asserted. The std::string length field is separate state from the
    # character buffer, and this check does not bind the store to this construction.
    note "corroborative only (not asserted): \$0x28 length-shaped immediate store in window: $(field length_0x28_seen)"
    note "  the length field belongs to the string object, not to the 41-byte character buffer;"
    note "  section 3.1 records it from manual disassembly, and it is not part of the proof chain"

    if [ "$(field ctx_revstr_before)" = "yes" ] && [ "$(field ctx_descstr_after)" = "yes" ]; then
      pass "the .rodata region lies in Dolphin's scm_rev literal pool ('Dolphin [HEAD] ' before, 'Dolphin/$DOLPHIN_DESCRIBE' after)"
    else
      fail "the .rodata region is not bounded by Dolphin's scm_rev literals"
    fi
  fi

  # The remaining scm_rev constants are stored as (pointer, length) pairs, so their boundaries are
  # read from the data rather than guessed from `strings` output.
  consts=$(python3 - "$dolphin_path" "$DOLPHIN_DESCRIBE" <<'PY'
import sys
data = open(sys.argv[1], 'rb').read()
describe = sys.argv[2].encode()
found = [lit.decode() for lit in (describe, b'HEAD', b'None', describe + b' Lin')
         if b'\x00' + lit + b'\x00' in data]
print(','.join(found))
print('yes' if b'Dolphin [HEAD] ' in data else 'no')
PY
)
  note "dolphin scm constants found: $(printf '%s\n' "$consts" | sed -n 1p)"
  if [ "$(printf '%s\n' "$consts" | sed -n 2p)" = "yes" ]; then
    pass "dolphin embeds the 'Dolphin [HEAD] ' scm_rev string"
  else
    fail "dolphin does NOT embed the 'Dolphin [HEAD] ' scm_rev string"
  fi

  note "NOTE: this proves the TOP-LEVEL revision only. The 33 gitlink pins recorded in"
  note "      dolphin-submodules-fd1aca3a.txt are determined by that commit but are NOT"
  note "      recoverable from the binary, and complete corresponding-source materialisation"
  note "      and archiving remains OPEN."
fi

# ---------------------------------------------------------------------------
# 4. The other three cores.
#
#    No universal negative is asserted and none is provable here: an inspection that finds no
#    revision does not establish that none is embedded. Two things are checked, and they are the
#    two the document actually claims:
#      (a) the documented public-CI candidate revision does not occur in the binary as an embedded
#          revision identifier — a narrow, falsifiable negative;
#      (b) a generic 40-character hex scan, reported as DIAGNOSTIC output only. Its results
#          establish nothing either way; they exist so a reader can see what an untargeted scan
#          turns up and why section 3.2 rejected each hit.
# ---------------------------------------------------------------------------
echo
echo "4. Other three cores — candidate absence check, plus diagnostic scan (section 3.2)"
echo

for row in "${CANDIDATES[@]}"; do
  IFS='|' read -r component candidate <<< "$row"
  relpath=""
  for erow in "${EXPECTED[@]}"; do
    IFS='|' read -r ec erp _ _ <<< "$erow"
    [ "$ec" = "$component" ] && relpath="$erp"
  done
  path="$CORES_DIR/$relpath"
  [ -f "$path" ] || { fail "$component: missing at $relpath"; continue; }

  out=$(python3 - "$path" "$candidate" <<'PY'
import re, sys
data = open(sys.argv[1], 'rb').read()
cand = sys.argv[2].encode()

# (a) the candidate revision, in any of the forms an embedded revision identifier takes.
forms = {
    'full': cand,
    'describe7': b'\x00' + cand[:7] + b'\x00',
    'describe8': b'\x00' + cand[:8] + b'\x00',
    'gdescribe': b'g' + cand[:7],
}
print('candidate_hits=' + (','.join(k for k, v in forms.items() if v in data) or 'none'))

# (b) diagnostic only — an untargeted 40-char hex scan. Not a provenance test.
runs = sorted({m.group().decode() for m in re.finditer(rb'[0-9a-f]{40}', data)})
print('hex40_runs=%d' % len(runs))
for s in runs[:8]:
    print('hex40=%s transitions=%d' % (s, sum(1 for a, b in zip(s, s[1:]) if a != b)))
PY
)
  hits=$(printf '%s\n' "$out" | sed -n 's/^candidate_hits=//p')
  runs=$(printf '%s\n' "$out" | sed -n 's/^hex40_runs=//p')

  if [ "$hits" = "none" ]; then
    pass "$component: candidate $candidate is NOT embedded as a revision identifier"
  else
    fail "$component: candidate revision matched in the binary as: $hits — re-open section 3.2"
  fi

  note "  DIAGNOSTIC (no conclusion drawn): $runs distinct 40-char hex run(s) present"
  printf '%s\n' "$out" | sed -n 's/^hex40=/    /p' | while read -r line; do note "  $line"; done
  note "  These runs are reported, not judged. Section 3.2 inspects and disposes of them by hand;"
  note "  their presence or absence proves nothing about whether a revision is embedded."
  note "  $component's exact source revision remains UNKNOWN."
done

# ---------------------------------------------------------------------------
echo
if [ "$FAILURES" -eq 0 ]; then
  echo "All checks passed."
  echo
  echo "Reminder — what this script does and does not establish:"
  echo
  echo "  Top-level revision provenance   dolphin: CLOSED (proven above)."
  echo "                                  nestopia, bsnes-mercury, beetle-psx: OPEN — candidate"
  echo "                                  identified, awaiting libretro confirmation. A candidate"
  echo "                                  is not corresponding source, and does not become one by"
  echo "                                  staying unchanged over time."
  echo "  Corresponding-source            OPEN for ALL FOUR cores, dolphin included. Dolphin's 33"
  echo "  materialisation and archive     gitlink pins are determined by the proven commit but are"
  echo "                                  not verifiable from the binary, and the checkout has not"
  echo "                                  been materialised, archived, or accompanied by notices."
  echo "  Public redistribution           OPEN / BLOCKED for every system. Nothing here authorises"
  echo "                                  distribution, and no legal-compliance claim is made."
  echo
  exit 0
fi

echo "$FAILURES check(s) failed."
echo
exit 1
