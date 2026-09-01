import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { ControllerFooter } from '../../components/ui/ControllerFooter';
import { FocusProvider } from '../../focus/FocusProvider';
import { useFocusApi } from '../../focus/focusContext';
import { installRectStub, layoutColumn } from '../../test/geometry';
import type {
  MetadataScrapeProgress,
  MetadataScrapeRunStatus,
  MetadataScrapeStatus,
} from '../../platform/ipc';
import { LibraryScraperPanel, SCRAPE_CONFIRMATION_THRESHOLD } from './LibraryScraperPanel';
import { useMetadataScrape } from '../../hooks/useMetadataScrape';

const mocks = vi.hoisted(() => ({
  getMetadataScrapeStatus: vi.fn(),
  previewMetadataScrape: vi.fn(),
  startMetadataScrape: vi.fn(),
  stopMetadataScrape: vi.fn(),
}));

vi.mock('../../platform/ipc', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../platform/ipc')>();
  return {
    ...actual,
    getMetadataScrapeStatus: mocks.getMetadataScrapeStatus,
    previewMetadataScrape: mocks.previewMetadataScrape,
    startMetadataScrape: mocks.startMetadataScrape,
    stopMetadataScrape: mocks.stopMetadataScrape,
  };
});

function progress(overrides: Partial<MetadataScrapeProgress> = {}): MetadataScrapeProgress {
  return {
    totalGames: 148,
    matched: 0,
    needsReview: 0,
    noMatch: 0,
    unsupported: 0,
    failed: 0,
    running: 0,
    waiting: 148,
    ...overrides,
  };
}

function status(
  runStatus: MetadataScrapeRunStatus | null,
  overrides: Partial<MetadataScrapeProgress> = {},
): MetadataScrapeStatus {
  if (runStatus === null) return { providerId: 'screenScraper', run: null, active: false };
  return {
    providerId: 'screenScraper',
    active: runStatus === 'preparing' || runStatus === 'running' || runStatus === 'stopping',
    run: {
      id: 1,
      providerId: 'screenScraper',
      mode: 'missingMetadata',
      status: runStatus,
      progress: progress(overrides),
      createdAt: 10,
      updatedAt: 10,
      finishedAt: runStatus === 'completed' || runStatus === 'stopped' ? 20 : null,
    },
  };
}

