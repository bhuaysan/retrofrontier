# V1 BIOS Matrix

Authoritative firmware policy for the eleven V1 systems. Closed by M10.2 alongside
[`docs/CORE_MATRIX.md`](CORE_MATRIX.md).

## Non-negotiable rules

1. **RetroFrontier never distributes BIOS or proprietary firmware.** No BIOS, ROM, firmware, signing
   key or credential is committed to this repository or downloaded by the application. BIOS files
   are user-owned data; RetroFrontier discovers and validates them and never downloads, executes or
   modifies them (DOMAIN — *BIOS File*).
2. **The frontend is never responsible for filesystem identity or verification.** BIOS discovery,
   hashing and validation live behind the Rust boundary. React receives a readiness verdict, never a
   path, a digest or a verification decision.
3. **A filename match without an authoritative identity is not a valid BIOS result.** It is reported
   as `notCoveredByCatalog` — never as valid (DOMAIN — *BIOS Requirement*).
4. **A BIOS policy is authoritative only when it comes from the approved core's own documentation.**
   A system with no approved core cannot have an authoritative BIOS policy, because "required by
   which core?" has no answer yet.

## Policy states

Every V1 system resolves to exactly one:

- **Not required** — no firmware is needed for the system's V1 content.
- **Required, identities authoritative** — firmware is mandatory and RetroFrontier knows exactly
  which dumps satisfy it.
- **Optional, effect documented** — firmware changes behaviour in a documented way; absence is not
  a failure.
- **Unresolved / release blocker** — the policy cannot be stated authoritatively yet.

## Matrix

| System | Policy state | Filenames | Authoritative identity | Evidence | Implemented in catalog |
|---|---|---|---|---|---|
| NES | **Not required** | — | n/a | Nestopia UE; `disksys.rom` is FDS-only and `.fds` is not a V1 extension | Yes — `BiosPolicy::NotRequired` |
| SNES | **Not required** (see §1) | — | n/a | bsnes-mercury; coprocessor firmware is title-specific | Yes — `NotRequired` |
| Nintendo 64 | **Not required** | — | n/a | Mupen64Plus-Next documents no BIOS | Yes — `NotRequired` |
| Game Boy | **Optional, effect documented** | `gb_bios.bin` | MD5 `32fbbd84168d3482956eb3c5051637f5` | mGBA + Gambatte core docs (agree) | No — catalog has no GB requirement |
| Game Boy Color | **Optional, effect documented** | `gbc_bios.bin` | MD5 `dbfce9db9deaa2567f6a84fde55f9680` | mGBA + Gambatte core docs (agree) | No |
| Game Boy Advance | **Optional, effect documented** | `gba_bios.bin` | MD5 `a860e8c0b6d573d191e4ec7db1b1e4f6` | mGBA core documentation | Partly — requirement exists, **identity still absent** |
| Mega Drive / Genesis | **Not required** | — | n/a | No approved core; no V1 Mega Drive content requires firmware | Yes — `NotRequired` |
| PlayStation | **Required, identities authoritative** | `scph5500.bin`, `scph5501.bin`, `scph5502.bin` | MD5, see §2 | Beetle PSX core documentation | **Yes — fully implemented** |
| Saturn | **Required, identities authoritative** (newly established) | `sega_101.bin`, `mpr-17933.bin` | MD5, see §3 | Beetle Saturn core documentation | Partly — requirement exists, **identities absent** |
| Dreamcast | **Unresolved — current catalog policy is wrong** | see §4 | — | Flycast core documentation contradicts the catalog | **No — catalog is incorrect** |
| GameCube | **Not required** | — | n/a | Dolphin; IPL deferred, see §5 | Yes — `NotRequired` |

Only PlayStation is both authoritative *and* fully implemented. The Game Boy family and Saturn rows
are now authoritative *as policy* but are deliberately not yet in the catalog, because their cores
are only Candidates — see §6.

## 1. SNES coprocessor firmware (deferred, unchanged)

