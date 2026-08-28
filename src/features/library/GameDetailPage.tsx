import { useEffect, useRef, type CSSProperties } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelArrow } from '../../components/ui/PixelIcon';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type {
  GameMetadataState,
  IpcError,
  LibraryGameDetail,
  SystemStatus,
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

const METADATA_STATE_COPY: Record<
  GameMetadataState['status'],
  { label: string; description: string }
> = {
  pending: {
    label: 'METADATA PENDING',
    description: 'Metadata lookup is still in progress. Local game information remains available.',
  },
  matched: {
    label: 'METADATA MATCHED',
    description: 'Normalized metadata is associated with this local game.',
  },
  noMatch: {
    label: 'NO METADATA MATCH',
    description: 'No provider match is available. This local game remains usable in the library.',
  },
  ambiguous: {
    label: 'MATCH REVIEW NEEDED',
    description: 'More than one metadata match needs review. No candidate was chosen here.',
  },
  deferred: {
    label: 'METADATA DEFERRED',
    description: 'Metadata lookup is deferred. Try again later from the metadata workflow.',
  },
  failed: {
    label: 'METADATA UNAVAILABLE',
    description: 'Metadata could not be enriched. Local game information remains available.',
  },
  stale: {
    label: 'METADATA STALE',
    description: 'Showing last-known-good metadata; revalidation is needed.',
  },
};

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
  const overall = readinessLoading
    ? {
        tone: 'unknown' as const,
        label: 'CHECKING READINESS',
        detail: 'Reading the current runtime, core, and BIOS snapshot…',
      }
    : getOverallReadiness(availability, systemStatus);
  const rows = getReadinessRows(availability, systemStatus);

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

function MetadataSection({ detail }: { detail: GameDetailModel }) {
  const metadataState = detail.metadata;
  const normalized = metadataState?.metadata?.metadata ?? null;
  const stateCopy = metadataState ? METADATA_STATE_COPY[metadataState.status] : null;

  return (
    <section aria-labelledby="metadata-heading" className="game-detail-panel game-detail-metadata">
      <div className="game-detail-panel-heading">
        <div>
          <span className="game-detail-kicker">ENRICHMENT</span>
          <h2 id="metadata-heading">NORMALIZED METADATA</h2>
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
      {detail.metadataLoading && !metadataState ? (
        <p className="game-detail-inline-status" role="status">
          READING NORMALIZED METADATA…
        </p>
      ) : null}
      {stateCopy ? (
        <div className={`metadata-state-callout metadata-state-callout--${metadataState?.status}`}>
          <strong>{stateCopy.label}</strong>
          <p>{stateCopy.description}</p>
        </div>
      ) : null}
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
              SOURCE CREDIT · {metadataState.metadata.provenance.sourceCredit}
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
