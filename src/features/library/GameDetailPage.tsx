import { useEffect, useRef, type CSSProperties, type ReactNode, type RefObject } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { PixelArrow, PixelStar } from '../../components/ui/PixelIcon';
import { getMetadataAction, hasSelectableCandidates, metadataStateCopy } from './metadataActions';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type { GameLaunchModel } from '../../hooks/useGameLaunch';
import { launchFailureHint, launchFailureTitle } from './launchStatus';
import type {
  GameMetadataState,
  IpcError,
  LibraryGameDetail,
  NormalizedMetadata,
  LaunchContentOption,
  ProviderFailureClass,
  SystemStatus,
  UnsupportedContentReason,
} from '../../platform/ipc';
import { GameCover } from './GameCover';

import {
  getOverallReadiness,
  getReadinessRows,
  type ReadinessRow,
  type ReadinessTone,
} from './readiness';
import { systemAccent, systemAccentKey } from './systemAccents';
import { systemShortLabel } from './systemLabels';

const EMPTY_DETAIL_RESET_KEY = {};

interface GameDetailPageProps {
  gameId: number | null;
  detail: GameDetailModel;
  systemStatus: SystemStatus | null;
  readinessLoading?: boolean;
  readinessError: IpcError | null;
  launch: GameLaunchModel;
  onRetryReadiness: () => void;
  onBackToLibrary: () => void;
}

function contentKindLabel(kind: LibraryGameDetail['contentUnits'][number]['kind']) {
  switch (kind) {
    case 'singleFile':
      return 'SINGLE FILE';
    case 'chd':
      return 'CHD';
    case 'cueBin':
      return 'CUE / BIN';
    case 'gdi':
      return 'GDI';
    case 'm3u':
      return 'M3U PLAYLIST';
  }
}

function formatSystemId(systemId: string) {
  return systemId.replaceAll('_', ' ').toLocaleUpperCase();
}

function formatFileCount(fileCount: number) {
  return `${fileCount.toLocaleString('en-US')} ${fileCount === 1 ? 'FILE' : 'FILES'}`;
}

function toneClass(tone: ReadinessTone) {
  return `readiness-status readiness-status--${tone}`;
}

/**
 * B6 leads the hero with a compact release year. Only a real normalized release date can produce
 * one: the first four digits of the authoritative value, never a year inferred from a filename or
 * from an unrelated local timestamp.
 */
function releaseYear(releaseDate: string | null | undefined): string | null {
  return releaseDate?.match(/^\d{4}/)?.[0] ?? null;
}

interface HeroInfoRow {
  key: string;
  label: string;
  value: string;
  dateTime?: string;
}

/**
 * B6's compact hero information block for an enriched game. Only authoritative normalized values
 * become rows; a missing optional field is omitted rather than rendered as a placeholder. Genre is
 * already a hero chip and the release year is already visible beside it, so a release row is added
 * only when the authoritative date carries more than that year.
 */
function enrichedHeroRows(
  normalized: NormalizedMetadata | null,
  year: string | null,
): HeroInfoRow[] {
  if (!normalized) return [];
  const rows: HeroInfoRow[] = [];
  const release = normalized.releaseDate?.trim();
  if (release && release !== year) {
    rows.push({ key: 'release', label: 'RELEASE', value: release, dateTime: release });
  }
  if (normalized.developer) {
    rows.push({ key: 'developer', label: 'DEVELOPER', value: normalized.developer });
  }
  if (normalized.publisher) {
    rows.push({ key: 'publisher', label: 'PUBLISHER', value: normalized.publisher });
  }
  if (normalized.players) {
    rows.push({ key: 'players', label: 'PLAYERS', value: normalized.players });
  }
  if (normalized.region) {
    rows.push({ key: 'region', label: 'REGION', value: normalized.region });
  }
  return rows;
}