function Dispatcher() {
  const api = useFocusApi();
  return (
    <>
      <button
        aria-hidden="true"
        data-testid="dispatch-back"
        onClick={() => api.dispatch('back', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-confirm"
        onClick={() => api.dispatch('confirm', 'gamepad')}
        type="button"
      />
      <button
        aria-hidden="true"
        data-testid="dispatch-down"
        onClick={() => api.dispatch('moveDown', 'gamepad')}
        type="button"
      />
    </>
  );
}

function Harness({
  onReviewMatches,
  providerWaiting,
}: {
  onReviewMatches: () => void;
  providerWaiting: boolean;
}) {
  const scrape = useMetadataScrape();
  return (
    <LibraryScraperPanel
      onReviewMatches={onReviewMatches}
      providerWaiting={providerWaiting}
      scrape={scrape}
    />
  );
}

function renderPanel(onReviewMatches = vi.fn(), providerWaiting = false) {
  const result = render(
    <FocusProvider>
      <Dispatcher />
      <Harness onReviewMatches={onReviewMatches} providerWaiting={providerWaiting} />
      <ControllerFooter controllerConnected gameRunning={false} interactive status="SETTINGS" />
    </FocusProvider>,
  );
  return { ...result, onReviewMatches };
}

function send(action: 'back' | 'confirm' | 'down') {
  act(() => {
    fireEvent.click(screen.getByTestId(`dispatch-${action}`));
  });
}

beforeEach(() => {
  installRectStub();
  mocks.getMetadataScrapeStatus.mockReset().mockResolvedValue(status(null));
  mocks.previewMetadataScrape
    .mockReset()
    .mockImplementation(async ({ mode }: { mode: string }) => ({
      mode,
      eligibleGames: mode === 'missingMetadata' ? 148 : 7,
    }));
  mocks.startMetadataScrape.mockReset().mockResolvedValue(status('running'));
  mocks.stopMetadataScrape.mockReset().mockResolvedValue(status('stopped'));
});

describe('LibraryScraperPanel', () => {
  it('offers both modes and re-counts eligibility when the mode changes', async () => {
    renderPanel();

    await screen.findByText('148 GAMES ELIGIBLE');
    expect(screen.getByRole('radio', { name: /MISSING METADATA/ })).toBeChecked();

    fireEvent.click(screen.getByRole('radio', { name: /REFRESH MATCHED GAMES/ }));

    await screen.findByText('7 GAMES ELIGIBLE');
    expect(screen.getByRole('radio', { name: /REFRESH MATCHED GAMES/ })).toBeChecked();
  });

  it('starts a small run without a confirmation', async () => {
    mocks.previewMetadataScrape.mockResolvedValue({ mode: 'missingMetadata', eligibleGames: 12 });
    renderPanel();
    await screen.findByText('12 GAMES ELIGIBLE');

    fireEvent.click(screen.getByRole('button', { name: 'START SCRAPER' }));

    await waitFor(() =>
      expect(mocks.startMetadataScrape).toHaveBeenCalledWith({ mode: 'missingMetadata' }),
    );
    await screen.findByText('SCRAPER RUNNING');
  });

  it('confirms before committing a large library to a metered provider', async () => {
    mocks.previewMetadataScrape.mockResolvedValue({
      mode: 'missingMetadata',
      eligibleGames: SCRAPE_CONFIRMATION_THRESHOLD,
    });
    renderPanel();
    await screen.findByText(`${SCRAPE_CONFIRMATION_THRESHOLD} GAMES ELIGIBLE`);

    fireEvent.click(screen.getByRole('button', { name: 'START SCRAPER' }));

    const confirmation = await screen.findByRole('alertdialog');
    expect(mocks.startMetadataScrape).not.toHaveBeenCalled();

    fireEvent.click(within(confirmation).getByRole('button', { name: 'START SCRAPER' }));
    await waitFor(() => expect(mocks.startMetadataScrape).toHaveBeenCalledTimes(1));
  });

  it('reports run progress in games and never counts a waiting game as processed', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('running', {
        matched: 31,
        needsReview: 6,
        noMatch: 5,
        unsupported: 2,
        failed: 3,
        running: 2,
        waiting: 99,
      }),
    );
    renderPanel();

    // 31 + 6 + 5 + 2 + 3 = 47 processed. The two in flight and the ninety-nine waiting are not.
    await screen.findByText('47 / 148');

    const results = screen.getByText('MATCHED').closest('dl');
    expect(results).not.toBeNull();
    for (const [label, value] of [
      ['MATCHED', '31'],
      ['NEEDS REVIEW', '6'],
      ['NO MATCH', '5'],
      ['UNSUPPORTED', '2'],
      ['FAILED', '3'],
    ]) {
      expect(within(results as HTMLElement).getByText(label)).toBeInTheDocument();
      expect(within(results as HTMLElement).getByText(value)).toBeInTheDocument();
    }
    const pending = screen.getByText('WAITING').closest('dl');
    expect(pending).not.toBeNull();
    expect(within(pending as HTMLElement).getByText('RUNNING')).toBeInTheDocument();
    expect(within(pending as HTMLElement).getByText('2')).toBeInTheDocument();
    expect(within(pending as HTMLElement).getByText('99')).toBeInTheDocument();
  });

  it('never infers a provider wait from a run that simply has nothing in flight', async () => {
    // A run that has only just started also has nothing running. Calling that "waiting for provider
    // capacity" would state something RetroFrontier has not been told.
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('running', { running: 0, waiting: 148 }),
    );
    renderPanel();

    await screen.findByText('SCRAPER RUNNING');
    expect(screen.queryByText('WAITING FOR PROVIDER CAPACITY')).not.toBeInTheDocument();
  });

  it('describes a real provider wait truthfully and predicts no reset time', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('running', { running: 0, waiting: 148 }),
    );
    const { container } = renderPanel(vi.fn(), true);

    await screen.findByText('WAITING FOR PROVIDER CAPACITY');
    const rendered = container.textContent ?? '';
    expect(rendered).toContain('when capacity returns');
    expect(rendered).not.toMatch(/quota resets/i);
    expect(rendered).not.toMatch(/\bETA\b/);
    expect(rendered).not.toMatch(/remaining time|estimated/i);
  });

  it('adopts a run that was already in progress before the screen opened', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running', { matched: 31 }));
    renderPanel();

    await screen.findByText('SCRAPER RUNNING');
    expect(screen.getByRole('button', { name: 'STOP SCRAPER' })).toBeInTheDocument();
    expect(mocks.startMetadataScrape).not.toHaveBeenCalled();
  });

  it('confirms a stop and keeps the run when the confirmation is dismissed', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running'));
    renderPanel();
    await screen.findByText('SCRAPER RUNNING');

    fireEvent.click(screen.getByRole('button', { name: 'STOP SCRAPER' }));
    const confirmation = await screen.findByRole('alertdialog');

    fireEvent.click(within(confirmation).getByRole('button', { name: 'CANCEL' }));
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
    expect(mocks.stopMetadataScrape).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole('button', { name: 'STOP SCRAPER' }));
    const reopened = await screen.findByRole('alertdialog');
    fireEvent.click(within(reopened).getByRole('button', { name: 'STOP SCRAPER' }));

    await waitFor(() => expect(mocks.stopMetadataScrape).toHaveBeenCalledTimes(1));
    await screen.findByText('SCRAPE STOPPED');
  });

  it('offers a review route out of a completed run and lets the summary be dismissed', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(
      status('completed', {
        matched: 119,
        needsReview: 14,
        noMatch: 10,
        unsupported: 3,
        failed: 2,
        running: 0,
        waiting: 0,
      }),
    );
    const { onReviewMatches } = renderPanel();

    await screen.findByText('SCRAPE COMPLETE');
    expect(screen.getByText('148 / 148')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'REVIEW MATCHES' }));
    expect(onReviewMatches).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: 'DONE' }));
    await screen.findByRole('button', { name: 'START SCRAPER' });
    expect(screen.queryByText('SCRAPE COMPLETE')).not.toBeInTheDocument();
  });

  it('surfaces a status read failure without claiming a run state', async () => {
    mocks.getMetadataScrapeStatus.mockRejectedValue(new Error('unavailable'));
    const { container } = renderPanel();

    await screen.findByText('SCRAPER STATUS UNAVAILABLE');
    expect(container.textContent).not.toContain('SCRAPER RUNNING');
  });
});

