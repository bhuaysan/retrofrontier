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

/// Where the configured trusted managed-release source came from. A qualification repository is
/// never presented as if it were a public production release channel.
export type RuntimeSourceOrigin = 'production' | 'qualification';

export type RuntimeInstallErrorCode =
  | 'sourceNotConfigured'
  | 'installationInProgress'
  | 'gameRunning'
  | 'releaseNotTrusted'
  | 'downloadFailed'
  | 'verificationFailed'
  | 'extractionFailed'
  | 'storageLimit'
  | 'unsupportedPlatform'
  | 'installationFailed';

export interface RuntimeInstallFailure {
  code: RuntimeInstallErrorCode;
  message: string;
}

export interface RuntimeInstallResponse {
  installed: boolean;
  status: RuntimeStatus;
  error: RuntimeInstallFailure | null;
}

export interface RuntimeInstallState {
  status: RuntimeStatus;
  sourceConfigured: boolean;
  sourceOrigin: RuntimeSourceOrigin | null;
  releaseTarget: string | null;
  installing: boolean;
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

export type LibraryMetadataMatchState =
  'pending' | 'matched' | 'noMatch' | 'ambiguous' | 'deferred' | 'failed' | 'stale';

/** M6.1's bounded title ordering. Additional sort orders require an explicit product decision. */
export type LibrarySort = 'titleAsc';

export interface LibraryQueryRequest {
  search?: string | null;
  systemId?: SystemId | null;
  favoritesOnly?: boolean;
  genre?: string | null;
  region?: string | null;
  availability?: GameAvailability | null;
  /** Restricts the page to games whose provider match needs a human decision. */
  needsMetadataReview?: boolean;
  sort?: LibrarySort;
  limit?: number;
  offset?: number;
}

export interface LibraryListItem {
  gameId: number;
  systemId: SystemId;
  localTitle: string;
  metadataTitle: string | null;
  displayTitle: string;
  sortTitle: string;
  availability: GameAvailability;
  favorite: boolean;
  metadataMatchState: LibraryMetadataMatchState;
  releaseDate: string | null;
  genre: string | null;
  region: string | null;
  /** Opaque native media-protocol reference; never a filesystem path or provider URL. */
  coverRef: string | null;
}

export interface LibraryPage {
  items: LibraryListItem[];
  total: number;
  offset: number;
  limit: number;
}

/**
 * The All Systems browse request.
 *
 * There is deliberately no `systemId`: choosing a system is what leaves shelf mode for the
 * paginated grid. Only the filters the Library really owns appear here.
 */
export interface LibraryShelvesRequest {
  search?: string | null;
  favoritesOnly?: boolean;
  /** Restricts every shelf to games whose local content is in the given availability state. */
  availability?: GameAvailability | null;
  /** Restricts every shelf to games whose provider match needs a human decision. */
  needsMetadataReview?: boolean;
  /** Zero or absent means the bounded backend default; larger values are capped by the backend. */
  previewLimit?: number;
}

/**
 * One system's bounded shelf. `total` is every matching game of that system; `items` is only the
 * preview the shelf renders.
 */
export interface LibraryShelf {
  systemId: SystemId;
  total: number;
  items: LibraryListItem[];
}

/**
 * Shelves for every system with at least one match. Systems with none are absent.
 *
 * Backend order is deterministic but is *not* the catalog's presentation order; the frontend
 * applies that, because the catalog is the authority on how systems are presented.
 */
export interface LibraryShelves {
  shelves: LibraryShelf[];
}

export interface LibrarySystemCount {
  systemId: SystemId;
  gameCount: number;
}

export interface LibrarySummary {
  totalGames: number;
  favoriteGames: number;
  systems: LibrarySystemCount[];
}

export interface LibraryContentUnitSummary {
  unitId: number;
  rootId: number;
  kind: ContentUnitKind;
  localTitle: string;
  primaryRelativePath: string;
  fileCount: number;
  availability: ContentUnitAvailability;
}

export interface LibraryGameDetail {
  gameId: number;
  systemId: SystemId;
  localTitle: string;
  availability: GameAvailability;
  favorite: boolean;
  contentUnits: LibraryContentUnitSummary[];
}

export interface GameFavorite {
  gameId: number;
  favorite: boolean;
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

export interface ScanIssuePageRequest {
  offset?: number;
  limit?: number;
}

export interface ScanIssuePage {
  issues: ScanIssue[];
  scanRunId: number | null;
  total: number;
  offset: number;
  limit: number;
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

export interface LibraryGameRequest {
  gameId: number;
}

export interface SetGameFavoriteRequest extends LibraryGameRequest {
  favorite: boolean;
}

export type MetadataProviderId = 'screenScraper';

export type ProviderMatchStatus =
  'pending' | 'matched' | 'noMatch' | 'ambiguous' | 'deferred' | 'failed' | 'stale';

export type MatchType =
  'deterministicSha1' | 'deterministicMd5' | 'deterministicCrc32' | 'heuristicUserConfirmed';

export type UnsupportedContentReason =
  | 'systemNotMapped'
  | 'chdRepresentationUndefined'
  | 'cueBinRepresentationUndefined'
  | 'gdiRepresentationUndefined'
  | 'playlistIsNotIdentity'
  | 'containerRepresentationUndefined'
  | 'missingContentEvidence'
  | 'noPrimaryContentFile';

export type ProviderFailureClass =
  | 'invalidRequest'
  | 'providerRestricted'
  | 'developerAuthenticationFailed'
  | 'userAuthenticationFailed'
  | 'noMatch'
  | 'providerUnavailable'
  | 'clientRejected'
  | 'capacityDeferred'
  | 'dailyQuotaExceeded'
  | 'negativeQuotaExceeded'
  | 'transport'
  | 'transientServer'
  | 'malformedResponse'
  | 'credentialsUnavailable'
  | 'mediaUnavailable';

export type MediaAssetKind = 'cover';
export type MediaAssetState = 'cached' | 'missing' | 'failed';
export type MetadataJobKind = 'identify' | 'refreshMetadata' | 'refreshCover';
export type MetadataJobState = 'pending' | 'running' | 'deferred' | 'failed' | 'completed';
export type UserAccountState = 'notConfigured' | 'configured' | 'invalid' | 'vaultUnavailable';

/** Provider-independent metadata. Deliberately small; M6 renders from exactly these fields. */
export interface NormalizedMetadata {
  title: string;
  sortTitle: string | null;
  synopsis: string | null;
  releaseDate: string | null;
  developer: string | null;
  publisher: string | null;
  genre: string | null;
  players: string | null;
  region: string | null;
}

/** Where a normalized record came from, so attribution can be presented. */
export interface MetadataProvenance {
  providerId: MetadataProviderId;
  providerGameId: string;
  sourceCredit: string | null;
  fetchedAt: number;
}

export interface ProviderMetadataRecord {
  metadata: NormalizedMetadata;
  provenance: MetadataProvenance;
}

/** The single cached primary cover, addressed only by an opaque native media reference. */
export interface MediaAsset {
  gameId: number;
  providerId: MetadataProviderId;
  kind: MediaAssetKind;
  state: MediaAssetState;
  providerMediaType: string | null;
  region: string | null;
  mediaRef: string | null;
  contentType: string | null;
  sizeBytes: number | null;
  contentSha256: string | null;
  providerCrc32: string | null;
  providerMd5: string | null;
  providerSha1: string | null;
  sourceCredit: string | null;
  lastFailure: ProviderFailureClass | null;
  fetchedAt: number | null;
  updatedAt: number;
}

/** A heuristic search suggestion. Never an accepted match. */
export interface ProviderCandidate {
  providerGameId: string;
  title: string;
  releaseDate: string | null;
}

export interface UserProviderSelection {
  gameId: number;
  providerId: MetadataProviderId;
  providerGameId: string;
  updatedAt: number;
}

export interface MetadataJob {
  id: number;
  gameId: number;
  providerId: MetadataProviderId;
  kind: MetadataJobKind;
  state: MetadataJobState;
  priority: number;
  attempts: number;
  lastFailure: ProviderFailureClass | null;
  earliestNextAttemptAt: number | null;
  claimedAt: number | null;
  createdAt: number;
  updatedAt: number;
}

export interface GameMetadataState {
  gameId: number;
  providerId: MetadataProviderId;
  status: ProviderMatchStatus;
  matchType: MatchType | null;
  /** True only while a deterministic match still agrees with the current local content evidence. */
  deterministic: boolean;
  providerGameId: string | null;
  providerRomId: string | null;
  unsupportedReason: UnsupportedContentReason | null;
  lastFailure: ProviderFailureClass | null;
  lastCheckedAt: number | null;
  metadata: ProviderMetadataRecord | null;
  cover: MediaAsset | null;
  candidates: ProviderCandidate[];
  userSelection: UserProviderSelection | null;
  jobs: MetadataJob[];
}

/** Provider quota as most recently reported by the provider itself. Every field may be absent. */
export interface ProviderQuotaSnapshot {
  maxThreads: number | null;
  maxRequestsPerMinute: number | null;
  maxRequestsPerDay: number | null;
  maxNegativeRequestsPerDay: number | null;
  requestsToday: number | null;
  negativeRequestsToday: number | null;
}

export interface MetadataProviderStatus {
  providerId: MetadataProviderId;
  credentialsConfigured: boolean;
  userAccount: UserAccountState;
  userAccountName: string | null;
  quota: ProviderQuotaSnapshot;
  quotaObservedAt: number | null;
  deferredUntil: number | null;
  deferReason: ProviderFailureClass | null;
  offline: boolean;
  pendingJobs: number;
  deferredJobs: number;
  failedJobs: number;
}

/**
 * The two whole-library scraper modes.
 *
 * `missingMetadata` targets games that have never had a meaningful provider attempt.
 * `refreshMatched` targets accepted matches whose metadata and cover should be refetched.
 */
export type MetadataScrapeMode = 'missingMetadata' | 'refreshMatched';

export type MetadataScrapeRunStatus =
  'preparing' | 'running' | 'stopping' | 'completed' | 'stopped';

/**
 * Game-count progress for one run.
 *
 * The unit is always games. `processed` is the sum of the five result counts; `running` and
 * `waiting` are deliberately not part of it, because a game the provider has deferred has not been
 * processed.
 */
export interface MetadataScrapeProgress {
  totalGames: number;
  matched: number;
  needsReview: number;
  noMatch: number;
  unsupported: number;
  failed: number;
  running: number;
  waiting: number;
}

export interface MetadataScrapeRun {
  id: number;
  providerId: MetadataProviderId;
  mode: MetadataScrapeMode;
  status: MetadataScrapeRunStatus;
  progress: MetadataScrapeProgress;
  createdAt: number;
  updatedAt: number;
  finishedAt: number | null;
}

export interface MetadataScrapeStatus {
  providerId: MetadataProviderId;
  /** The active run, or the most recent finished one. */
  run: MetadataScrapeRun | null;
  active: boolean;
}

export interface MetadataScrapePreview {
  mode: MetadataScrapeMode;
  eligibleGames: number;
}

export interface MetadataScrapeModeRequest {
  mode: MetadataScrapeMode;
}

/** Readable account state. There is deliberately no password field. */
export interface ProviderAccountStatus {
  configured: boolean;
  state: UserAccountState;
  username: string | null;
}

export interface GameMetadataRequest {
  gameId: number;
}

export interface SelectMetadataCandidateRequest {
  gameId: number;
  providerGameId: string;
}

/** Write-only credential input. Nothing ever returns a password. */
export interface SetProviderCredentialsRequest {
  username: string;
  password: string;
}

/** Stable, exhaustive M7 launch failure codes. React selects messaging from the code alone. */
export type LaunchErrorCode =
  | 'gameNotFound'
  | 'gameUnavailable'
  | 'contentSelectionRequired'
  | 'contentUnavailable'
  | 'runtimeNotReady'
  | 'corePolicyUnresolved'
  | 'coreNotInstalled'
  | 'coreNotApproved'
  | 'biosMissing'
  | 'biosInvalid'
  | 'biosNotCoveredByCatalog'
  | 'hostPrerequisiteMissing'
  | 'gameAlreadyRunning'
  | 'configPreparationFailed'
  | 'spawnFailed'
  | 'processIdentityFailed'
  | 'processExitedDuringLaunch'
  | 'sessionPersistenceFailed'
  | 'saveStateBaselineFailed'
  | 'internalLaunchFailure';

/** A Linux host capability the managed runtime cannot provide. Only a display session blocks. */
export type HostPrerequisite =
  'displaySession' | 'graphicsDevice' | 'audioService' | 'inputDevices';

export interface LaunchDiagnostic {
  kind: HostPrerequisite;
  message: string;
}

/** One launchable version of a game. Deliberately carries no filesystem path. */
export interface LaunchContentOption {
  contentUnitId: number;
  kind: ContentUnitKind;
  localTitle: string;
  fileCount: number;
  availability: ContentUnitAvailability;
}

/** Typed detail. Every field is an identifier React may see; never a path or an OS error. */
export interface LaunchFailureContext {
  systemId: SystemId | null;
  coreId: string | null;
  biosRequirementIds: string[];
  runtimeState: RuntimeState | null;
  hostPrerequisite: HostPrerequisite | null;
  exitCode: number | null;
  contentOptions: LaunchContentOption[];
}

export interface LaunchFailure {
  code: LaunchErrorCode;
  message: string;
  context: LaunchFailureContext;
}

export interface RunningGameSession {
  sessionId: number;
  gameId: number;
  contentUnitId: number;
  coreId: string;
  startedAt: number;
}

/**
 * The durable running-game projection. `blocked` is true only when a managed process record
 * exists whose identity is uncertain: a launch is refused, but no running session can be described.
 */
export interface LaunchState {
  running: RunningGameSession | null;
  blocked: boolean;
}

export type PlaySessionOutcome =
  'running' | 'completed' | 'failedToStart' | 'crashed' | 'interrupted';

export interface ExitedGameSession {
  sessionId: number;
  gameId: number;
  outcome: PlaySessionOutcome;
  exitCode: number | null;
}

export interface GameLaunchStateChanged {
  state: LaunchState;
  exited: ExitedGameSession | null;
}

export type LaunchResponse =
  | { status: 'started'; session: RunningGameSession; diagnostics: LaunchDiagnostic[] }
  | { status: 'contentSelectionRequired'; options: LaunchContentOption[] }
  | { status: 'failed'; error: LaunchFailure };

/** Semantic launch request. There is deliberately no filesystem-path field. */
export interface LaunchGameRequest {
  gameId: number;
  contentUnitId?: number | null;
}

/**
 * Whether RetroFrontier currently *permits* loading a Save State.
 *
 * It is deliberately not a compatibility claim: nothing here says a state will deserialize. It says
 * whether the exact core binary that produced the state can be located and whether the managed
 * launch pipeline is free right now.
 */
export type SaveStateLoadability = 'ready' | 'coreUnavailable' | 'temporarilyBlocked';

export interface SaveStateCapabilities {
  loadability: SaveStateLoadability;
  deletable: boolean;
}

/**
 * The bounded Save-State projection Game Detail renders. It deliberately carries no digest and no
 * filesystem path; `thumbnailRef` is an opaque reference the native media protocol resolves.
 */
export interface SaveStateView {
  id: number;
  gameId: number;
  contentUnitId: number;
  slot: number;
  coreId: string;
  coreDisplayVersion: string | null;
  coreSourceRevision: string | null;
  /** Set only when the game has more than one content unit, so a disc label appears exactly when it disambiguates. */
  contentUnitLabel: string | null;
  createdAt: number;
  updatedAt: number;
  thumbnailRef: string | null;
  capabilities: SaveStateCapabilities;
}

export type SaveStateErrorCode =
  | 'notFound'
  | 'unavailable'
  | 'coreUnavailable'
  | 'temporarilyBlocked'
  | 'integrityMismatch'
  | 'unsafeFilesystemTarget'
  | 'indeterminate'
  | 'reconciliationFailed'
  | 'launchFailed'
  | 'deleteFailed';

export interface SaveStateFailure {
  code: SaveStateErrorCode;
  message: string;
}

/**
 * The result of a save-state load.
 *
 * A load has two genuinely different ways to be refused, and collapsing them would make the UI
 * guess. `refused` is a Save-State verdict — the state is gone, its registered identity no longer
 * matches, its historical core is unavailable, or a game is running. `launchFailed` is the managed
 * launch pipeline's own normalized verdict about a launch that was otherwise permitted.
 *
 * There is deliberately no content-selection arm: the content unit is recorded provenance, so a
 * save-state load never has a choice to offer.
 */
export type LoadSaveStateResponse =
  | { status: 'started'; session: RunningGameSession; diagnostics: LaunchDiagnostic[] }
  | { status: 'refused'; error: SaveStateFailure }
  | { status: 'launchFailed'; error: LaunchFailure };

export type DeleteSaveStateResponse =
  { status: 'deleted'; saveStateId: number } | { status: 'failed'; error: SaveStateFailure };

/** Semantic requests. There is deliberately no path, slot, core, digest, or thumbnail field. */
export interface ListSaveStatesRequest {
  gameId: number;
}

export interface SaveStateRequest {
  saveStateId: number;
}

export const LIBRARY_SCAN_PROGRESS_EVENT = 'library-scan-progress';
export const LIBRARY_SCAN_COMPLETED_EVENT = 'library-scan-completed';
export const METADATA_STATE_CHANGED_EVENT = 'metadata-state-changed';
export const GAME_LAUNCH_STATE_CHANGED_EVENT = 'game-launch-state-changed';

export interface MetadataStateChanged {
  gameId: number;
  providerId: MetadataProviderId;
}

/** Stable codes currently emitted by the Rust `AppError` boundary. */
export const STABLE_IPC_ERROR_CODES = [
  'path_unavailable',
  'storage_unavailable',
  'database_unavailable',
  'migration_failed',
  'runtime_unavailable',
  'catalog_invalid',
  'bios_unavailable',
  'bios_override_disabled',
  'library_unavailable',
  'content_root_invalid_path',
  'content_root_unavailable',
  'content_root_not_directory',
  'content_root_overlap',
  'content_root_invalid_operation',
  'metadata_unavailable',
] as const;

export type StableIpcErrorCode = (typeof STABLE_IPC_ERROR_CODES)[number];
export type ContentRootIpcErrorCode = Extract<
  StableIpcErrorCode,
  | 'content_root_invalid_path'
  | 'content_root_unavailable'
  | 'content_root_not_directory'
  | 'content_root_overlap'
  | 'content_root_invalid_operation'
>;

/**
 * An unknown string remains valid so a newer native backend can be shown with a generic fallback
 * until the frontend contract is updated.
 */
export type IpcErrorCode = StableIpcErrorCode | (string & {});

export interface IpcErrorShape {
  code: IpcErrorCode;
  message: string;
}

export class IpcError extends Error implements IpcErrorShape {
  readonly code: IpcErrorCode;

