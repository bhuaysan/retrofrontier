import type {
  BiosRequirementStatus,
  GameAvailability,
  LibraryContentUnitSummary,
  ReadinessReason,
  RuntimeState,
  SystemStatus,
} from '../../platform/ipc';

export type ReadinessTone = 'ready' | 'missing' | 'unavailable' | 'unknown';

export type ReadinessRowId = 'localContent' | 'runtime' | 'core' | 'bios';

export interface ReadinessRow {
  id: ReadinessRowId;
  label: string;
  tone: ReadinessTone;
  status: string;
  detail: string;
}

export interface OverallReadiness {
  tone: ReadinessTone;
  label: string;
  detail: string;
}

const RUNTIME_STATE_LABELS: Record<RuntimeState, string> = {
  notInstalled: 'not installed',
  ready: 'ready',
  installing: 'installing',
  updating: 'updating',
  repairing: 'repairing',
  broken: 'unavailable',
  rollbackAvailable: 'ready with rollback available',
};

function runtimeRow(status: SystemStatus | null): ReadinessRow {
  if (!status) {
    return unknownRow('runtime', 'RUNTIME', 'The runtime status is not available.');
  }

  const unavailableReason = status.readiness.reasons.find(
    (reason) => reason.kind === 'runtimeUnavailable',
  );
  if (unavailableReason?.kind === 'runtimeUnavailable') {
    return {
      id: 'runtime',
      label: 'RUNTIME',
      tone: 'unavailable',
      status: 'UNAVAILABLE',
      detail: `Managed runtime is ${RUNTIME_STATE_LABELS[unavailableReason.state]}.`,
    };
  }

  const runtimeState = status.core.availability.runtimeState;
  return {
    id: 'runtime',
    label: 'RUNTIME',
    tone: 'ready',
    status: 'AVAILABLE',
    detail: `Managed runtime ${RUNTIME_STATE_LABELS[runtimeState]}.`,
  };
}

function coreRow(status: SystemStatus | null): ReadinessRow {
  if (!status) {
    return unknownRow('core', 'CORE', 'The system core policy is not available.');
  }

  if (status.core.policy.decision.kind !== 'resolved') {
    return unknownRow('core', 'CORE', 'The approved system core policy is unresolved.');
  }

  const defaultCoreId = status.core.policy.defaultCoreId;
  if (defaultCoreId === null || status.core.availability.defaultCoreAvailable === null) {
    return unknownRow('core', 'CORE', 'No verified default core is available for this system.');
  }

  if (status.core.availability.defaultCoreAvailable) {
    return {
      id: 'core',
      label: 'CORE',
      tone: 'ready',
      status: 'AVAILABLE',
      detail: `Approved default core: ${defaultCoreId}.`,
    };
  }

  return {
    id: 'core',
    label: 'CORE',
    tone: 'missing',
    status: 'MISSING',
    detail: `Approved default core unavailable: ${defaultCoreId}.`,
  };
}

function requirementDetail(requirement: BiosRequirementStatus | undefined, prefix: string) {
  const filenames = requirement?.expectedFilenames.join(' / ');
  return filenames ? `${prefix}: ${filenames}.` : `${prefix}.`;
}

function biosRow(status: SystemStatus | null): ReadinessRow {
  if (!status) {
    return unknownRow('bios', 'BIOS', 'The BIOS status is not available.');
  }

  if (status.bios.policy === 'notRequired') {
    return {
      id: 'bios',
      label: 'BIOS',
      tone: 'ready',
      status: 'NOT REQUIRED',
      detail: 'This system does not require a BIOS file.',
    };
  }

  if (status.bios.policy === 'optional') {
    const requirement = status.bios.requirements.find((item) => !item.required);
    if (requirement?.state === 'presentValid') {
      const filename = requirement.matchedFilename ?? requirement.expectedFilenames[0];
      return {
        id: 'bios',
        label: 'BIOS',
        tone: 'ready',
        status: 'AVAILABLE (OPTIONAL)',
        detail: filename ? `Optional BIOS present: ${filename}.` : 'Optional BIOS is present.',
      };
    }
    if (requirement?.state === 'presentInvalid') {
      return {
        id: 'bios',
        label: 'BIOS',
        tone: 'unavailable',
        status: 'INVALID (OPTIONAL)',
        detail: requirementDetail(requirement, 'Optional BIOS is invalid'),
      };
    }
    if (requirement?.state === 'notCoveredByCatalog') {
      return unknownRow(
        'bios',
        'BIOS',
        'The optional BIOS identity is not covered by the catalog.',
      );
    }
    return {
      id: 'bios',
      label: 'BIOS',
      tone: 'ready',
      status: 'OPTIONAL',
      detail: 'No BIOS is required for this system; an optional dump may be supplied.',
    };
  }

  if (status.bios.ready) {
    const requirement = status.bios.requirements.find((item) => item.required);
    const filename = requirement?.matchedFilename ?? requirement?.expectedFilenames[0];
    return {
      id: 'bios',
      label: 'BIOS',
      tone: 'ready',
      status: 'AVAILABLE',
      detail: filename ? `Required BIOS present: ${filename}.` : 'Required BIOS is present.',
    };
  }

  const requirement = status.bios.requirements.find(
    (item) => item.required && item.state !== 'presentValid',
  );
  if (requirement?.state === 'presentInvalid') {
    return {
      id: 'bios',
      label: 'BIOS',
      tone: 'unavailable',
      status: 'INVALID',
      detail: requirementDetail(requirement, 'Required BIOS is invalid'),
    };
  }
  if (requirement?.state === 'notCoveredByCatalog') {
    return unknownRow('bios', 'BIOS', 'The required BIOS identity is not covered by the catalog.');
  }

  return {
    id: 'bios',
    label: 'BIOS',
    tone: 'missing',
    status: 'MISSING',
    detail: requirementDetail(requirement, 'Required BIOS is missing'),
  };
}

