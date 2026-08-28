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

export function metadataStateCopy(status: GameMetadataState['status']): MetadataStateCopy {
  return STATE_COPY[status];
}

export function getMetadataAction(state: GameMetadataState | null): MetadataAction | null {
  if (!state || state.jobs.some((job) => LIVE_JOB_STATES.has(job.state))) {
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
