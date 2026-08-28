import { useEffect, useRef, type CSSProperties } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { PixelArrow } from '../../components/ui/PixelIcon';
import { getMetadataAction, hasSelectableCandidates, metadataStateCopy } from './metadataActions';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type {
  GameMetadataState,
  IpcError,
  LibraryGameDetail,
  ProviderFailureClass,
  SystemStatus,
  UnsupportedContentReason,
} from '../../platform/ipc';
import { GameCover } from './GameCover';
import { PixelStar } from './GameCard';
import { getOverallReadiness, getReadinessRows, type ReadinessTone } from './readiness';
import { systemAccent } from './systemAccents';

interface GameDetailPageProps {
  gameId: number | null;
  detail: GameDetailModel;
  systemStatus: SystemStatus | null;
  readinessLoading?: boolean;
  readinessError: IpcError | null;
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
      return 'The metadata provider is not configured for this build. Local content remains usable.';
    case 'userAuthenticationFailed':
      return 'The optional personal provider account needs attention. Local content remains usable.';
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
    <section
      aria-labelledby="local-content-heading"
      className="game-detail-panel game-detail-content"
    >
      <div className="game-detail-panel-heading">
        <div>
          <span className="game-detail-kicker">LOCAL CONTENT</span>
          <h2 id="local-content-heading">ASSOCIATED CONTENT</h2>
        </div>
        <span className="game-detail-panel-meta">
          {detail.contentUnits.length} {detail.contentUnits.length === 1 ? 'UNIT' : 'CONTENT UNITS'}
        </span>
      </div>
      <div className="game-detail-content-summary">
        <span
          className={`game-detail-availability game-detail-availability--${detail.availability}`}
        >
          {detail.availability === 'available'
            ? 'LOCAL CONTENT AVAILABLE'
            : 'LOCAL CONTENT MISSING'}
        </span>
        <span>
          {detail.contentUnits.length === 0 ? 'NO CONTENT UNITS' : 'LOCAL UNITS SUMMARIZED'}
        </span>
      </div>
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
  systemStatus,
  readinessLoading,
  readinessError,
  onRetryReadiness,
}: {
  detail: LibraryGameDetail | null;
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
  const rows = getReadinessRows(availability, systemStatus, contentUnits);

  return (
    <section
      aria-labelledby="readiness-heading"
      className="game-detail-panel game-detail-readiness"
    >
      <div className="game-detail-panel-heading">
        <div>
          <span className="game-detail-kicker">REQUIREMENTS</span>
          <h2 id="readiness-heading">EMULATION READINESS</h2>
        </div>
        <span className={toneClass(overall.tone)}>{overall.label}</span>
      </div>
      <p className="game-detail-readiness-summary">{overall.detail}</p>
      {readinessError ? (
        <InlineError
          title="READINESS UNAVAILABLE"
          message="RetroFrontier could not read the current runtime, core, or BIOS snapshot. Local content and metadata remain available."
          actionLabel="RETRY READINESS"
          onAction={onRetryReadiness}
        />
      ) : null}
      <div className="readiness-grid">
        {rows.map((row) => (
          <article className={`readiness-row readiness-row--${row.tone}`} key={row.id}>
            <div className="readiness-row-heading">
              <h3>{row.label}</h3>
              <span className={toneClass(row.tone)}>{row.status}</span>
            </div>
            <p>{row.detail}</p>
          </article>
        ))}
      </div>
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
    return metadataState.status === 'ambiguous' ? (
      <p className="game-detail-empty-copy">
        No provider candidates were returned. Search again when the provider is available; local
        content is unchanged.
      </p>
    ) : null;
  }

  const selectedProviderGameId = metadataState.userSelection?.providerGameId ?? null;
  const selecting = detail.metadataActionKind === 'select';
  const description =
    metadataState.unsupportedReason !== null
      ? 'Automatic identification is not available for this content. Choose a provider candidate below to resolve it manually.'
      : metadataState.status === 'deferred'
        ? 'Provider work is deferred, but these candidates remain available for manual resolution.'
        : metadataState.status === 'failed'
          ? 'Automatic provider work did not complete, but these candidates remain available for manual resolution.'
          : metadataState.status === 'stale'
            ? 'The previous provider match needs revalidation; these candidates remain available for manual resolution.'
            : 'These provider candidates are available for manual resolution. Choose one to confirm the provider match.';

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
        <span className="game-detail-panel-meta">
          {metadataState.candidates.length}{' '}
          {metadataState.candidates.length === 1 ? 'OPTION' : 'OPTIONS'}
        </span>
      </div>
      <p className="game-detail-empty-copy">{description}</p>
      <ul aria-label="Metadata candidates" className="metadata-candidate-list">
        {metadataState.candidates.map((candidate, index) => {
          const selected = selectedProviderGameId === candidate.providerGameId;
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
                aria-busy={selecting}
                aria-pressed={selected}
                aria-label={`${selecting ? 'Selecting' : 'Select'} ${candidate.title} candidate ${index + 1}`}
                className="metadata-candidate-action"
                disabled={detail.metadataActionPending}
                onClick={() => void detail.selectMetadataCandidate(candidate.providerGameId)}
                type="button"
                variant={selected ? 'primary' : 'secondary'}
              >
                {selecting ? 'SELECTING…' : selected ? 'SELECTED' : 'SELECT'}
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
  const normalized = metadataState?.metadata?.metadata ?? null;
  const stateCopy = metadataState
    ? metadataStateCopy(metadataState.status, metadataState.unsupportedReason)
    : null;
  const action = getMetadataAction(metadataState);
  const pendingLabel = metadataOperationPendingLabel(detail.metadataActionKind);
  const failureCopy = metadataFailureCopy(metadataState?.lastFailure ?? null);
  const unsupportedCopy = unsupportedContentCopy(metadataState?.unsupportedReason ?? null);
  const userConfirmed =
    metadataState?.userSelection !== null && metadataState?.userSelection !== undefined;

  const runMetadataAction = () => {
    if (!action) return;
    if (action.kind === 'request') void detail.requestMetadata();
    else void detail.refreshMetadata();
  };

  const stateDescription =
    metadataState?.status === 'ambiguous' && metadataState.candidates.length === 0
      ? 'No provider candidates are available. Search again without changing local content.'
      : stateCopy?.description;

  useEffect(() => {
    if (wasActionPending.current && !detail.metadataActionPending) {
      metadataHeading.current?.focus();
    }
    wasActionPending.current = detail.metadataActionPending;
  }, [detail.metadataActionPending]);

  return (
    <section aria-labelledby="metadata-heading" className="game-detail-panel game-detail-metadata">
      <div className="game-detail-panel-heading">
        <div>
          <span className="game-detail-kicker">ENRICHMENT</span>
          <h2 id="metadata-heading" ref={metadataHeading} tabIndex={-1}>
            NORMALIZED METADATA
          </h2>
        </div>
        {stateCopy ? <span className="game-detail-panel-meta">{stateCopy.label}</span> : null}
      </div>
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
          <p>{stateDescription}</p>
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
            <p>RetroFrontier will keep this provider choice until you forget it.</p>
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
      {normalized ? (
        <>
          {normalized.synopsis ? (
            <p className="game-detail-synopsis">{normalized.synopsis}</p>
          ) : null}
          <dl className="game-detail-metadata-list">
            {normalized.releaseDate ? (
              <div>
                <dt>RELEASE DATE</dt>
                <dd>
                  <time dateTime={normalized.releaseDate}>{normalized.releaseDate}</time>
                </dd>
              </div>
            ) : null}
            {normalized.developer ? (
              <div>
                <dt>DEVELOPER</dt>
                <dd>{normalized.developer}</dd>
              </div>
            ) : null}
            {normalized.publisher ? (
              <div>
                <dt>PUBLISHER</dt>
                <dd>{normalized.publisher}</dd>
              </div>
            ) : null}
            {normalized.genre ? (
              <div>
                <dt>GENRE</dt>
                <dd>{normalized.genre}</dd>
              </div>
            ) : null}
            {normalized.players ? (
              <div>
                <dt>PLAYERS</dt>
                <dd>{normalized.players}</dd>
              </div>
            ) : null}
            {normalized.region ? (
              <div>
                <dt>REGION</dt>
                <dd>{normalized.region}</dd>
              </div>
            ) : null}
          </dl>
          {metadataState?.metadata?.provenance.sourceCredit ? (
            <p className="game-detail-provenance">
              {userConfirmed ? 'USER-CONFIRMED MATCH · ' : ''}SOURCE CREDIT ·{' '}
              {metadataState.metadata.provenance.sourceCredit}
            </p>
          ) : null}
        </>
      ) : metadataState && !detail.metadataLoading ? (
        <p className="game-detail-empty-copy">No normalized metadata is available yet.</p>
      ) : null}
    </section>
  );
}

export function GameDetailPage({
  gameId,
  detail,
  systemStatus,
  readinessLoading = false,
  readinessError,
  onRetryReadiness,
  onBackToLibrary,
}: GameDetailPageProps) {
  const headingRef = useRef<HTMLHeadingElement>(null);
  const localDetail = detail.localDetail;
  const normalized = detail.metadata?.metadata?.metadata ?? null;
  const title = normalized?.title ?? localDetail?.localTitle ?? 'GAME DETAIL';
  const systemId = localDetail?.systemId ?? systemStatus?.id ?? null;
  const systemName =
    systemStatus?.displayName ?? (systemId ? formatSystemId(systemId) : 'SYSTEM UNKNOWN');
  const accent = systemAccent(systemId ?? 'unknown');
  const cover = detail.metadata?.cover;
  const coverRef = cover?.state === 'cached' ? cover.mediaRef : null;
  const notFound = gameId === null || (detail.localLoaded && !localDetail && !detail.localError);
  const localError = detail.localError;

  useEffect(() => {
    headingRef.current?.focus();
  }, [gameId, notFound]);

  return (
    <main className="app-main game-detail-main" id="game-detail-main">
      <BackToLibrary onBackToLibrary={onBackToLibrary} />
      <h1 className="game-detail-title" ref={headingRef} tabIndex={-1}>
        {gameId === null ? 'INVALID GAME LINK' : notFound ? 'GAME NOT FOUND' : title}
      </h1>

      {notFound ? (
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
          <section className="game-detail-hero" aria-labelledby="game-hero-heading">
            <div className="game-detail-cover-frame">
              <GameCover
                accent={accent}
                className="game-detail-cover"
                coverRef={coverRef}
                placeholderClassName="game-detail-placeholder"
                resetKey={cover ?? detail.metadata ?? localDetail ?? detail}
                loading="eager"
                title={title}
              />
            </div>
            <div className="game-detail-hero-copy">
              <div className="game-detail-system-line">
                <span
                  className="game-detail-system"
                  style={{ '--system-accent': accent } as CSSProperties}
                >
                  {systemName}
                </span>
                {localDetail?.availability === 'unavailable' ? (
                  <span className="game-detail-availability game-detail-availability--unavailable">
                    LOCAL CONTENT MISSING
                  </span>
                ) : null}
              </div>
              <h2 id="game-hero-heading">{title}</h2>
              {localDetail ? (
                <p className="game-detail-local-identity">LOCAL TITLE · {localDetail.localTitle}</p>
              ) : null}
              {localDetail ? (
                <button
                  aria-label={
                    detail.favoritePending
                      ? `Updating favorite for ${title}`
                      : localDetail.favorite
                        ? `Remove ${title} from favorites`
                        : `Add ${title} to favorites`
                  }
                  aria-pressed={localDetail.favorite}
                  className="game-detail-favorite"
                  disabled={detail.favoritePending}
                  onClick={() => void detail.toggleFavorite()}
                  type="button"
                >
                  <PixelStar filled={localDetail.favorite} />
                  {localDetail.favorite ? 'FAVORITED' : 'ADD TO FAVORITES'}
                </button>
              ) : null}
              {detail.favoriteError ? (
                <p className="game-detail-action-error" role="alert">
                  FAVORITE NOT UPDATED · TRY AGAIN
                </p>
              ) : null}
            </div>
          </section>

          <MetadataSection detail={detail} />
          <ReadinessSection
            detail={localDetail}
            onRetryReadiness={onRetryReadiness}
            readinessLoading={readinessLoading}
            readinessError={readinessError}
            systemStatus={systemStatus}
          />
          {localDetail ? <LocalContentSection detail={localDetail} /> : null}
        </>
      )}
    </main>
  );
}