  constructor(code: IpcErrorCode, message: string) {
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

export async function getRuntimeInstallState(): Promise<RuntimeInstallState> {
  try {
    return await invoke<RuntimeInstallState>('get_runtime_install_state');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

/** Anticipated installation problems arrive inside the response, not as a thrown IPC error. */
export async function installRuntime(): Promise<RuntimeInstallResponse> {
  try {
    return await invoke<RuntimeInstallResponse>('install_runtime');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function repairRuntime(): Promise<RuntimeInstallResponse> {
  try {
    return await invoke<RuntimeInstallResponse>('repair_runtime');
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

/** Opens only the Rust-resolved application-owned managed ROM folder. */
export async function openManagedRomFolder(): Promise<void> {
  try {
    await invoke('open_managed_rom_folder');
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

export async function getScanIssuePage(request: ScanIssuePageRequest = {}): Promise<ScanIssuePage> {
  try {
    return await invoke<ScanIssuePage>('get_scan_issue_page', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function queryLibrary(request: LibraryQueryRequest = {}): Promise<LibraryPage> {
  try {
    return await invoke<LibraryPage>('query_library', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

/** The bounded All Systems shelf projection. One request for every system, never one per system. */
export async function queryLibraryShelves(
  request: LibraryShelvesRequest = {},
): Promise<LibraryShelves> {
  try {
    return await invoke<LibraryShelves>('query_library_shelves', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getLibrarySummary(): Promise<LibrarySummary> {
  try {
    return await invoke<LibrarySummary>('get_library_summary');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getLibraryGameDetail(
  request: LibraryGameRequest,
): Promise<LibraryGameDetail | null> {
  try {
    return await invoke<LibraryGameDetail | null>('get_library_game_detail', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function setGameFavorite(request: SetGameFavoriteRequest): Promise<GameFavorite> {
  try {
    return await invoke<GameFavorite>('set_game_favorite', { request });
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

export async function onMetadataStateChanged(
  handler: (event: MetadataStateChanged) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<MetadataStateChanged>(METADATA_STATE_CHANGED_EVENT, (event) =>
      handler(event.payload),
    );
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function launchGame(request: LaunchGameRequest): Promise<LaunchResponse> {
  try {
    return await invoke<LaunchResponse>('launch_game', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getLaunchState(): Promise<LaunchState> {
  try {
    return await invoke<LaunchState>('get_launch_state');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function onGameLaunchStateChanged(
  handler: (event: GameLaunchStateChanged) => void,
): Promise<UnlistenFn> {
  try {
    return await listen<GameLaunchStateChanged>(GAME_LAUNCH_STATE_CHANGED_EVENT, (event) =>
      handler(event.payload),
    );
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function listSaveStates(request: ListSaveStatesRequest): Promise<SaveStateView[]> {
  try {
    return await invoke<SaveStateView[]>('list_save_states', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

/**
 * Loads a Save State through the one managed launch pipeline.
 *
 * The answer is its own union rather than the ordinary `LaunchResponse`, because the two ways a
 * load can be refused are different verdicts: one is about the state, one is about the launch.
 */
export async function loadSaveState(request: SaveStateRequest): Promise<LoadSaveStateResponse> {
  try {
    return await invoke<LoadSaveStateResponse>('load_save_state', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function deleteSaveState(request: SaveStateRequest): Promise<DeleteSaveStateResponse> {
  try {
    return await invoke<DeleteSaveStateResponse>('delete_save_state', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getGameMetadata(request: GameMetadataRequest): Promise<GameMetadataState> {
  try {
    return await invoke<GameMetadataState>('get_game_metadata', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function requestGameMetadata(request: GameMetadataRequest): Promise<void> {
  try {
    await invoke('request_game_metadata', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function refreshGameMetadata(request: GameMetadataRequest): Promise<void> {
  try {
    await invoke('refresh_game_metadata', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getMetadataProviderStatus(): Promise<MetadataProviderStatus> {
  try {
    return await invoke<MetadataProviderStatus>('get_metadata_provider_status');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function selectGameMetadataCandidate(
  request: SelectMetadataCandidateRequest,
): Promise<void> {
  try {
    await invoke('select_game_metadata_candidate', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function clearGameMetadataCandidate(request: GameMetadataRequest): Promise<void> {
  try {
    await invoke('clear_game_metadata_candidate', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

/**
 * Submits the optional personal provider account. Write-only: the password is handed straight to
 * Rust, which persists it in the OS credential vault, and no read command ever returns it.
 */
export async function setMetadataProviderCredentials(
  request: SetProviderCredentialsRequest,
): Promise<void> {
  try {
    await invoke('set_metadata_provider_credentials', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function clearMetadataProviderCredentials(): Promise<void> {
  try {
    await invoke('clear_metadata_provider_credentials');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function previewMetadataScrape(
  request: MetadataScrapeModeRequest,
): Promise<MetadataScrapePreview> {
  try {
    return await invoke<MetadataScrapePreview>('preview_metadata_scrape', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getMetadataScrapeStatus(): Promise<MetadataScrapeStatus> {
  try {
    return await invoke<MetadataScrapeStatus>('get_metadata_scrape_status');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function startMetadataScrape(
  request: MetadataScrapeModeRequest,
): Promise<MetadataScrapeStatus> {
  try {
    return await invoke<MetadataScrapeStatus>('start_metadata_scrape', { request });
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function stopMetadataScrape(): Promise<MetadataScrapeStatus> {
  try {
    return await invoke<MetadataScrapeStatus>('stop_metadata_scrape');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}

export async function getMetadataProviderAccount(): Promise<ProviderAccountStatus> {
  try {
    return await invoke<ProviderAccountStatus>('get_metadata_provider_account');
  } catch (error: unknown) {
    throw normalizeIpcError(error);
  }
}
