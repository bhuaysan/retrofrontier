import { useEffect, useRef, useState } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { useFocusNode, useFocusScope } from '../../focus/focusContext';
import { focusNodes, focusScopes } from '../../focus/focusNodes';
import type { MetadataScrapeModel } from '../../hooks/useMetadataScrape';
import type { MetadataScrapeMode, MetadataScrapeRun } from '../../platform/ipc';
import { scrapeModeCopy, scrapeResultRows, scrapeRunCopy } from './scrapeStatus';

/**
 * Target size above which starting is confirmed first.
 *
 * A whole-library scrape of a few dozen games is an ordinary action. A run over thousands is a long
 * commitment against a metered provider budget, so it reuses the same confirmation-and-focus-scope
 * pattern the destructive Settings actions already use.
 */
export const SCRAPE_CONFIRMATION_THRESHOLD = 250;

interface LibraryScraperPanelProps {
  scrape: MetadataScrapeModel;
  /**
   * True only when the provider itself has a live deferral recorded.
   *
   * Passed in rather than inferred from the run: a run with nothing in flight has only just
   * started as often as it is blocked, and guessing between those two would put a claim on screen
   * that RetroFrontier cannot support.
   */
  providerWaiting: boolean;
  onReviewMatches: () => void;
}

function ModeOption({
  mode,
  selected,
  disabled,
  onSelect,
}: {
  mode: MetadataScrapeMode;
  selected: boolean;
  disabled: boolean;
  onSelect: () => void;
}) {
  const copy = scrapeModeCopy(mode);
  const optionRef = useFocusNode({
    id: focusNodes.settingsScrape(`mode-${mode}`),
    confirm: disabled ? null : { label: 'SELECT' },
  });

  return (
    <button
      aria-checked={selected}
      className={`scrape-mode${selected ? ' scrape-mode--selected' : ''}`}
      disabled={disabled}
      onClick={onSelect}
      ref={optionRef}
      role="radio"
      type="button"
    >
      <span aria-hidden="true" className="scrape-mode-marker" />
      <span className="scrape-mode-text">
        <strong>{copy.label}</strong>
        <span>{copy.description}</span>
      </span>
    </button>
  );
}

function ScrapeProgress({ run }: { run: MetadataScrapeRun }) {
  const { progress } = run;
  const processed =
    progress.matched +
    progress.needsReview +
    progress.noMatch +
    progress.unsupported +
    progress.failed;

  return (
    <div className="scrape-progress">
      <p aria-live="polite" className="scrape-progress-count" role="status">
        <strong>{`${processed} / ${progress.totalGames}`}</strong> PROCESSED
      </p>
      <dl className="scrape-progress-results">
        {scrapeResultRows(progress).map((row) => (
          <div key={row.label}>
            <dt>{row.label}</dt>
            <dd>{row.value}</dd>
          </div>
        ))}
      </dl>
      <dl className="scrape-progress-pending">
        <div>
          <dt>RUNNING</dt>
          <dd>{progress.running}</dd>
        </div>
        <div>
          <dt>WAITING</dt>
          <dd>{progress.waiting}</dd>
        </div>
      </dl>
    </div>
  );
}