/**
 * A local-only game has no provider metadata, and none is invented for it. The hero is instead
 * composed from truth M6 already owns: the local availability and shape of the content, and the
 * at-a-glance readiness statuses that `readiness.ts` derived. The explanations, errors, and
 * recovery guidance for those statuses stay in the readiness section below.
 *
 * Per-unit shape is projected only for a single content unit; a multi-unit game keeps that detail
 * in the Local Content section instead of collapsing several units into one misleading row.
 */
function localHeroRows(
  localDetail: LibraryGameDetail | null,
  readinessRows: readonly ReadinessRow[],
): HeroInfoRow[] {
  if (!localDetail) return [];
  const rows: HeroInfoRow[] = [];
  const status = (id: ReadinessRow['id']) => readinessRows.find((row) => row.id === id)?.status;

  const content = status('localContent');
  if (content) {
    rows.push({ key: 'content', label: 'CONTENT', value: content });
  }
  const [unit] = localDetail.contentUnits;
  if (unit && localDetail.contentUnits.length === 1) {
    rows.push({ key: 'format', label: 'FORMAT', value: contentKindLabel(unit.kind) });
    rows.push({ key: 'path', label: 'PATH', value: unit.primaryRelativePath });
  }
  for (const id of ['runtime', 'core', 'bios'] as const) {
    const value = status(id);
    if (value) {
      rows.push({ key: id, label: id.toLocaleUpperCase(), value });
    }
  }
  return rows;
}

/**
 * B6 section rule: a compact label, a hard rule, and an optional trailing status. Game Detail's
 * secondary sections use it instead of full-width dashboard panels so the game stays dominant.
 */
function SectionHeading({
  headingId,
  headingRef,
  focusable = false,
  title,
  status,
}: {
  headingId: string;
  headingRef?: RefObject<HTMLHeadingElement | null>;
  focusable?: boolean;
  title: string;
  status?: ReactNode;
}) {
  return (
    <div className="game-detail-section-heading">
      <h2 id={headingId} ref={headingRef} tabIndex={focusable ? -1 : undefined}>
        {title}
      </h2>
      <span aria-hidden="true" className="game-detail-section-rule" />
      {status}
    </div>
  );
}

/**
 * The provider-specific reason, added under the state description. The state description already
 * states that the local game remains usable, so this line no longer repeats that same truth.
 */
function metadataFailureCopy(failure: ProviderFailureClass | null) {
  switch (failure) {
    case 'capacityDeferred':
      return 'Provider capacity is temporarily deferred. Cached metadata remains visible.';
    case 'dailyQuotaExceeded':
      return 'The provider daily quota is temporarily deferred. Cached metadata remains visible.';
    case 'negativeQuotaExceeded':
      return 'The provider no-match quota is temporarily deferred. Cached metadata remains visible.';
    case 'providerRestricted':
      return 'Provider access is temporarily deferred. Cached metadata remains visible.';
    case 'providerUnavailable':
    case 'transport':
    case 'transientServer':
      return 'The metadata provider is unavailable. Cached metadata remains visible.';
    case 'developerAuthenticationFailed':
    case 'credentialsUnavailable':
      return 'The metadata provider is not configured for this build.';
    case 'userAuthenticationFailed':
      return 'The optional personal provider account needs attention.';
    case 'invalidRequest':
      return 'This metadata request is not valid for the current local content. Automatic retry is unavailable.';
    case 'clientRejected':
      return 'The metadata provider rejected this request. Automatic retry is unavailable.';
    case 'mediaUnavailable':
      return 'Metadata is available, but the provider cover could not be cached.';
    default:
      return null;
  }
}

function unsupportedContentCopy(reason: UnsupportedContentReason | null) {
  switch (reason) {
    case 'systemNotMapped':
      return 'This system is not mapped to the current metadata provider.';
    case 'chdRepresentationUndefined':
      return 'CHD content is playable locally, but this representation is not automatically identified.';
    case 'cueBinRepresentationUndefined':
      return 'CUE / BIN content is playable locally, but this representation is not automatically identified.';
    case 'gdiRepresentationUndefined':
      return 'GDI content is playable locally, but this representation is not automatically identified.';
    case 'playlistIsNotIdentity':
      return 'A playlist is not used as provider identity. Its local content remains available.';
    case 'containerRepresentationUndefined':
      return 'This container representation is not automatically identified by the provider.';
    case 'missingContentEvidence':
      return 'The local content does not have enough identification evidence for automatic lookup.';
    case 'noPrimaryContentFile':
      return 'The local content has no primary file for automatic provider lookup.';
    default:
      return null;
  }
}