// Game-level availability is true when at least one content unit is available, so it cannot stand
// in for "all local content is present". The per-unit availability the backend already normalized
// is the authoritative input for that distinction; this is presentation over M4 state, not a second
// readiness policy.
function hasIncompleteContent(contentUnits: readonly LibraryContentUnitSummary[]): boolean {
  return contentUnits.some((unit) => unit.availability !== 'available');
}

function localContentRow(
  availability: GameAvailability | null,
  contentUnits: readonly LibraryContentUnitSummary[],
): ReadinessRow {
  if (availability === 'available') {
    if (hasIncompleteContent(contentUnits)) {
      return {
        id: 'localContent',
        label: 'LOCAL CONTENT',
        tone: 'missing',
        status: 'PARTIALLY AVAILABLE',
        detail: 'Some of the associated local content is incomplete or missing.',
      };
    }
    return {
      id: 'localContent',
      label: 'LOCAL CONTENT',
      tone: 'ready',
      status: 'AVAILABLE',
      detail: 'The associated local content is available.',
    };
  }
  if (availability === 'unavailable') {
    return {
      id: 'localContent',
      label: 'LOCAL CONTENT',
      tone: 'missing',
      status: 'MISSING',
      detail: 'The associated local content is unavailable or incomplete.',
    };
  }
  return unknownRow(
    'localContent',
    'LOCAL CONTENT',
    'Local content availability is not available.',
  );
}

function unknownRow(id: ReadinessRowId, label: string, detail: string): ReadinessRow {
  return { id, label, tone: 'unknown', status: 'UNKNOWN', detail };
}

function checkingRow(id: ReadinessRowId, label: string): ReadinessRow {
  return {
    id,
    label,
    tone: 'unknown',
    status: 'CHECKING',
    detail: `Checking the current ${label.toLocaleLowerCase()} snapshot.`,
  };
}

export function getReadinessRows(
  availability: GameAvailability | null,
  status: SystemStatus | null,
  contentUnits: readonly LibraryContentUnitSummary[],
  loading = false,
): ReadinessRow[] {
  const rows = [
    localContentRow(availability, contentUnits),
    runtimeRow(status),
    coreRow(status),
    biosRow(status),
  ];
  if (!loading) return rows;
  return rows.map((row) => (row.id === 'localContent' ? row : checkingRow(row.id, row.label)));
}

function reasonDetail(reason: ReadinessReason): string {
  switch (reason.kind) {
    case 'corePolicyUnresolved':
      return 'The approved system core policy is unresolved.';
    case 'runtimeUnavailable':
      return `Managed runtime is ${RUNTIME_STATE_LABELS[reason.state]}.`;
    case 'missingCore':
      return 'The approved default core is not available.';
    case 'missingRequiredBios':
      return 'A required BIOS file is missing.';
    case 'invalidRequiredBios':
      return 'A required BIOS file is invalid.';
    case 'biosIdentityNotCovered':
      return 'The required BIOS identity is not covered by the catalog.';
  }
}

export function getOverallReadiness(
  availability: GameAvailability | null,
  status: SystemStatus | null,
  contentUnits: readonly LibraryContentUnitSummary[],
): OverallReadiness {
  if (availability === 'unavailable') {
    return {
      tone: 'missing',
      label: 'MISSING CONTENT',
      detail: 'The local content must be available before this game can be considered ready.',
    };
  }
  if (availability === 'available' && hasIncompleteContent(contentUnits)) {
    return {
      tone: 'missing',
      label: 'INCOMPLETE CONTENT',
      detail:
        'Some local content for this game is incomplete or missing. The remaining requirements are listed below.',
    };
  }
  if (availability === null || status === null) {
    return {
      tone: 'unknown',
      label: 'READINESS UNKNOWN',
      detail: 'RetroFrontier could not verify the requirements for this system.',
    };
  }
  if (status.readiness.ready) {
    return {
      tone: 'ready',
      label: 'EMULATION REQUIREMENTS SATISFIED',
      detail: 'Local content and the current system requirements are satisfied.',
    };
  }

  const reason = status.readiness.reasons[0];
  return {
    tone: reason?.kind === 'corePolicyUnresolved' ? 'unknown' : 'unavailable',
    label: reason ? 'REQUIREMENTS NOT SATISFIED' : 'READINESS UNKNOWN',
    detail: reason ? reasonDetail(reason) : 'RetroFrontier could not verify the requirements.',
  };
}