describe('LibraryScraperPanel controller navigation', () => {
  it('reaches both modes and the start action with the controller', async () => {
    renderPanel();
    await screen.findByText('148 GAMES ELIGIBLE');

    // jsdom performs no layout, so the stack this panel renders has to be described explicitly for
    // geometry-derived navigation to have anything to move through.
    layoutColumn([
      screen.getByRole('radio', { name: /MISSING METADATA/ }),
      screen.getByRole('radio', { name: /REFRESH MATCHED GAMES/ }),
      screen.getByRole('button', { name: 'START SCRAPER' }),
    ]);

    act(() => screen.getByRole('radio', { name: /MISSING METADATA/ }).focus());
    send('down');
    expect(screen.getByRole('radio', { name: /REFRESH MATCHED GAMES/ })).toHaveFocus();

    send('confirm');
    await screen.findByText('7 GAMES ELIGIBLE');

    layoutColumn([
      screen.getByRole('radio', { name: /MISSING METADATA/ }),
      screen.getByRole('radio', { name: /REFRESH MATCHED GAMES/ }),
      screen.getByRole('button', { name: 'START SCRAPER' }),
    ]);
    send('down');
    expect(screen.getByRole('button', { name: 'START SCRAPER' })).toHaveFocus();
  });

  it('traps focus in the stop confirmation and restores it on dismissal', async () => {
    mocks.getMetadataScrapeStatus.mockResolvedValue(status('running'));
    renderPanel();
    await screen.findByText('SCRAPER RUNNING');

    const stop = screen.getByRole('button', { name: 'STOP SCRAPER' });
    act(() => stop.focus());
    send('confirm');

    const confirmation = await screen.findByRole('alertdialog');
    await waitFor(() =>
      expect(within(confirmation).getByRole('button', { name: 'STOP SCRAPER' })).toHaveFocus(),
    );

    // `back` dismisses the scope rather than leaving Settings.
    send('back');
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument());
    await waitFor(() => expect(screen.getByRole('button', { name: 'STOP SCRAPER' })).toHaveFocus());
    expect(mocks.stopMetadataScrape).not.toHaveBeenCalled();
  });

  it('does not strand focus when starting replaces the idle controls', async () => {
    mocks.previewMetadataScrape.mockResolvedValue({ mode: 'missingMetadata', eligibleGames: 12 });
    renderPanel();
    await screen.findByText('12 GAMES ELIGIBLE');

    const start = screen.getByRole('button', { name: 'START SCRAPER' });
    act(() => start.focus());
    send('confirm');

    await screen.findByText('SCRAPER RUNNING');
    // The button that had focus is gone; something focusable must still hold it.
    expect(document.activeElement).not.toBe(document.body);
    expect(screen.getByRole('button', { name: 'STOP SCRAPER' })).toBeInTheDocument();
  });
});
