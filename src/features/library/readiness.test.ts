import { describe, expect, it } from 'vitest';

import type { SystemStatus } from '../../platform/ipc';
import { getReadinessRows, getOverallReadiness } from './readiness';

function systemStatus(overrides: Partial<SystemStatus> = {}): SystemStatus {
  return {
    id: 'playstation',
    displayName: 'PlayStation',
    manufacturer: 'Sony',
    aliases: ['PS1'],
    supportedExtensions: ['.chd'],
    core: {
      policy: {
        defaultCoreId: 'pcsx_rearmed',
        approvedCoreIds: ['pcsx_rearmed'],
        decision: { kind: 'resolved' },
      },
      availability: {
        runtimeState: 'ready',
        availableCoreIds: ['pcsx_rearmed'],
        defaultCoreAvailable: true,
      },
    },
    bios: {
      policy: 'required',
      ready: true,
      requirements: [
        {
          requirementId: 'playstation-bios',
          systemId: 'playstation',
          required: true,
          state: 'presentValid',
          expectedFilenames: ['SCPH1001.BIN'],
          expectedSizeBytes: 524288,
          description: 'PlayStation BIOS',
          matchedFilename: 'SCPH1001.BIN',
          fileSizeBytes: 524288,
          sha256: 'must-not-render',
        },
      ],
    },
    readiness: { ready: true, reasons: [] },
    ...overrides,
  };
}

describe('readiness presentation projection', () => {
  it('keeps local content, runtime, core, and BIOS as separate rows', () => {
    const rows = getReadinessRows('available', systemStatus());

    expect(rows.map(({ id, tone }) => ({ id, tone }))).toEqual([
      { id: 'localContent', tone: 'ready' },
      { id: 'runtime', tone: 'ready' },
      { id: 'core', tone: 'ready' },
      { id: 'bios', tone: 'ready' },
    ]);
    expect(rows.find(({ id }) => id === 'bios')?.detail).toContain('SCPH1001.BIN');
    expect(rows.find(({ id }) => id === 'bios')?.detail).not.toContain('must-not-render');
  });

  it('prioritizes unavailable local content over an otherwise ready environment', () => {
    const rows = getReadinessRows('unavailable', systemStatus());
    const overall = getOverallReadiness('unavailable', systemStatus());

    expect(rows[0]).toMatchObject({ id: 'localContent', tone: 'missing', status: 'MISSING' });
    expect(overall).toMatchObject({ tone: 'missing', label: 'MISSING CONTENT' });
  });

  it('describes missing runtime, unresolved core policy, and missing BIOS without internal fields', () => {
    const status = systemStatus({
      core: {
        policy: {
          defaultCoreId: null,
          approvedCoreIds: [],
          decision: { kind: 'unresolved', researchItem: 'internal research note' },
        },
        availability: {
          runtimeState: 'notInstalled',
          availableCoreIds: [],
          defaultCoreAvailable: null,
        },
      },
      bios: {
        policy: 'required',
        ready: false,
        requirements: [
          {
            requirementId: 'playstation-bios',
            systemId: 'playstation',
            required: true,
            state: 'missing',
            expectedFilenames: ['SCPH1001.BIN'],
            expectedSizeBytes: 524288,
            description: 'PlayStation BIOS',
            matchedFilename: null,
            fileSizeBytes: null,
            sha256: null,
          },
        ],
      },
      readiness: {
        ready: false,
        reasons: [
          { kind: 'runtimeUnavailable', state: 'notInstalled' },
          { kind: 'corePolicyUnresolved', researchItem: 'internal research note' },
          { kind: 'missingRequiredBios', requirementId: 'playstation-bios' },
        ],
      },
    });
    const rows = getReadinessRows('available', status);
    const overall = getOverallReadiness('available', status);

    expect(rows.find(({ id }) => id === 'runtime')).toMatchObject({
      tone: 'unavailable',
      status: 'UNAVAILABLE',
    });
    expect(rows.find(({ id }) => id === 'core')).toMatchObject({
      tone: 'unknown',
      status: 'UNKNOWN',
    });
    expect(rows.find(({ id }) => id === 'core')?.detail).not.toContain('internal research note');
    expect(rows.find(({ id }) => id === 'bios')).toMatchObject({
      tone: 'missing',
      status: 'MISSING',
    });
    expect(overall).toMatchObject({ tone: 'unavailable', label: 'REQUIREMENTS NOT SATISFIED' });
  });

  it('returns an unknown projection when the system snapshot is unavailable', () => {
    const rows = getReadinessRows('available', null);
    const overall = getOverallReadiness('available', null);

    expect(rows.map(({ id, tone }) => ({ id, tone }))).toEqual([
      { id: 'localContent', tone: 'ready' },
      { id: 'runtime', tone: 'unknown' },
      { id: 'core', tone: 'unknown' },
      { id: 'bios', tone: 'unknown' },
    ]);
    expect(overall).toMatchObject({ tone: 'unknown', label: 'READINESS UNKNOWN' });
  });

  it('shows BIOS as not required when the catalog policy says no BIOS is needed', () => {
    const status = systemStatus({
      bios: { policy: 'notRequired', ready: true, requirements: [] },
    });

    expect(getReadinessRows('available', status).find(({ id }) => id === 'bios')).toMatchObject({
      tone: 'ready',
      status: 'NOT REQUIRED',
    });
  });

  it('does not describe an optional missing BIOS as required or available', () => {
    const status = systemStatus({
      bios: {
        policy: 'optional',
        ready: true,
        requirements: [
          {
            requirementId: 'optional-bios',
            systemId: 'playstation',
            required: false,
            state: 'optionalMissing',
            expectedFilenames: ['optional.bin'],
            expectedSizeBytes: null,
            description: 'Optional BIOS',
            matchedFilename: null,
            fileSizeBytes: null,
            sha256: null,
          },
        ],
      },
    });

    expect(getReadinessRows('available', status).find(({ id }) => id === 'bios')).toMatchObject({
      tone: 'ready',
      status: 'OPTIONAL',
    });
  });
});
