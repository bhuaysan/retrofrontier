import { describe, expect, it } from 'vitest';

import type { GameMetadataState } from '../../platform/ipc';
import { getMetadataAction, hasSelectableCandidates, metadataStateCopy } from './metadataActions';

function metadataState(
  status: GameMetadataState['status'],
  jobs: GameMetadataState['jobs'] = [],
  overrides: Partial<GameMetadataState> = {},
): GameMetadataState {
  return {
    gameId: 7,
    providerId: 'screenScraper',
    status,
    matchType: null,
    deterministic: false,
    providerGameId: null,
    providerRomId: null,
    unsupportedReason: null,
    lastFailure: null,
    lastCheckedAt: null,
    metadata: null,
    cover: null,
    candidates: [],
    userSelection: null,
    jobs,
    ...overrides,
  };
}

const candidates: GameMetadataState['candidates'] = [
  { providerGameId: 'candidate-a', title: 'Candidate A', releaseDate: null },
  { providerGameId: 'candidate-b', title: 'Candidate B', releaseDate: '1994-01-01' },
];

describe('metadata action projection', () => {
  it.each([
    ['pending', 'request', 'REQUEST METADATA'],
    ['matched', 'refresh', 'REFRESH METADATA'],
    ['stale', 'refresh', 'REVALIDATE METADATA'],
    ['noMatch', 'request', 'TRY METADATA AGAIN'],
    ['ambiguous', 'request', 'SEARCH AGAIN'],
    ['deferred', 'request', 'TRY METADATA AGAIN'],
    ['failed', 'request', 'RETRY METADATA'],
  ] as const)('maps %s to the supported %s command', (status, kind, label) => {
    const action = getMetadataAction(metadataState(status));

    expect(action).toEqual({
      kind,
      label,
      pendingLabel: kind === 'refresh' ? 'UPDATING METADATA' : 'REQUESTING METADATA',
    });
  });

  it('suppresses a second metadata action while a live backend job exists', () => {
    const action = getMetadataAction(
      metadataState('matched', [
        {
          id: 3,
          gameId: 7,
          providerId: 'screenScraper',
          kind: 'refreshMetadata',
          state: 'running',
          priority: 200,
          attempts: 1,
          lastFailure: null,
          earliestNextAttemptAt: null,
          claimedAt: 100,
          createdAt: 100,
          updatedAt: 100,
        },
      ]),
    );

    expect(action).toBeNull();
  });

  it('returns no action when the metadata read has not produced a state', () => {
    expect(getMetadataAction(null)).toBeNull();
  });

  it.each(['deferred', 'failed'] as const)(
    'makes manual candidates the recovery path for %s instead of another provider request',
    (status) => {
      expect(getMetadataAction(metadataState(status, [], { candidates }))).toBeNull();
    },
  );

  it.each([
    'systemNotMapped',
    'chdRepresentationUndefined',
    'cueBinRepresentationUndefined',
    'gdiRepresentationUndefined',
    'playlistIsNotIdentity',
    'containerRepresentationUndefined',
    'missingContentEvidence',
    'noPrimaryContentFile',
  ] as const)(
    'does not offer automatic retry for capability-gated deferred state: %s',
    (reason) => {
      expect(
        getMetadataAction(metadataState('deferred', [], { unsupportedReason: reason })),
      ).toBeNull();
    },
  );

  it('keeps the provider retry available for a deferred state without candidates or a capability gate', () => {
    expect(getMetadataAction(metadataState('deferred'))).toEqual({
      kind: 'request',
      label: 'TRY METADATA AGAIN',
      pendingLabel: 'REQUESTING METADATA',
    });
  });

  it('describes a capability-gated deferred state as manual resolution', () => {
    expect(metadataStateCopy('deferred', 'chdRepresentationUndefined')).toEqual({
      label: 'METADATA DEFERRED',
      description:
        'Automatic identification is not available for this content. Choose a provider candidate below when one is available.',
    });
  });
});

describe('metadata candidate presentation decision', () => {
  it.each(['ambiguous', 'deferred', 'failed', 'stale'] as const)(
    'treats persisted candidates as selectable for %s state',
    (status) => {
      expect(hasSelectableCandidates(metadataState(status, [], { candidates }))).toBe(true);
    },
  );

  it.each(['deferred', 'failed'] as const)('rejects an empty %s candidate set', (status) => {
    expect(hasSelectableCandidates(metadataState(status))).toBe(false);
  });

  it('does not treat historical candidates on an accepted match as selectable', () => {
    expect(hasSelectableCandidates(metadataState('matched', [], { candidates }))).toBe(false);
  });
});

describe('metadata state copy', () => {
  it('keeps all provider states explicit and local-first', () => {
    expect(metadataStateCopy('pending')).toEqual({
      label: 'METADATA PENDING',
      description:
        'No provider result is attached yet. Request enrichment when you want to check this game.',
    });
    expect(metadataStateCopy('matched')).toEqual({
      label: 'METADATA MATCHED',
      description: 'Normalized metadata is associated with this local game.',
    });
    expect(metadataStateCopy('noMatch')).toEqual({
      label: 'NO METADATA MATCH',
      description: 'No provider match is available. The local game remains usable.',
    });
    expect(metadataStateCopy('ambiguous')).toEqual({
      label: 'MATCH REVIEW NEEDED',
      description:
        'Choose a provider candidate below, or search again without changing local content.',
    });
    expect(metadataStateCopy('deferred')).toEqual({
      label: 'METADATA DEFERRED',
      description: 'Provider work is deferred. The local library and cached data remain usable.',
    });
    expect(metadataStateCopy('failed')).toEqual({
      label: 'METADATA UNAVAILABLE',
      description: 'Metadata could not be enriched. The local game remains usable.',
    });
    expect(metadataStateCopy('stale')).toEqual({
      label: 'METADATA STALE',
      description: 'Showing the last-known-good metadata while it is revalidated.',
    });
  });
});
