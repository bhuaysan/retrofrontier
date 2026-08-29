import type { GameMetadataState } from '../../platform/ipc';

export type MetadataAction = {
  kind: 'request' | 'refresh';
  label: string;
  pendingLabel: string;
};

export type MetadataStateCopy = {
  label: string;
  description: string;
};

const STATE_COPY: Record<GameMetadataState['status'], MetadataStateCopy> = {
  pending: {
    label: 'METADATA PENDING',
    description:
      'No provider result is attached yet. Request enrichment when you want to check this game.',
  },
  matched: {
    label: 'METADATA MATCHED',
    description: 'Normalized metadata is associated with this local game.',
  },
  noMatch: {
    label: 'NO METADATA MATCH',
    description: 'No provider match is available. The local game remains usable.',
  },
  ambiguous: {
    label: 'MATCH REVIEW NEEDED',
    description:
      'Choose a provider candidate below, or search again without changing local content.',
  },
  deferred: {
    label: 'METADATA DEFERRED',
    description: 'Provider work is deferred. The local library and cached data remain usable.',
  },
  failed: {
    label: 'METADATA UNAVAILABLE',
    description: 'Metadata could not be enriched. The local game remains usable.',
  },
  stale: {
    label: 'METADATA STALE',
    description: 'Showing the last-known-good metadata while it is revalidated.',
  },
};

const LIVE_JOB_STATES: ReadonlySet<GameMetadataState['jobs'][number]['state']> = new Set([
  'pending',
  'running',
  'deferred',
]);

const PERMANENT_METADATA_FAILURES: ReadonlySet<NonNullable<GameMetadataState['lastFailure']>> =
  new Set([
    'invalidRequest',
    'developerAuthenticationFailed',
    'userAuthenticationFailed',
    'clientRejected',
    'credentialsUnavailable',
  ]);

/**
 * Candidate rows are authoritative manual-resolution suggestions unless an accepted match is
 * current. The backend keeps suggestion rows when a match is persisted, so a matched state must
 * not surface those historical rows as another choice.
 */
export function hasSelectableCandidates(state: GameMetadataState | null): boolean {
  // M5 does not emit pending jobs with candidates, but keep this projection broad enough to
  // preserve any persisted, non-matched suggestions the authoritative DTO provides.
  return state !== null && state.status !== 'matched' && state.candidates.length > 0;
}

export function metadataStateCopy(
  status: GameMetadataState['status'],
  unsupportedReason: GameMetadataState['unsupportedReason'] = null,
  candidateCount = 0,
): MetadataStateCopy {
  if (status === 'deferred' && unsupportedReason === 'systemNotMapped') {
    return {
      label: 'METADATA DEFERRED',
      description:
        'Automatic identification is unavailable because this system is not mapped to the provider.',
    };
  }
  if (status === 'deferred' && unsupportedReason !== null && candidateCount > 0) {
    return {
      label: 'METADATA DEFERRED',
      description:
        'Automatic identification is not available for this content. Choose a provider candidate below.',
    };
  }
  if (status === 'deferred' && unsupportedReason !== null) {
    return {
      label: 'METADATA DEFERRED',
      description:
        'Automatic identification is unavailable for this content. Try again to search for provider candidates.',
    };
  }
  if (status === 'ambiguous' && candidateCount === 0) {
    return {
      label: 'MATCH REVIEW NEEDED',
      description:
        'No provider candidates are available. Search again without changing local content.',
    };
  }
  return STATE_COPY[status];
}

export function getMetadataAction(state: GameMetadataState | null): MetadataAction | null {
  if (!state || state.jobs.some((job) => LIVE_JOB_STATES.has(job.state))) {
    return null;
  }

  // An unmapped system cannot be changed by repeating the same automatic identification request.
  // Other deferred capability gates may still benefit from another heuristic search when the
  // backend persisted no candidates. Persisted candidates remain the meaningful manual path.
  if (
    state.unsupportedReason === 'systemNotMapped' ||
    (hasSelectableCandidates(state) &&
      (state.status === 'deferred' || state.status === 'failed')) ||
    (state.unsupportedReason !== null && state.status !== 'deferred') ||
    (state.status === 'failed' &&
      state.lastFailure !== null &&
      PERMANENT_METADATA_FAILURES.has(state.lastFailure))
  ) {
    return null;
  }

  switch (state.status) {
    case 'matched':
      return {
        kind: 'refresh',
        label: 'REFRESH METADATA',
        pendingLabel: 'UPDATING METADATA',
      };
    case 'stale':
      return {
        kind: 'refresh',
        label: 'REVALIDATE METADATA',
        pendingLabel: 'UPDATING METADATA',
      };
    case 'pending':
      return {
        kind: 'request',
        label: 'REQUEST METADATA',
        pendingLabel: 'REQUESTING METADATA',
      };
    case 'noMatch':
      return {
        kind: 'request',
        label: 'TRY METADATA AGAIN',
        pendingLabel: 'REQUESTING METADATA',
      };
    case 'ambiguous':
      return {
        kind: 'request',
        label: 'SEARCH AGAIN',
        pendingLabel: 'REQUESTING METADATA',
      };
    case 'deferred':
      return {
        kind: 'request',
        label: 'TRY METADATA AGAIN',
        pendingLabel: 'REQUESTING METADATA',
      };
    case 'failed':
      return {
        kind: 'request',
        label: 'RETRY METADATA',
        pendingLabel: 'REQUESTING METADATA',
      };
  }
}