export function LibraryScraperPanel({
  scrape,
  providerWaiting,
  onReviewMatches,
}: LibraryScraperPanelProps) {
  const [confirming, setConfirming] = useState<'start' | 'stop' | null>(null);
  const [dismissedRunId, setDismissedRunId] = useState<number | null>(null);
  const confirmationButton = useRef<HTMLButtonElement>(null);
  const primaryAction = useRef<HTMLButtonElement>(null);
  const returnFocusToPrimary = useRef(false);
  const heading = useRef<HTMLHeadingElement>(null);

  const run = scrape.status?.run ?? null;
  const active = scrape.active;
  const busy = scrape.actionPending;
  const finishedRun = run !== null && !active ? run : null;
  // Dismissal is keyed by the run it dismissed, so the next run's summary appears on its own. No
  // effect has to reset it when a run starts.
  const showSummary = finishedRun !== null && finishedRun.id !== dismissedRunId;
  const eligible = scrape.eligibleGames;
  const canStart = !active && !busy && eligible !== null && eligible > 0;

  // The confirmation owns focus and `back` while it is open; entry and exit focus are handled by the
  // effect below so a controller never lands on a button that has just been unmounted.
  const confirmScopeRef = useFocusScope({
    id: focusScopes.metadataScrapeStop,
    dismissLabel: 'CANCEL',
    initialFocus: 'none',
    restore: 'none',
    onDismiss: () => {
      if (busy) return;
      returnFocusToPrimary.current = true;
      setConfirming(null);
    },
  });

  const startRef = useFocusNode({
    id: focusNodes.settingsScrape('start'),
    confirm: canStart ? { label: 'START SCRAPER' } : null,
  });
  const stopRef = useFocusNode({
    id: focusNodes.settingsScrape('stop'),
    confirm: active && !busy ? { label: 'STOP SCRAPER' } : null,
  });
  const reviewRef = useFocusNode({
    id: focusNodes.settingsScrape('review'),
    confirm: { label: 'REVIEW MATCHES' },
  });
  const doneRef = useFocusNode({
    id: focusNodes.settingsScrape('done'),
    confirm: { label: 'DONE' },
  });
  const confirmRef = useFocusNode({
    id: focusNodes.settingsScrape('confirm'),
    confirm: busy ? null : { label: confirming === 'stop' ? 'STOP SCRAPER' : 'START SCRAPER' },
  });
  const cancelRef = useFocusNode({
    id: focusNodes.settingsScrape('cancel'),
    confirm: busy ? null : { label: 'CANCEL' },
  });

  useEffect(() => {
    if (confirming !== null) {
      confirmationButton.current?.focus();
    } else if (returnFocusToPrimary.current) {
      returnFocusToPrimary.current = false;
      // The button that opened the confirmation may have been replaced by the other primary action
      // if the run's state changed underneath it, so the heading is the guaranteed landing place.
      (primaryAction.current ?? heading.current)?.focus();
    }
  }, [confirming]);

  const beginStart = () => {
    if (!canStart) return;
    if (eligible !== null && eligible >= SCRAPE_CONFIRMATION_THRESHOLD) {
      setConfirming('start');
      return;
    }
    void scrape.start();
  };

  const commitConfirmation = async () => {
    const pending = confirming;
    if (pending === null) return;
    const committed = pending === 'stop' ? await scrape.stop() : await scrape.start();
    if (committed) {
      returnFocusToPrimary.current = true;
      setConfirming(null);
    }
  };

  const runCopy = run ? scrapeRunCopy(run, providerWaiting) : null;

  return (
    <section aria-labelledby="scrape-heading" className="metadata-scrape-section">
      <div className="metadata-row-heading">
        <h3 id="scrape-heading" ref={heading} tabIndex={-1}>
          LIBRARY SCRAPER
        </h3>
        <span className="panel-meta">{active ? 'RUNNING' : 'IDLE'}</span>
      </div>

      <p className="metadata-account-help" id="scrape-help">
        A library scan finds games on this machine. Fetching metadata for them is a separate step
        you start here.
      </p>

      {scrape.statusError ? (
        <InlineError
          title="SCRAPER STATUS UNAVAILABLE"
          message="RetroFrontier could not read scraper status. Local content and cached metadata remain available."
          actionLabel="RETRY SCRAPER STATUS"
          onAction={() => void scrape.refresh()}
        />
      ) : null}
      {scrape.actionError ? (
        <InlineError
          title="SCRAPER ACTION FAILED"
          message="RetroFrontier could not change the scraper run. Nothing already scraped was lost."
          actionLabel="RETRY SCRAPER STATUS"
          onAction={() => void scrape.refresh()}
        />
      ) : null}

      {active && run ? (
        <div className="scrape-active">
          <p aria-live="polite" className="scrape-run-state" role="status">
            <strong>{runCopy?.label}</strong>
            <span>{runCopy?.description}</span>
          </p>
          <ScrapeProgress run={run} />
          {confirming === 'stop' ? (
            <div
              aria-labelledby="scrape-stop-title"
              aria-modal="false"
              className="scrape-confirmation"
              ref={confirmScopeRef}
              role="alertdialog"
              onKeyDown={(event) => {
                if (event.key === 'Escape' && !busy) {
                  event.preventDefault();
                  returnFocusToPrimary.current = true;
                  setConfirming(null);
                }
              }}
            >
              <span id="scrape-stop-title">
                Stop this scraper run? Metadata already fetched is kept, and the games it has not
                reached yet stay available for a later run.
              </span>
              <PixelButton
                disabled={busy}
                onClick={() => {
                  returnFocusToPrimary.current = true;
                  setConfirming(null);
                }}
                ref={cancelRef}
                type="button"
                variant="secondary"
              >
                CANCEL
              </PixelButton>
              <PixelButton
                disabled={busy}
                onClick={() => void commitConfirmation()}
                ref={(node) => {
                  confirmationButton.current = node;
                  confirmRef(node);
                }}
                type="button"
              >
                STOP SCRAPER
              </PixelButton>
            </div>
          ) : (
            <div className="scrape-actions">
              <PixelButton
                disabled={busy}
                onClick={() => setConfirming('stop')}
                ref={(node) => {
                  primaryAction.current = node;
                  stopRef(node);
                }}
                type="button"
                variant="secondary"
              >
                STOP SCRAPER
              </PixelButton>
            </div>
          )}
        </div>
      ) : showSummary && finishedRun ? (
        <div className="scrape-summary">
          <p aria-live="polite" className="scrape-run-state" role="status">
            <strong>{runCopy?.label}</strong>
            <span>{runCopy?.description}</span>
          </p>
          <ScrapeProgress run={finishedRun} />
          <div className="scrape-actions">
            <PixelButton onClick={onReviewMatches} ref={reviewRef} type="button">
              REVIEW MATCHES
            </PixelButton>
            <PixelButton
              onClick={() => setDismissedRunId(finishedRun.id)}
              ref={(node) => {
                primaryAction.current = node;
                doneRef(node);
              }}
              type="button"
              variant="secondary"
            >
              DONE
            </PixelButton>
          </div>
        </div>
      ) : (
        <div className="scrape-idle">
          <div aria-labelledby="scrape-mode-label" className="scrape-modes" role="radiogroup">
            <span className="library-filter-label" id="scrape-mode-label">
              // SCRAPE
            </span>
            {(['missingMetadata', 'refreshMatched'] as const).map((mode) => (
              <ModeOption
                disabled={busy}
                key={mode}
                mode={mode}
                onSelect={() => scrape.setMode(mode)}
                selected={scrape.mode === mode}
              />
            ))}
          </div>

          <p aria-live="polite" className="scrape-eligible" role="status">
            {scrape.previewError
              ? 'ELIGIBLE GAMES UNAVAILABLE'
              : scrape.previewLoading && eligible === null
                ? 'COUNTING ELIGIBLE GAMES…'
                : `${eligible ?? 0} ${eligible === 1 ? 'GAME' : 'GAMES'} ELIGIBLE`}
          </p>

          {confirming === 'start' ? (
            <div
              aria-labelledby="scrape-start-title"
              aria-modal="false"
              className="scrape-confirmation"
              ref={confirmScopeRef}
              role="alertdialog"
              onKeyDown={(event) => {
                if (event.key === 'Escape' && !busy) {
                  event.preventDefault();
                  returnFocusToPrimary.current = true;
                  setConfirming(null);
                }
              }}
            >
              <span id="scrape-start-title">
                Scrape {eligible} games? This runs in the background against your ScreenScraper
                budget, and you can stop it at any time.
              </span>
              <PixelButton
                disabled={busy}
                onClick={() => {
                  returnFocusToPrimary.current = true;
                  setConfirming(null);
                }}
                ref={cancelRef}
                type="button"
                variant="secondary"
              >
                CANCEL
              </PixelButton>
              <PixelButton
                disabled={busy}
                onClick={() => void commitConfirmation()}
                ref={(node) => {
                  confirmationButton.current = node;
                  confirmRef(node);
                }}
                type="button"
              >
                START SCRAPER
              </PixelButton>
            </div>
          ) : (
            <div className="scrape-actions">
              <PixelButton
                aria-describedby="scrape-help"
                disabled={!canStart}
                onClick={beginStart}
                ref={(node) => {
                  primaryAction.current = node;
                  startRef(node);
                }}
                type="button"
              >
                START SCRAPER
              </PixelButton>
            </div>
          )}
        </div>
      )}
    </section>
  );
}