function metadataOperationPendingLabel(kind: GameDetailModel['metadataActionKind']) {
  switch (kind) {
    case 'request':
      return 'REQUESTING METADATA';
    case 'refresh':
      return 'UPDATING METADATA';
    case 'select':
      return 'SELECTING MATCH';
    case 'clear':
      return 'FORGETTING PROVIDER CHOICE';
    default:
      return 'UPDATING METADATA';
  }
}

function BackToLibrary({ onBackToLibrary }: { onBackToLibrary: () => void }) {
  return (
    <a
      className="game-detail-back"
      href="/library"
      onClick={(event) => {
        event.preventDefault();
        onBackToLibrary();
      }}
    >
      <PixelArrow direction="left" />
      <span>BACK TO LIBRARY</span>
    </a>
  );
}

function DetailLoading({ metadataLoading }: { metadataLoading: boolean }) {
  return (
    <div className="game-detail-loading" role="status" aria-live="polite">
      <span className="loading-block" aria-hidden="true" />
      <span>READING GAME DETAIL…</span>
      {metadataLoading ? <span className="game-detail-loading-secondary">METADATA…</span> : null}
    </div>
  );
}

function LocalContentSection({ detail }: { detail: LibraryGameDetail }) {
  return (
    <section aria-labelledby="local-content-heading" className="game-detail-section">
      <SectionHeading
        headingId="local-content-heading"
        status={
          <span className="game-detail-section-meta">
            {detail.contentUnits.length}{' '}
            {detail.contentUnits.length === 1 ? 'UNIT' : 'CONTENT UNITS'}
          </span>
        }
        title="LOCAL CONTENT"
      />
      {detail.contentUnits.length > 0 ? (
        <ul className="game-detail-content-list">
          {detail.contentUnits.map((unit) => (
            <li className="game-detail-content-item" key={unit.unitId}>
              <div className="game-detail-content-item-heading">
                <div>
                  <h3>{unit.localTitle}</h3>
                  <span className="game-detail-content-kind">{contentKindLabel(unit.kind)}</span>
                </div>
                <span
                  className={`game-detail-unit-status game-detail-unit-status--${unit.availability}`}
                >
                  {unit.availability === 'available'
                    ? 'AVAILABLE'
                    : unit.availability === 'incomplete'
                      ? 'INCOMPLETE'
                      : 'MISSING'}
                </span>
              </div>
              <code className="game-detail-relative-path" title={unit.primaryRelativePath}>
                {unit.primaryRelativePath}
              </code>
              {/* Operationally useful for troubleshooting a content root, but tertiary: it never
                  competes with the local title, kind, availability, or path. */}
              <div className="game-detail-content-item-meta">
                <span>CONTENT ROOT #{unit.rootId}</span>
                <span>{formatFileCount(unit.fileCount)}</span>
              </div>
            </li>
          ))}
        </ul>
      ) : (
        <p className="game-detail-empty-copy">
          The game record has no associated content units. The local association may need a scan.
        </p>
      )}
    </section>
  );
}

