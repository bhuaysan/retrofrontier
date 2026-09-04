import { describe, expect, it } from 'vitest';

import type { SaveStateLoadability, SaveStateView } from '../../platform/ipc';
import {
  coreIdentityLabel,
  loadabilityHint,
  loadabilityLabel,
  saveStateTimeLabel,
  slotLabel,
} from './saveStateCopy';

const LOADABILITIES: SaveStateLoadability[] = ['ready', 'coreUnavailable', 'temporarilyBlocked'];

function view(overrides: Partial<SaveStateView> = {}): SaveStateView {
  return {
    id: 1,
    gameId: 7,
    contentUnitId: 11,
    slot: 3,
    coreId: 'beetle_psx',
    coreDisplayVersion: '0.9.44',
    coreSourceRevision: null,
    contentUnitLabel: null,
    createdAt: 0,
    updatedAt: 0,
    thumbnailRef: null,
    capabilities: { loadability: 'ready', deletable: true },
    ...overrides,
  };
}

describe('save-state copy', () => {
  it('labels the three loadability values', () => {
    expect(loadabilityLabel('ready')).toBe('READY TO LOAD');
    expect(loadabilityLabel('coreUnavailable')).toBe('REQUIRED CORE UNAVAILABLE');
    expect(loadabilityLabel('temporarilyBlocked')).toBe('TEMPORARILY UNAVAILABLE');
  });

  it('names the missing core rather than blaming the state', () => {
    const hint = loadabilityHint('coreUnavailable');
    expect(hint).toMatch(/core/i);
    expect(hint).not.toMatch(/broken|damaged|corrupt|invalid/i);
  });

  it('says that nothing is wrong with a temporarily blocked state', () => {
    const hint = loadabilityHint('temporarilyBlocked');
    expect(hint).toMatch(/nothing is wrong with/i);
    expect(hint).not.toMatch(/broken|damaged|corrupt|invalid/i);
  });

  it('renders the slot as product copy', () => {
    expect(slotLabel(3)).toBe('SLOT 3');
    expect(slotLabel(1)).toBe('SLOT 1');
    expect(slotLabel(999)).toBe('SLOT 999');
  });

  it('renders a compact core identity from the recorded provenance', () => {
    expect(coreIdentityLabel(view())).toBe('BEETLE PSX · 0.9.44');
    // Two builds of one core are distinguished by their own recorded versions, never by a digest.
    expect(coreIdentityLabel(view({ coreDisplayVersion: '0.9.45' }))).toBe('BEETLE PSX · 0.9.45');
  });

  it('adds the source revision only when it is the sole differentiator', () => {
    // With a display version present the revision adds nothing a user can act on, so it stays out.
    expect(
      coreIdentityLabel(view({ coreDisplayVersion: '0.9.44', coreSourceRevision: 'a1b2c3d4e5f6' })),
    ).toBe('BEETLE PSX · 0.9.44');
    expect(
      coreIdentityLabel(view({ coreDisplayVersion: null, coreSourceRevision: 'a1b2c3d4e5f6' })),
    ).toBe('BEETLE PSX · a1b2c3d');
    expect(coreIdentityLabel(view({ coreDisplayVersion: null }))).toBe('BEETLE PSX');
  });

  it('renders a stable, locale-independent save time', () => {
    // Constructed from local calendar parts, so the assertion holds in any host time zone while
    // still pinning the format and its zero padding.
    const updatedAt = new Date(2026, 8, 3, 14, 32).getTime();
    expect(saveStateTimeLabel(updatedAt)).toBe('2026-09-03 14:32');
    expect(saveStateTimeLabel(new Date(2026, 0, 9, 4, 5).getTime())).toBe('2026-01-09 04:05');
  });

  it('never claims compatibility for any input', () => {
    const candidates: string[] = [
      ...LOADABILITIES.map(loadabilityLabel),
      ...LOADABILITIES.map(loadabilityHint),
      ...[0, 1, 3, 42, 999, 1000].map(slotLabel),
      ...[
        view(),
        view({ coreDisplayVersion: null }),
        view({ coreDisplayVersion: null, coreSourceRevision: 'deadbeefdeadbeef' }),
        view({ coreId: 'bsnes-mercury-balanced', coreDisplayVersion: null }),
      ].map(coreIdentityLabel),
      ...[0, 1_700_000_000_000, Date.now()].map(saveStateTimeLabel),
    ];

    for (const candidate of candidates) {
      // `loadable` is about permission. A save state is never described as compatible or
      // incompatible, because RetroFrontier cannot know whether it will deserialize.
      expect(candidate.toLocaleLowerCase()).not.toContain('compatible');
    }
  });

  it('exposes no digest-shaped value', () => {
    const label = coreIdentityLabel(
      view({
        coreDisplayVersion: null,
        coreSourceRevision: 'a'.repeat(64),
      }),
    );
    expect(label).not.toMatch(/[0-9a-f]{64}/i);
  });
});
