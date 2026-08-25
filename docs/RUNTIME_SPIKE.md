# Managed RetroArch Runtime Spike

## Goal
Prove RetroFrontier can install and run its own isolated RetroArch runtime without bundling RetroArch in the installer and without depending on a local installation.

## Model
Implementation/research: **GPT Luna Max**

Final architecture/security review: **GPT Sol Max**

## Platforms
- Windows x86_64
- macOS arm64
- macOS x86_64
- Linux x86_64

## Questions

### Distribution
- What acceptable downloadable RetroArch artifact works per platform?
- Is it portable enough for app-managed install?
- How are support assets and cores obtained?
- What license/attribution obligations apply?

### Isolation
Verify:
- explicit executable path
- explicit RetroFrontier config
- no unrelated system config
- explicit core path
- explicit BIOS/system path
- explicit save/state paths

### Updates
Evaluate:
- version pinning
- manifest schema
- staging
- integrity
- authenticity
- safe extraction
- activation
- rollback
- interrupted recovery
- retention

### Runtime Behavior
Validate:
- executable starts
- a core loads
- video/audio initialize
- controller is visible
- save output goes to intended directory
- RetroFrontier can observe process exit

### Platform Findings
Pay attention to:
- macOS signing/quarantine/runtime restrictions
- Intel vs Apple Silicon
- Linux dependency portability
- Windows portable archive behavior

## Non-Goals
No final updater UI, every V1 core, final release signing, or final hosting infrastructure is required.

## Deliverable
Document:
1. tested source/artifact per platform
2. procedure
3. findings
4. blockers
5. recommended runtime layout
6. recommended update/rollback design
7. architecture changes
8. unresolved security questions