bsnes-mercury documents optional coprocessor firmware (`dsp1*`, `dsp2*`, `dsp3*`, `dsp4*`,
`cx4.data.rom`, `st010*`, `st011*`, `st018*`, `sgb.boot.rom`) needed only by a small number of
enhancement-chip titles, and it ships HLE options for many of them.

Marking every SNES title BIOS-required would be false, so SNES stays **Not required**. Per-title
firmware detection needs cartridge-level identification RetroFrontier does not have. This remains
deliberately deferred and is **not** a V1 blocker.

M10.2 confirms the M7 reasoning and changes nothing here.

## 2. PlayStation (preserved unchanged)

M10.2 re-examined the PlayStation policy as instructed and found **no evidence requiring
correction**. It is preserved exactly.

| Filename | Description | MD5 |
|---|---|---|
| `scph5500.bin` | PS1 JP BIOS | `8dd7d5296a650fac7319bce665a6a53c` |
| `scph5501.bin` | PS1 US BIOS | `490f666e1afb15b7362b406ed1cea246` |
| `scph5502.bin` | PS1 EU BIOS | `32736f17079d0b2b7024407c39bd3050` |

Retained consequences:

- `scph1001.bin` stays excluded — the approved core does not look that filename up.
- Identities are MD5 because that is what the core publishes; recording invented SHA-256 values
  would be unverifiable. Discovery still reports the observed SHA-256.
- No expected size is asserted; the digest pins identity exactly.
- Region enforcement is deferred: any one of the three satisfies the requirement.
- Beetle PSX can fall back to a bundled OpenBIOS. RetroFrontier keeps PlayStation BIOS **required**,
  validates before spawn, and never enables the core's BIOS override.

## 3. Saturn (newly authoritative)

Beetle Saturn documents its firmware explicitly. These identities are authoritative *for that core*
and become catalog data only if Beetle Saturn is approved.

| Filename | Description | MD5 |
|---|---|---|
| `sega_101.bin` | Saturn JP BIOS | `85ec9ca47d8f6807718151cbcca8b964` |
| `mpr-17933.bin` | Saturn US/EU BIOS | `3240872c70984b6cbfda1586cab68dbe` |

Saturn BIOS is **required** — Beetle Saturn has no HLE fallback comparable to Beetle PSX's OpenBIOS.

The core additionally documents two **title-specific cartridge ROMs**, which are *not* system BIOS
and must not be modelled as a system BIOS requirement:

| Filename | Purpose | MD5 |
|---|---|---|
| `mpr-18811-mx.ic1` | *King of Fighters '95* cartridge | `255113ba943c92a54facd25a10fd780c` |
| `mpr-19367-mx.ic1` | *Ultraman* cartridge | `1cd19988d1d72a3e7caa0b73234c96b4` |

These are the Saturn analogue of the SNES coprocessor problem: they need per-title identification
RetroFrontier does not have, and they are selected through a core option. **Deferred**, exactly as
SNES is, and explicitly *not* a reason to mark Saturn titles BIOS-required beyond the system BIOS.

The current catalog lists the two system filenames with **no identities at all**, which under rule 3
means a real dump is reported `notCoveredByCatalog`. Adding these MD5 values is a precondition of
Saturn approval.

## 4. Dreamcast — the catalog is currently wrong

This is M10.2's most significant BIOS finding.

The catalog states Dreamcast BIOS is **Required**, naming `dc_boot.bin` and `dc_flash.bin` at the
top level of the system directory, with no identities. Flycast's own documentation states:

- `dc/dc_boot.bin` — "Dreamcast BIOS — **Optional**", MD5 `e10c53c2f8b90bab96ead2d368858623`;
- **`dc_flash.bin` is not listed at all** among the documented firmware files;
- "All bios files need to be in a directory named **`dc`**" in RetroArch's system directory.

So the shipped catalog is wrong in three independent ways: wrong requirement kind (Required vs
Optional), a file the core documentation does not list, and the wrong filesystem location.

