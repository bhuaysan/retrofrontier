import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
  appName: string;
  version: string;
  platform: string;
  architecture: string;
  databaseReady: boolean;
}

export type RuntimeState =
  | 'notInstalled'
  | 'ready'
  | 'installing'
  | 'updating'
  | 'repairing'
  | 'broken'
  | 'rollbackAvailable';

export interface RuntimeStatus {
  state: RuntimeState;
  installationId: string | null;
  releaseId: string | null;
  canRollback: boolean;
  repairRequired: boolean;
}

export type SystemId =
  | 'nes'
  | 'snes'
  | 'nintendo_64'
  | 'game_boy'
  | 'game_boy_color'
  | 'game_boy_advance'
  | 'mega_drive'
  | 'playstation'
  | 'sega_saturn'
  | 'sega_dreamcast'
  | 'nintendo_gamecube';

export type BiosPolicy = 'notRequired' | 'optional' | 'required';
export type BiosRootStatus = 'ready' | 'missing' | 'notDirectory' | 'unsafe';
export type BiosRequirementStatusState =
  'presentValid' | 'missing' | 'presentInvalid' | 'optionalMissing' | 'notCoveredByCatalog';

export interface BiosRequirementStatus {
  requirementId: string;
  systemId: SystemId;
  required: boolean;
  state: BiosRequirementStatusState;
  expectedFilenames: string[];
  expectedSizeBytes: number | null;
  description: string;
  matchedFilename: string | null;
  fileSizeBytes: number | null;
  sha256: string | null;
}

export interface BiosDiscovery {
  root: string;
  rootStatus: BiosRootStatus;
  requirements: BiosRequirementStatus[];
}

export interface SystemBiosStatus {
  policy: BiosPolicy;
  ready: boolean;
  requirements: BiosRequirementStatus[];
}

export type CorePolicyDecision =
  { kind: 'resolved' } | { kind: 'unresolved'; researchItem: string };

export interface CorePolicy {
  defaultCoreId: string | null;
  approvedCoreIds: string[];
  decision: CorePolicyDecision;
}

export interface CoreAvailabilityStatus {
  runtimeState: RuntimeState;
  availableCoreIds: string[];
  defaultCoreAvailable: boolean | null;
}

export interface SystemCoreStatus {
  policy: CorePolicy;
  availability: CoreAvailabilityStatus;
}

export type ReadinessReason =
  | { kind: 'corePolicyUnresolved'; researchItem: string }
  | { kind: 'runtimeUnavailable'; state: RuntimeState }
  | { kind: 'missingCore'; coreId: string }
  | { kind: 'missingRequiredBios'; requirementId: string }
  | { kind: 'invalidRequiredBios'; requirementId: string }
  | { kind: 'biosIdentityNotCovered'; requirementId: string };

export interface SystemReadiness {
  ready: boolean;
  reasons: ReadinessReason[];
}

export interface SystemStatus {
  id: SystemId;
  displayName: string;
  manufacturer: string;
  aliases: string[];
  supportedExtensions: string[];
  core: SystemCoreStatus;
  bios: SystemBiosStatus;
  readiness: SystemReadiness;
}

export interface SystemsResponse {
  runtime: RuntimeStatus;
  biosRoot: string;
  biosRootStatus: BiosRootStatus;
  systems: SystemStatus[];
}

export interface IpcErrorShape {
  code: string;
  message: string;
}

export class IpcError extends Error implements IpcErrorShape {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'IpcError';
    this.code = code;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function normalizeIpcError(error: unknown): IpcError {
  if (isRecord(error) && typeof error.code === 'string' && typeof error.message === 'string') {
    return new IpcError(error.code, error.message);
  }

  return new IpcError(
    'ipc_unavailable',
    'The native RetroFrontier foundation could not be reached. Start the Tauri application and try again.',
  );
}

export async function getAppInfo(): Promise<AppInfo> {
  try {
    return await invoke<AppInfo>('get_app_info');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getRuntimeStatus(): Promise<RuntimeStatus> {
  try {
    return await invoke<RuntimeStatus>('get_runtime_status');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getSystems(): Promise<SystemsResponse> {
  try {
    return await invoke<SystemsResponse>('get_systems');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getBiosStatus(rootOverride?: string): Promise<BiosDiscovery> {
  try {
    const args = rootOverride === undefined ? undefined : { request: { rootOverride } };
    return await invoke<BiosDiscovery>('get_bios_status', args);
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}
