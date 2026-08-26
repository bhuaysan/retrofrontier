import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

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

export type ContentRootKind = 'managed' | 'external';
export type ContentRootAvailability =
  'available' | 'partiallyAvailable' | 'unavailable' | 'disabled' | 'unsafe';

export interface ContentRoot {
  id: number;
  path: string;
  kind: ContentRootKind;
  enabled: boolean;
  systemHint: SystemId | null;
  availability: ContentRootAvailability;
  lastScanAt: number | null;
  lastSuccessfulScanAt: number | null;
  createdAt: number;
  updatedAt: number;
}

export type GameAvailability = 'available' | 'unavailable';
export type ContentUnitAvailability = 'available' | 'incomplete' | 'missing';
export type ContentFileAvailability = 'available' | 'unavailable' | 'missing';
export type ContentUnitKind = 'singleFile' | 'chd' | 'cueBin' | 'gdi' | 'm3u';
export type ContentFileRole =
  'standalone' | 'descriptor' | 'track' | 'playlist' | 'disc' | 'discDescriptor' | 'discTrack';

export interface Game {
  id: number;
  systemId: SystemId;
  localTitle: string;
  availability: GameAvailability;
  createdAt: number;
  updatedAt: number;
}

export interface ContentFile {
  id: number;
  rootId: number;
  relativePath: string;
  sizeBytes: number;
  modifiedAt: number;
  crc32: string | null;
  md5: string | null;
  sha1: string | null;
  availability: ContentFileAvailability;
}

export interface ContentFileMembership {
  ordinal: number;
  role: ContentFileRole;
  file: ContentFile;
}

export interface ContentUnit {
  id: number;
  gameId: number;
  rootId: number;
  systemId: SystemId;
  kind: ContentUnitKind;
  localTitle: string;
  primaryRelativePath: string;
  fingerprint: string | null;
  availability: ContentUnitAvailability;
  createdAt: number;
  updatedAt: number;
  files: ContentFileMembership[];
}

export interface GameSnapshot {
  game: Game;
  contentUnits: ContentUnit[];
}

export interface LibrarySnapshot {
  games: GameSnapshot[];
}

export type ScanIssueKind =
  | 'rootUnavailable'
  | 'unreadablePath'
  | 'unsafePath'
  | 'unrepresentablePath'
  | 'unsupportedSystem'
  | 'ambiguousSystem'
  | 'incompatibleSystemHint'
  | 'malformedCue'
  | 'malformedGdi'
  | 'malformedM3u'
  | 'unsafeDescriptorReference'
  | 'missingReferencedFile'
  | 'referenceCycle'
  | 'hashReadFailure'
  | 'duplicateContent'
  | 'ambiguousReconciliation'
  | 'overlappingContentRoot'
  | 'watcherFailure';

export interface ScanIssue {
  id: number | null;
  scanRunId: number | null;
  rootId: number | null;
  kind: ScanIssueKind;
  relativePath: string | null;
  relatedPath: string | null;
  detail: string | null;
  createdAt: number;
}

export type ScanPhase =
  'discovery' | 'relationshipResolution' | 'hashing' | 'reconciliation' | 'completed';
export type ScanRunState = 'running' | 'completed' | 'failed';

export interface ScanCounters {
  rootsDiscovered: number;
  rootsCompleted: number;
  filesDiscovered: number;
  filesProcessed: number;
  filesHashed: number;
  bytesHashed: number;
  issuesFound: number;
}

export interface ScanProgress {
  runId: number;
  phase: ScanPhase;
  counters: ScanCounters;
}

export interface ScanSummary {
  runId: number;
  state: ScanRunState;
  counters: ScanCounters;
  durationMs: number;
}

export interface ScanStatus {
  running: boolean;
  progress: ScanProgress | null;
  lastResult: ScanSummary | null;
}

export interface AddExternalContentRootRequest {
  path: string;
  systemHint?: SystemId | null;
}

export interface ContentRootRequest {
  rootId: number;
}

export interface SetContentRootEnabledRequest extends ContentRootRequest {
  enabled: boolean;
}

export const LIBRARY_SCAN_PROGRESS_EVENT = 'library-scan-progress';
export const LIBRARY_SCAN_COMPLETED_EVENT = 'library-scan-completed';

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

export async function getContentRoots(): Promise<ContentRoot[]> {
  try {
    return await invoke<ContentRoot[]>('get_content_roots');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function addExternalContentRoot(
  request: AddExternalContentRootRequest,
): Promise<ContentRoot> {
  try {
    return await invoke<ContentRoot>('add_external_content_root', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function removeExternalContentRoot(request: ContentRootRequest): Promise<void> {
  try {
    await invoke('remove_external_content_root', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function setContentRootEnabled(
  request: SetContentRootEnabledRequest,
): Promise<ContentRoot> {
  try {
    return await invoke<ContentRoot>('set_content_root_enabled', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function rescanLibrary(): Promise<ScanSummary> {
  try {
    return await invoke<ScanSummary>('rescan_library');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getScanStatus(): Promise<ScanStatus> {
  try {
    return await invoke<ScanStatus>('get_scan_status');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getScanIssues(): Promise<ScanIssue[]> {
  try {
    return await invoke<ScanIssue[]>('get_scan_issues');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getLibrarySnapshot(): Promise<LibrarySnapshot> {
  try {
    return await invoke<LibrarySnapshot>('get_library_snapshot');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function onLibraryScanProgress(
  handler: (progress: ScanProgress) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<ScanProgress>(LIBRARY_SCAN_PROGRESS_EVENT, (event) =>
      handler(event.payload),
    );
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function onLibraryScanCompleted(
  handler: (summary: ScanSummary) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<ScanSummary>(LIBRARY_SCAN_COMPLETED_EVENT, (event) =>
      handler(event.payload),
    );
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}