Nothing user-visible breaks today, because Dreamcast approves no core and DOMAIN rule 15 makes it
unlaunchable regardless. But the entry is a **BIOS guess presented as policy**, which is exactly what
this milestone exists to eliminate.

M10.2 records it as **Unresolved**. It is not corrected in code because the correct values depend on
approving Flycast, which has not happened. On approval, the catalog must be changed to: Optional,
`dc_boot.bin` with MD5 `e10c53c2f8b90bab96ead2d368858623`, located under `dc/`.

**Architectural consequence.** The `dc/` subdirectory is a *core-required internal layout*.
RetroFrontier's BIOS discovery has no concept of one. This is the existing open backlog item *"map
user BIOS folders to any future core-required internal layout"*, and Dreamcast is the first system
that actually needs it. It is a hard precondition of Dreamcast approval, and the mapping must remain
behind the Rust boundary — the frontend must not learn about firmware paths (rule 2).

**Uncertainty recorded honestly.** Flycast is widely known to also consume a `dc_flash.bin` and to
be able to synthesise one, and "Optional" reflects Flycast's ability to direct-boot most GD-ROM
content without firmware. The documentation is the authoritative source for policy; the practical
behaviour of a real managed launch has **not** been measured by RetroFrontier. Dreamcast BIOS policy
must be re-verified against an actual qualified launch before approval, not settled from
documentation alone.

## 5. GameCube — optional IPL versus Dolphin `Sys` (unchanged)

Two different things that must not be conflated:

- **Dolphin `Sys` support data** is *not* BIOS. It is a managed, authenticated Runtime Release
  component (`dolphin-sys`), obtained from libretro's system-assets buildbot, never from a user's
  Dolphin installation, and linked into the composed system directory as `dolphin-emu/Sys`. It is
  required, and it is already implemented and shipped in Release 002.
- **The GameCube IPL** (the console boot ROM, which produces the boot animation) is genuinely
  optional user-owned firmware. Dolphin runs GameCube titles without it.

GameCube therefore stays **Not required**, and the IPL stays **deferred** — it is a cosmetic
enhancement, not a launch precondition. M10.2 changes nothing here.

## 6. Why authoritative rows are not yet in the catalog

The Game Boy family and Saturn now have authoritative identities, and they are still absent from
`SystemCatalog`. That is deliberate.

A BIOS requirement is a statement about *the approved core*. Writing mGBA's `gba_bios.bin` identity
into the catalog while the Game Boy Advance core policy is only a *Candidate* would assert a
core-specific fact about a system that approves no core — and if a different core were later
approved, the identity could be wrong.

The correct sequence is: approve the core, then adopt that core's documented identities in the same
change. M10.2 completes the research half so that the approval, when it happens, is a small,
evidence-backed change rather than a fresh investigation.

The one exception already in the catalog is the GBA `gba_bios.bin` *filename* with no identity,
inherited from M3. It is harmless under rule 3 (it can only ever report
`notCoveredByCatalog`), and MD5 `a860e8c0b6d573d191e4ec7db1b1e4f6` is now recorded here ready for
adoption.

## 7. Evidence

Primary sources, read 2026-09-04. Community BIOS lists were **not** used.

- `docs.libretro.com/library/beetle_saturn/` — Saturn system and cartridge firmware, MD5 values.
- `docs.libretro.com/library/flycast/` — Dreamcast firmware table, optional status, `dc/` layout.
- `docs.libretro.com/library/mgba/` — `gba_bios.bin`, `gb_bios.bin`, `gbc_bios.bin`, `sgb_bios.bin`
  MD5 values and the "Use BIOS file if found" core option.
- `docs.libretro.com/library/gambatte/` — `gb_bios.bin` / `gbc_bios.bin` MD5 values, independently
  agreeing with mGBA.
- `docs.libretro.com/library/mupen64plus/` — no BIOS section.
- Beetle PSX core documentation, as recorded in M7 and re-checked here.

No firmware file was downloaded, inspected, hashed or committed while producing this document.