function ReadinessSection({
  detail,
  rows,
  systemStatus,
  readinessLoading,
  readinessError,
  onRetryReadiness,
}: {
  detail: LibraryGameDetail | null;
  rows: readonly ReadinessRow[];
  systemStatus: SystemStatus | null;
  readinessLoading: boolean;
  readinessError: IpcError | null;
  onRetryReadiness: () => void;
}) {
  const availability = detail?.availability ?? null;
  const contentUnits = detail?.contentUnits ?? [];
  const overall = readinessLoading
    ? {
        tone: 'unknown' as const,
        label: 'CHECKING READINESS',
        detail: 'Reading the current runtime, core, and BIOS snapshot…',
      }
    : getOverallReadiness(availability, systemStatus, contentUnits);

  return (
    <section aria-labelledby="readiness-heading" className="game-detail-section">
      <SectionHeading
        headingId="readiness-heading"
        status={<span className={toneClass(overall.tone)}>{overall.label}</span>}
        title="EMULATION READINESS"
      />
      <p className="game-detail-readiness-summary">{overall.detail}</p>
      {readinessError ? (
        <InlineError
          title="READINESS UNAVAILABLE"
          message="RetroFrontier could not read the current runtime, core, or BIOS snapshot. Local content and metadata remain available."
          actionLabel="RETRY READINESS"
          onAction={onRetryReadiness}
        />
      ) : null}
      {/* One requirement list, not four dashboard cards. Every readiness truth from readiness.ts
          is still rendered per row, and status never depends on colour alone. */}
      <ul aria-label="Emulation requirements" className="readiness-list">
        {rows.map((row) => (
          <li className={`readiness-row readiness-row--${row.tone}`} key={row.id}>
            <span className="readiness-row-label">{row.label}</span>
            <span className={toneClass(row.tone)}>{row.status}</span>
            <p className="readiness-row-detail">{row.detail}</p>
          </li>
        ))}
      </ul>
      {rows.some(
        ({ id, tone }) => id === 'bios' && (tone === 'missing' || tone === 'unavailable'),
      ) && systemStatus?.bios.policy === 'required' ? (
        <p className="game-detail-guidance">
          Supply the required BIOS file through RetroFrontier&apos;s managed BIOS location, then
          retry readiness.
        </p>
      ) : null}
    </section>
  );
}

function MetadataCandidates({
  detail,
  metadataState,
}: {
  detail: GameDetailModel;
  metadataState: GameMetadataState;
}) {
  if (!hasSelectableCandidates(metadataState)) {
    return null;
  }

  const selectedProviderGameId = metadataState.userSelection?.providerGameId ?? null;
  const selecting = detail.metadataActionKind === 'select';

  return (
    <div
      aria-labelledby="metadata-candidate-heading"
      className="metadata-candidate-panel"
      role="group"
    >
      <div className="metadata-candidate-heading">
        <div>
          <span className="game-detail-kicker">ORDERED PROVIDER RESULTS</span>
          <h3 id="metadata-candidate-heading">CHOOSE A METADATA MATCH</h3>
        </div>
        <span className="game-detail-section-meta">
          {metadataState.candidates.length}{' '}
          {metadataState.candidates.length === 1 ? 'OPTION' : 'OPTIONS'}
        </span>
      </div>
      <ul aria-label="Metadata candidates" className="metadata-candidate-list">
        {metadataState.candidates.map((candidate, index) => {
          const selected = selectedProviderGameId === candidate.providerGameId;
          const selectingCandidate =
            selecting && detail.metadataActionTarget === candidate.providerGameId;
          const date = candidate.releaseDate
            ? `RELEASED ${candidate.releaseDate}`
            : 'RELEASE DATE UNKNOWN';
          return (
            <li
              className={`metadata-candidate${selected ? ' metadata-candidate--selected' : ''}`}
              key={`${candidate.providerGameId}-${index}`}
            >
              <div className="metadata-candidate-copy">
                <span className="metadata-candidate-index">
                  {String(index + 1).padStart(2, '0')}
                </span>
                <div>
                  <h4>{candidate.title}</h4>
                  <span>{date}</span>
                </div>
              </div>
              <PixelButton
                aria-busy={selectingCandidate || undefined}
                aria-pressed={selected}
                aria-label={`${selectingCandidate ? 'Selecting' : 'Select'} ${candidate.title} candidate ${index + 1}`}
                className="metadata-candidate-action"
                disabled={detail.metadataActionPending}
                onClick={() => void detail.selectMetadataCandidate(candidate.providerGameId)}
                type="button"
                variant={selected ? 'primary' : 'secondary'}
              >
                {selectingCandidate ? 'SELECTING…' : selected ? 'SELECTED' : 'SELECT'}
              </PixelButton>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function MetadataSection({ detail }: { detail: GameDetailModel }) {
  const metadataHeading = useRef<HTMLHeadingElement>(null);
  const wasActionPending = useRef(false);
  const metadataState = detail.metadata;
  const stateCopy = metadataState
    ? metadataStateCopy(
        metadataState.status,
        metadataState.unsupportedReason,
        metadataState.candidates.length,
      )
    : null;
  const action = getMetadataAction(metadataState);
  const pendingLabel = metadataOperationPendingLabel(detail.metadataActionKind);
  const failureCopy = metadataFailureCopy(metadataState?.lastFailure ?? null);
  const unsupportedCopy = unsupportedContentCopy(metadataState?.unsupportedReason ?? null);
  const userConfirmed =
    metadataState?.userSelection !== null && metadataState?.userSelection !== undefined;
  const sourceCredit = metadataState?.metadata?.provenance.sourceCredit ?? null;

  const runMetadataAction = () => {
    if (!action) return;
    if (action.kind === 'request') void detail.requestMetadata();
    else void detail.refreshMetadata();
  };

  useEffect(() => {
    if (wasActionPending.current && !detail.metadataActionPending) {
      metadataHeading.current?.focus();
    }
    wasActionPending.current = detail.metadataActionPending;
  }, [detail.metadataActionPending]);

  // A candidate workflow is the one metadata state that genuinely needs more than the shared
  // Detail content width; every ordinary state stays inside it.
  const wide = metadataState ? hasSelectableCandidates(metadataState) : false;

  return (
    // The normalized data itself now belongs to the hero. What remains here is the provider
    // workflow and its state, which only takes visual weight when a decision or a failure is real.
    <section
      aria-labelledby="metadata-heading"
      className={`game-detail-section${wide ? ' game-detail-section--wide' : ''}`}
    >
      <SectionHeading
        focusable
        headingId="metadata-heading"
        headingRef={metadataHeading}
        title="METADATA"
      />
      {detail.metadataError && (
        <InlineError
          title={metadataState ? 'METADATA REFRESH FAILED' : 'METADATA UNAVAILABLE'}
          message="RetroFrontier could not read this game's normalized metadata. Local content and readiness remain available."
          actionLabel="RETRY METADATA"
          onAction={() => void detail.retryMetadata()}
        />
      )}
      {detail.metadataActionError ? (
        <InlineError
          title="METADATA ACTION FAILED"
          message="The metadata action could not be completed. Cached metadata remains unchanged; try again when the provider is available."
        />
      ) : null}
      {detail.metadataLoading && !metadataState ? (
        <p className="game-detail-inline-status" role="status">
          READING NORMALIZED METADATA…
        </p>
      ) : null}
      {stateCopy ? (
        <div
          aria-live="polite"
          className={`metadata-state-callout metadata-state-callout--${metadataState?.status}`}
          role="status"
        >
          <strong>{stateCopy.label}</strong>
          <p>{stateCopy.description}</p>
          {failureCopy ? <p className="metadata-state-detail">{failureCopy}</p> : null}
          {unsupportedCopy ? <p className="metadata-state-detail">{unsupportedCopy}</p> : null}
        </div>
      ) : null}
      {metadataState && action ? (
        <div className="metadata-action-row">
          <PixelButton
            aria-busy={detail.metadataActionPending}
            disabled={detail.metadataActionPending || detail.metadataLoading}
            onClick={runMetadataAction}
            type="button"
            variant="secondary"
          >
            {detail.metadataActionPending ? pendingLabel : action.label}
          </PixelButton>
          {detail.metadataActionPending ? (
            <span className="game-detail-inline-status" role="status">
              {pendingLabel}…
            </span>
          ) : null}
        </div>
      ) : null}
      {detail.metadataActionPending && !action ? (
        <p className="game-detail-inline-status" role="status">
          {pendingLabel}…
        </p>
      ) : null}
      {metadataState?.jobs.some((job) => ['pending', 'running', 'deferred'].includes(job.state)) ? (
        <p className="game-detail-inline-status" role="status">
          METADATA WORK IS ALREADY QUEUED. LOCAL CONTENT REMAINS AVAILABLE.
        </p>
      ) : null}
      {userConfirmed ? (
        <div className="metadata-selection-summary">
          <div>
            <strong>USER-CONFIRMED MATCH</strong>
            <p>Forget the manual provider association; cached metadata remains available.</p>
          </div>
          <PixelButton
            disabled={detail.metadataActionPending}
            onClick={() => void detail.clearMetadataSelection()}
            type="button"
            variant="secondary"
          >
            FORGET PROVIDER CHOICE
          </PixelButton>
        </div>
      ) : null}
      {metadataState ? <MetadataCandidates detail={detail} metadataState={metadataState} /> : null}
      {sourceCredit ? (
        <p className="game-detail-provenance">SOURCE CREDIT · {sourceCredit}</p>
      ) : null}
    </section>
  );
}

function contentOptionLabel(option: LaunchContentOption) {
  return `${option.localTitle} · ${contentKindLabel(option.kind)} · ${formatFileCount(option.fileCount)}`;
}

/**
 * The M7 Play action.
 *
 * Rust decides whether a launch may proceed, so this control never pre-judges readiness: it stays
 * available and renders whatever normalized result comes back. It is disabled only while this
 * screen is waiting, while a managed game is running, and while launch state is uncertain.
 */
function LaunchAction({
  gameId,
  launch,
  title,
}: {
  gameId: number;
  launch: GameLaunchModel;
  title: string;
}) {
  const runningThisGame = launch.running?.gameId === gameId;
  const runningAnotherGame = launch.running !== null && !runningThisGame;
  const pending = launch.pendingGameId === gameId;
  const disabled = pending || launch.running !== null || launch.blocked;
  const label = runningThisGame
    ? 'RUNNING'
    : pending
      ? 'LAUNCHING…'
      : runningAnotherGame
        ? 'ANOTHER GAME IS RUNNING'
        : 'PLAY';

  return (
    <>
      <button
        aria-busy={pending || undefined}
        aria-label={runningThisGame ? `${title} is running` : `Play ${title}`}
        className="game-detail-play"
        data-state={runningThisGame ? 'running' : pending ? 'launching' : 'idle'}
        disabled={disabled}
        onClick={() => void launch.launch(gameId)}
        type="button"
      >
        {label}
      </button>

      {/* One live region for the whole launch interaction, so a screen reader hears the state
          change once rather than once per element. */}
      <div aria-live="polite" className="game-detail-launch-status">
        {runningThisGame ? (
          <p className="game-detail-inline-status">
            RETROARCH IS RUNNING · RETROFRONTIER RETURNS WHEN IT EXITS
          </p>
        ) : null}
        {pending ? <p className="game-detail-inline-status">STARTING RETROARCH…</p> : null}
        {launch.blocked && !runningThisGame ? (
          <p className="game-detail-action-error">
            A PREVIOUS GAME PROCESS COULD NOT BE VERIFIED · RESTART RETROFRONTIER
          </p>
        ) : null}
        {launch.diagnostics.length > 0 && runningThisGame ? (
          <ul className="game-detail-launch-diagnostics">
            {launch.diagnostics.map((diagnostic) => (
              <li key={diagnostic.kind}>{diagnostic.message}</li>
            ))}
          </ul>
        ) : null}
      </div>

      {launch.contentOptions && launch.contentOptions.length > 0 ? (
        <div className="game-detail-launch-selection">
          <h3>CHOOSE A VERSION</h3>
          <ul>
            {launch.contentOptions.map((option) => (
              <li key={option.contentUnitId}>
                <button
                  onClick={() => void launch.launch(gameId, option.contentUnitId)}
                  type="button"
                >
                  {contentOptionLabel(option)}
                </button>
              </li>
            ))}
          </ul>
          <button
            className="text-link"
            onClick={() => launch.cancelContentSelection()}
            type="button"
          >
            CANCEL
          </button>
        </div>
      ) : null}

      {launch.failure ? (
        <InlineError
          title={launchFailureTitle(launch.failure.code)}
          message={[launch.failure.message, launchFailureHint(launch.failure.code)]
            .filter((part): part is string => Boolean(part))
            .join(' ')}
          actionLabel="DISMISS"
          onAction={() => launch.dismissFailure()}
        />
      ) : null}
    </>
  );
}

export function GameDetailPage({
  gameId,
  detail,
  systemStatus,
  readinessLoading = false,
  readinessError,
  launch,
  onRetryReadiness,
  onBackToLibrary,
}: GameDetailPageProps) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const localDetail = detail.localDetail;
  const normalized = detail.metadata?.metadata?.metadata ?? null;
  const title = normalized?.title?.trim() || localDetail?.localTitle.trim() || 'GAME DETAIL';
  const systemId = localDetail?.systemId ?? systemStatus?.id ?? null;
  const systemName =
    systemStatus?.displayName ?? (systemId ? formatSystemId(systemId) : 'SYSTEM UNKNOWN');
  const shortSystem = systemId ? systemShortLabel(systemId, systemName) : systemName;
  const accent = systemAccent(systemId ?? 'unknown');
  const cover = detail.metadata?.cover;
  const coverRef = cover?.state === 'cached' ? cover.mediaRef : null;
  const notFound = gameId === null || (detail.localLoaded && !localDetail && !detail.localError);
  const localError = detail.localError;
  const year = releaseYear(normalized?.releaseDate);
  const readinessRows = getReadinessRows(
    localDetail?.availability ?? null,
    systemStatus,
    localDetail?.contentUnits ?? [],
    readinessLoading,
  );
  // Real provider metadata always wins the hero. A local-only game falls back to the truthful
  // local/readiness projection rather than to an almost empty right half.
  const enrichedRows = enrichedHeroRows(normalized, year);
  const infoRows =
    enrichedRows.length > 0 ? enrichedRows : localHeroRows(localDetail, readinessRows);
  // Local telemetry is only worth hero space when it is a genuinely different identity from the
  // one already shown as the title.
  const localTitle = localDetail?.localTitle.trim() ?? '';
  const showLocalTitle =
    localTitle !== '' && localTitle.toLocaleLowerCase() !== title.toLocaleLowerCase();

  useEffect(() => {
    headingRef.current?.focus();
  }, [gameId, notFound]);

  return (
    <main
      aria-labelledby="game-detail-title"
      className="app-main game-detail-main"
      id="game-detail-main"
    >
      <BackToLibrary onBackToLibrary={onBackToLibrary} />

      {notFound ? (
        <>
          <h1 className="game-detail-title" id="game-detail-title" ref={headingRef} tabIndex={-1}>
            {gameId === null ? 'INVALID GAME LINK' : 'GAME NOT FOUND'}
          </h1>
          <section className="game-detail-not-found" aria-labelledby="game-not-found-copy">
            <h2 id="game-not-found-copy">
              {gameId === null
                ? 'THIS GAME LINK IS INVALID'
                : 'THIS GAME IS NO LONGER IN THE LIBRARY'}
            </h2>
            <p>
              {gameId === null
                ? 'The game ID in this route is not valid.'
                : 'The local game record could not be found. It may have changed after a scan.'}
            </p>
            <a
              className="text-link"
              href="/library"
              onClick={(event) => {
                event.preventDefault();
                onBackToLibrary();
              }}
            >
              RETURN TO LIBRARY
            </a>
          </section>
        </>
      ) : (
        <>
          {detail.localLoading && !detail.localLoaded ? (
            <DetailLoading metadataLoading={detail.metadataLoading} />
          ) : null}
          {localError ? (
            <InlineError
              title={localDetail ? 'LOCAL DETAIL REFRESH FAILED' : 'GAME DETAIL UNAVAILABLE'}
              message="RetroFrontier could not read the bounded local game detail. Metadata may still be shown; try the local detail again."
              actionLabel="RETRY LOCAL DETAIL"
              onAction={() => void detail.retryLocal()}
            />
          ) : null}

          {/* B6 hero: 280px cover column, the one dominant title, compact identity, and the
              cover-column actions: the M7 Play action above the M6 Favorite action. */}
          <section
            className="game-detail-hero"
            data-system-accent={systemAccentKey(systemId ?? 'unknown')}
            style={{ '--system-accent': accent } as CSSProperties}
          >
            <div className="game-detail-cover-column">
              <div className="game-detail-cover-frame">
                <GameCover
                  accent={accent}
                  className="game-detail-cover"
                  coverRef={coverRef}
                  placeholderClassName="game-detail-placeholder"
                  resetKey={cover ?? detail.metadata ?? localDetail ?? EMPTY_DETAIL_RESET_KEY}
                  loading="eager"
                  title={title}
                />
              </div>
              {localDetail ? (
                <div className="game-detail-cover-actions">
                  {gameId !== null ? (
                    <LaunchAction gameId={gameId} launch={launch} title={title} />
                  ) : null}
                  <button
                    aria-busy={detail.favoritePending || undefined}
                    aria-label={
                      localDetail.favorite
                        ? `Remove ${title} from favorites`
                        : `Add ${title} to favorites`
                    }
                    aria-pressed={localDetail.favorite}
                    className="game-detail-favorite"
                    onClick={() => void detail.toggleFavorite()}
                    type="button"
                  >
                    <PixelStar filled={localDetail.favorite} />
                    {localDetail.favorite ? 'FAVORITED' : 'ADD TO FAVORITES'}
                  </button>
                  {detail.favoriteError ? (
                    <p className="game-detail-action-error" role="alert">
                      FAVORITE NOT UPDATED · TRY AGAIN
                    </p>
                  ) : null}
                </div>
              ) : null}
            </div>

            <div className="game-detail-hero-copy">
              {/* B6 leads with the game title; the compact identity chips sit underneath it. DOM
                  order and visual order agree — nothing here is reordered in CSS. */}
              <h1
                className="game-detail-title"
                id="game-detail-title"
                ref={headingRef}
                tabIndex={-1}
              >
                {title}
              </h1>

              <div className="game-detail-chips">
                <span className="game-detail-system" title={systemName}>
                  <span aria-hidden="true">{shortSystem}</span>
                  <span className="visually-hidden">{systemName}</span>
                </span>
                {normalized?.genre ? (
                  <span className="game-detail-chip">{normalized.genre}</span>
                ) : null}
                {year ? (
                  <time
                    className="game-detail-year"
                    dateTime={normalized?.releaseDate ?? undefined}
                  >
                    {year}
                  </time>
                ) : null}
                {localDetail?.availability === 'unavailable' ? (
                  <span className="game-detail-availability game-detail-availability--unavailable">
                    LOCAL CONTENT MISSING
                  </span>
                ) : null}
              </div>

              {showLocalTitle ? (
                <p className="game-detail-local-identity">LOCAL TITLE · {localTitle}</p>
              ) : null}

              {normalized?.synopsis ? (
                <p className="game-detail-synopsis">{normalized.synopsis}</p>
              ) : null}

              {infoRows.length > 0 ? (
                <dl className="game-detail-info">
                  {infoRows.map((row) => (
                    <div key={row.key}>
                      <dt>{row.label}</dt>
                      <dd>
                        {row.dateTime ? (
                          <time dateTime={row.dateTime}>{row.value}</time>
                        ) : (
                          row.value
                        )}
                      </dd>
                    </div>
                  ))}
                </dl>
              ) : null}
            </div>
          </section>

          <ReadinessSection
            detail={localDetail}
            onRetryReadiness={onRetryReadiness}
            rows={readinessRows}
            readinessLoading={readinessLoading}
            readinessError={readinessError}
            systemStatus={systemStatus}
          />
          <MetadataSection detail={detail} />
          {localDetail ? <LocalContentSection detail={localDetail} /> : null}
        </>
      )}
    </main>
  );
}
