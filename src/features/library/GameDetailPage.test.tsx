import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { GameMetadataState, LibraryGameDetail, SystemStatus } from '../../platform/ipc';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import { GameDetailPage } from './GameDetailPage';

const localDetail: LibraryGameDetail = {
  gameId: 7,
  systemId: 'playstation',
  localTitle: 'Ridge Racer Local',
  availability: 'available',
  favorite: false,
  contentUnits: [
    {
      unitId: 11,
      rootId: 2,
      kind: 'cueBin',
      localTitle: 'Ridge Racer Disc 1',
      primaryRelativePath: 'PlayStation/Ridge Racer (Disc 1).cue',
      fileCount: 4,
      availability: 'available',
    },
    {
      unitId: 12,
      rootId: 2,
      kind: 'm3u',
      localTitle: 'Ridge Racer Playlist',
      primaryRelativePath: 'PlayStation/Ridge Racer.m3u',
      fileCount: 2,
      availability: 'incomplete',
    },
  ],
};

const metadata: GameMetadataState = {
  gameId: 7,
  providerId: 'screenScraper',
  status: 'matched',
  matchType: 'deterministicSha1',
  deterministic: true,
  providerGameId: 'provider-id-hidden',
  providerRomId: 'rom-id-hidden',
  unsupportedReason: null,
  lastFailure: null,
  lastCheckedAt: 1,
  metadata: {
    metadata: {
      title: 'Ridge Racer',
      sortTitle: 'ridge racer',
      synopsis: 'A fast arcade racing game.',
      releaseDate: '1994-12-03',
      developer: 'Namco',
      publisher: 'Namco',
      genre: 'Racing',
      players: '1-2',
      region: 'US',
    },
    provenance: {
      providerId: 'screenScraper',
      providerGameId: 'provider-id-hidden',
      sourceCredit: 'ScreenScraper',
      fetchedAt: 1,
    },
  },
  cover: null,
  candidates: [],
  userSelection: null,
  jobs: [],
};

function systemStatus(overrides: Partial<SystemStatus> = {}): SystemStatus {
  return {
    id: 'playstation',
    displayName: 'PlayStation',
    manufacturer: 'Sony',
    aliases: ['PS1'],
    supportedExtensions: ['.cue', '.chd'],
    core: {
      policy: {
        defaultCoreId: 'pcsx_rearmed',
        approvedCoreIds: ['pcsx_rearmed'],
        decision: { kind: 'resolved' },
      },
      availability: {
        runtimeState: 'ready',
        availableCoreIds: ['pcsx_rearmed'],
        defaultCoreAvailable: true,
      },
    },
    bios: {
      policy: 'required',
      ready: true,
      requirements: [
        {
          requirementId: 'playstation-bios',
          systemId: 'playstation',
          required: true,
          state: 'presentValid',
          expectedFilenames: ['SCPH1001.BIN'],
          expectedSizeBytes: 524288,
          description: 'PlayStation BIOS',
          matchedFilename: 'SCPH1001.BIN',
          fileSizeBytes: 524288,
          sha256: 'must-not-render',
        },
      ],
    },
    readiness: { ready: true, reasons: [] },
    ...overrides,
  };
}

function detailModel(overrides: Partial<GameDetailModel> = {}): GameDetailModel {
  return {
    localDetail,
    metadata,
    localLoaded: true,
    metadataLoaded: true,
    localLoading: false,
    metadataLoading: false,
    localError: null,
    metadataError: null,
    favoritePending: false,
    favoriteError: null,
    metadataActionPending: false,
    metadataActionKind: null,
    metadataActionTarget: null,
    metadataActionError: null,
    refresh: vi.fn().mockResolvedValue(undefined),
    retryLocal: vi.fn().mockResolvedValue(undefined),
    retryMetadata: vi.fn().mockResolvedValue(undefined),
    requestMetadata: vi.fn().mockResolvedValue(undefined),
    refreshMetadata: vi.fn().mockResolvedValue(undefined),
    selectMetadataCandidate: vi.fn().mockResolvedValue(undefined),
    clearMetadataSelection: vi.fn().mockResolvedValue(undefined),
    toggleFavorite: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function renderDetail(
  model: GameDetailModel = detailModel(),
  status: SystemStatus | null = systemStatus(),
  readinessLoading = false,
) {
  return render(
    <GameDetailPage
      detail={model}
      gameId={7}
      onBackToLibrary={vi.fn()}
      onRetryReadiness={vi.fn()}
      readinessLoading={readinessLoading}
      readinessError={null}
      systemStatus={status}
    />,
  );
}

describe('GameDetailPage', () => {
  it('presents the bounded local detail, normalized metadata, content units, and readiness', () => {
    renderDetail();

    expect(screen.getAllByRole('heading', { level: 1 })).toHaveLength(1);
    expect(screen.getByRole('heading', { level: 1, name: 'Ridge Racer' })).toBeInTheDocument();
    expect(screen.getByText('PlayStation')).toBeInTheDocument();
    expect(screen.getByText('A fast arcade racing game.')).toBeInTheDocument();
    const metadataPanel = screen.getByRole('region', { name: /normalized metadata/i });
    expect(within(metadataPanel).getByRole('status')).toHaveTextContent('METADATA MATCHED');
    expect(screen.getAllByText('Namco')).toHaveLength(2);
    expect(screen.getByText('1994-12-03')).toBeInTheDocument();
    expect(
      screen.getByRole('img', { name: 'No cover available for Ridge Racer' }),
    ).toBeInTheDocument();

    const content = screen.getByRole('region', { name: /associated content/i });
    expect(within(content).getByText('2 CONTENT UNITS')).toBeInTheDocument();
    expect(within(content).getByText('CUE / BIN')).toBeInTheDocument();
    expect(within(content).getByText('M3U PLAYLIST')).toBeInTheDocument();
    expect(within(content).getByText('PlayStation/Ridge Racer.m3u')).toBeInTheDocument();
    expect(within(content).getAllByText('CONTENT ROOT #2')).toHaveLength(2);

    // The fixture mixes one available and one incomplete unit, so the overall claim must stay
    // negative even though the game-level availability is `available` and every system
    // requirement is satisfied.
    const readiness = screen.getByRole('region', { name: /emulation readiness/i });
    expect(within(readiness).getByText('INCOMPLETE CONTENT')).toBeInTheDocument();
    expect(
      within(readiness).queryByText('EMULATION REQUIREMENTS SATISFIED'),
    ).not.toBeInTheDocument();
    expect(within(readiness).getByText('PARTIALLY AVAILABLE')).toBeInTheDocument();
    expect(within(readiness).getAllByText('AVAILABLE')).toHaveLength(3);
    expect(within(readiness).queryByText('NOT REQUIRED')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /launch|play/i })).not.toBeInTheDocument();
    expect(screen.queryByText(/save state/i)).not.toBeInTheDocument();
    expect(screen.queryByText('provider-id-hidden')).not.toBeInTheDocument();
    expect(screen.queryByText('must-not-render')).not.toBeInTheDocument();
  });

  it('claims the approved positive readiness only when every content unit is available', () => {
    renderDetail(
      detailModel({
        localDetail: {
          ...localDetail,
          contentUnits: localDetail.contentUnits.map((unit) => ({
            ...unit,
            availability: 'available' as const,
          })),
        },
      }),
    );

    const readiness = screen.getByRole('region', { name: /emulation readiness/i });
    expect(within(readiness).getByText('EMULATION REQUIREMENTS SATISFIED')).toBeInTheDocument();
    expect(within(readiness).queryByText('EMULATION READY')).not.toBeInTheDocument();
    expect(within(readiness).queryByText('PARTIALLY AVAILABLE')).not.toBeInTheDocument();
    expect(within(readiness).getAllByText('AVAILABLE')).toHaveLength(4);
  });

  it('retains cached normalized metadata and cover while marking stale state', () => {
    const staleMetadata: GameMetadataState = {
      ...metadata,
      status: 'stale',
      cover: {
        gameId: 7,
        providerId: 'screenScraper',
        kind: 'cover',
        state: 'cached',
        providerMediaType: 'image/png',
        region: 'US',
        mediaRef: 'rfmedia://localhost/cover/7',
        contentType: 'image/png',
        sizeBytes: 120,
        contentSha256: 'hidden',
        providerCrc32: null,
        providerMd5: null,
        providerSha1: null,
        sourceCredit: 'ScreenScraper',
        lastFailure: null,
        fetchedAt: 1,
        updatedAt: 2,
      },
    };

    renderDetail(detailModel({ metadata: staleMetadata }));

    expect(screen.getAllByText('METADATA STALE')).toHaveLength(2);
    expect(screen.getByText('A fast arcade racing game.')).toBeInTheDocument();
    expect(screen.getByRole('img', { name: 'Cover art for Ridge Racer' })).toHaveAttribute(
      'src',
      'rfmedia://localhost/cover/7',
    );
    expect(screen.getByText(/while it is revalidated/i)).toBeInTheDocument();
  });

  it.each([
    ['pending', 'METADATA PENDING'],
    ['noMatch', 'NO METADATA MATCH'],
    ['ambiguous', 'MATCH REVIEW NEEDED'],
    ['deferred', 'METADATA DEFERRED'],
    ['failed', 'METADATA UNAVAILABLE'],
  ] as const)(
    'labels the normalized metadata state %s without changing local identity',
    (status, label) => {
      renderDetail(detailModel({ metadata: { ...metadata, status } }));

      expect(screen.getAllByText(label).length).toBeGreaterThanOrEqual(1);
      expect(screen.getByRole('heading', { level: 1, name: 'Ridge Racer' })).toBeInTheDocument();
      expect(screen.getByText(/Ridge Racer Local/)).toBeInTheDocument();
    },
  );

  it.each([
    ['pending', 'REQUEST METADATA'],
    ['matched', 'REFRESH METADATA'],
    ['stale', 'REVALIDATE METADATA'],
    ['noMatch', 'TRY METADATA AGAIN'],
    ['ambiguous', 'SEARCH AGAIN'],
    ['deferred', 'TRY METADATA AGAIN'],
    ['failed', 'RETRY METADATA'],
  ] as const)('offers the supported metadata action for %s', (status, label) => {
    renderDetail(detailModel({ metadata: { ...metadata, status } }));

    expect(screen.getByRole('button', { name: label })).toBeInTheDocument();
  });

  it('renders ordered provider candidates without inventing confidence or exposing provider IDs', () => {
    const selectMetadataCandidate = vi.fn().mockResolvedValue(undefined);
    const ambiguousMetadata: GameMetadataState = {
      ...metadata,
      status: 'ambiguous',
      metadata: null,
      providerGameId: null,
      providerRomId: null,
      candidates: [
        { providerGameId: 'candidate-a', title: 'Zelda Candidate', releaseDate: '1986-02-21' },
        { providerGameId: 'candidate-b', title: 'Another Zelda', releaseDate: null },
      ],
    };

    renderDetail(detailModel({ metadata: ambiguousMetadata, selectMetadataCandidate }));

    const list = screen.getByRole('list', { name: 'Metadata candidates' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    expect(
      within(list)
        .getAllByRole('heading', { level: 4 })
        .map((heading) => heading.textContent),
    ).toEqual(['Zelda Candidate', 'Another Zelda']);
    const metadataPanel = screen.getByRole('region', { name: /normalized metadata/i });
    expect(within(metadataPanel).getByRole('status')).toHaveTextContent(
      'Choose a provider candidate below, or search again without changing local content.',
    );
    expect(
      within(metadataPanel).queryByText(/no provider candidates are available/i),
    ).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'SEARCH AGAIN' })).toBeInTheDocument();
    expect(screen.queryByText(/%|confidence|score/i)).not.toBeInTheDocument();
    expect(screen.queryByText('candidate-a')).not.toBeInTheDocument();
    expect(screen.queryByText('candidate-b')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /select zelda candidate/i }));
    expect(selectMetadataCandidate).toHaveBeenCalledWith('candidate-a');
  });

  it.each([
    ['deferred', 'chdRepresentationUndefined', 'CHD content'] as const,
    ['failed', null, 'metadata could not be enriched'] as const,
  ])(
    'renders persisted candidates for %s state and keeps manual selection reachable',
    (status, unsupportedReason, stateCopy) => {
      const selectMetadataCandidate = vi.fn().mockResolvedValue(undefined);
      const state: GameMetadataState = {
        ...metadata,
        status,
        matchType: null,
        providerGameId: null,
        providerRomId: null,
        unsupportedReason,
        lastFailure: status === 'failed' ? 'transientServer' : null,
        metadata: null,
        candidates: [
          { providerGameId: 'candidate-a', title: 'Deferred Candidate', releaseDate: null },
          { providerGameId: 'candidate-b', title: 'Second Candidate', releaseDate: '1994-01-01' },
        ],
      };

      renderDetail(detailModel({ metadata: state, selectMetadataCandidate }));

      const list = screen.getByRole('list', { name: 'Metadata candidates' });
      expect(
        within(list)
          .getAllByRole('heading', { level: 4 })
          .map((heading) => heading.textContent),
      ).toEqual(['Deferred Candidate', 'Second Candidate']);
      expect(screen.getByText(new RegExp(stateCopy, 'i'))).toBeInTheDocument();
      if (unsupportedReason !== null) {
        expect(screen.queryByText(/provider work is deferred/i)).not.toBeInTheDocument();
      }
      expect(screen.queryByRole('button', { name: 'TRY METADATA AGAIN' })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: 'RETRY METADATA' })).not.toBeInTheDocument();
      expect(screen.queryByText(/%|confidence|score/i)).not.toBeInTheDocument();

      fireEvent.click(screen.getByRole('button', { name: /select second candidate/i }));
      expect(selectMetadataCandidate).toHaveBeenCalledWith('candidate-b');
    },
  );

  it('renders persisted candidates for a no-match state while keeping a fresh search available', () => {
    const noMatchMetadata: GameMetadataState = {
      ...metadata,
      status: 'noMatch',
      metadata: null,
      providerGameId: null,
      providerRomId: null,
      candidates: [
        { providerGameId: 'candidate-a', title: 'No-Match Candidate', releaseDate: null },
      ],
    };

    renderDetail(detailModel({ metadata: noMatchMetadata }));

    expect(screen.getByRole('list', { name: 'Metadata candidates' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'CHOOSE A METADATA MATCH' })).toBeInTheDocument();
    expect(screen.getByText('No-Match Candidate')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'TRY METADATA AGAIN' })).toBeInTheDocument();
    expect(
      screen.queryByText(/choose one of the ordered provider candidates/i),
    ).not.toBeInTheDocument();
  });

  it('shows truthful in-flight feedback while a candidate choice is being applied', () => {
    const ambiguousMetadata: GameMetadataState = {
      ...metadata,
      status: 'ambiguous',
      metadata: null,
      providerGameId: null,
      providerRomId: null,
      candidates: [{ providerGameId: 'candidate-a', title: 'Zelda Candidate', releaseDate: null }],
    };

    renderDetail(
      detailModel({
        metadata: ambiguousMetadata,
        metadataActionPending: true,
        metadataActionKind: 'select',
        metadataActionTarget: 'candidate-a',
      }),
    );

    expect(screen.getByRole('button', { name: /selecting zelda candidate/i })).toHaveTextContent(
      'SELECTING…',
    );
    const metadataPanel = screen.getByRole('region', { name: /normalized metadata/i });
    expect(within(metadataPanel).getAllByRole('status')).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ textContent: expect.stringContaining('SELECTING MATCH') }),
      ]),
    );
  });

  it('marks only the selected candidate as busy while the choice is pending', () => {
    const candidateMetadata: GameMetadataState = {
      ...metadata,
      status: 'ambiguous',
      metadata: null,
      providerGameId: null,
      providerRomId: null,
      candidates: [
        { providerGameId: 'candidate-a', title: 'First Candidate', releaseDate: null },
        { providerGameId: 'candidate-b', title: 'Second Candidate', releaseDate: null },
      ],
    };

    renderDetail(
      detailModel({
        metadata: candidateMetadata,
        metadataActionPending: true,
        metadataActionKind: 'select',
        metadataActionTarget: 'candidate-b',
      }),
    );

    const first = screen.getByRole('button', { name: 'Select First Candidate candidate 1' });
    const second = screen.getByRole('button', { name: 'Selecting Second Candidate candidate 2' });
    expect(first).toBeDisabled();
    expect(first).not.toHaveAttribute('aria-busy', 'true');
    expect(second).toBeDisabled();
    expect(second).toHaveAttribute('aria-busy', 'true');
  });

  it('returns focus to the metadata section after a metadata mutation settles', () => {
    const initial = detailModel();
    const view = renderDetail(initial);
    const heading = screen.getByRole('heading', { name: 'NORMALIZED METADATA' });

    view.rerender(
      <GameDetailPage
        detail={detailModel({ metadataActionPending: true })}
        gameId={7}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );
    view.rerender(
      <GameDetailPage
        detail={detailModel()}
        gameId={7}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );

    expect(document.activeElement).toBe(heading);
  });

  it('does not create a candidate picker when the backend returns no candidates', () => {
    renderDetail(
      detailModel({
        metadata: { ...metadata, status: 'ambiguous', candidates: [] },
      }),
    );

    const metadataPanel = screen.getByRole('region', { name: /normalized metadata/i });
    expect(
      within(metadataPanel).getAllByText(/no provider candidates are available/i),
    ).toHaveLength(1);
    expect(
      within(metadataPanel).queryByText(/choose a provider candidate below/i),
    ).not.toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Metadata candidates' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /select candidate/i })).not.toBeInTheDocument();
  });

  it('keeps a retry path for an unsupported deferred state with no candidates', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'deferred',
          unsupportedReason: 'chdRepresentationUndefined',
          candidates: [],
        },
      }),
    );

    expect(screen.getByRole('button', { name: 'TRY METADATA AGAIN' })).toBeInTheDocument();
    expect(screen.getByText(/try again to search for provider candidates/i)).toBeInTheDocument();
    expect(screen.queryByText(/choose a provider candidate below/i)).not.toBeInTheDocument();
    expect(screen.queryByRole('list', { name: 'Metadata candidates' })).not.toBeInTheDocument();
  });

  it('restores the metadata action when an authoritative live job disappears', () => {
    const liveJob = {
      id: 3,
      gameId: 7,
      providerId: 'screenScraper' as const,
      kind: 'refreshMetadata' as const,
      state: 'running' as const,
      priority: 200,
      attempts: 1,
      lastFailure: null,
      earliestNextAttemptAt: null,
      claimedAt: 100,
      createdAt: 100,
      updatedAt: 100,
    };
    const view = renderDetail(detailModel({ metadata: { ...metadata, jobs: [liveJob] } }));

    expect(screen.queryByRole('button', { name: 'REFRESH METADATA' })).not.toBeInTheDocument();

    view.rerender(
      <GameDetailPage
        detail={detailModel({ metadata: { ...metadata, jobs: [] } })}
        gameId={7}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );

    expect(screen.getByRole('button', { name: 'REFRESH METADATA' })).toBeInTheDocument();
  });

  it('keeps a genuinely unmapped system actionless when no candidates exist', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'deferred',
          unsupportedReason: 'systemNotMapped',
          candidates: [],
        },
      }),
    );

    expect(screen.queryByRole('button', { name: 'TRY METADATA AGAIN' })).not.toBeInTheDocument();
    expect(
      screen.getByText(/system is not mapped to the current metadata provider/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/choose a provider candidate below/i)).not.toBeInTheDocument();
  });

  it('does not present a generic retry for a permanent metadata failure', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'failed',
          metadata: null,
          lastFailure: 'userAuthenticationFailed',
        },
      }),
    );

    expect(screen.queryByRole('button', { name: 'RETRY METADATA' })).not.toBeInTheDocument();
    expect(
      screen.getByText(/optional personal provider account needs attention/i),
    ).toBeInTheDocument();
  });

  it('keeps readiness rows truthful while the system snapshot is checking', () => {
    renderDetail(detailModel(), systemStatus(), true);

    const readiness = screen.getByRole('region', { name: /emulation readiness/i });
    expect(within(readiness).getAllByText('CHECKING')).toHaveLength(3);
    expect(within(readiness).getByText('PARTIALLY AVAILABLE')).toBeInTheDocument();
    expect(within(readiness).queryByText('UNAVAILABLE')).not.toBeInTheDocument();
  });

  it.each(['deferred', 'failed'] as const)(
    'does not create an empty candidate picker for %s state',
    (status) => {
      renderDetail(detailModel({ metadata: { ...metadata, status, candidates: [] } }));

      expect(screen.queryByRole('list', { name: 'Metadata candidates' })).not.toBeInTheDocument();
      expect(screen.queryByRole('button', { name: /select candidate/i })).not.toBeInTheDocument();
    },
  );

  it('renders persisted candidates for stale state while keeping revalidation available', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'stale',
          candidates: [
            { providerGameId: 'stale-a', title: 'Stale Candidate A', releaseDate: null },
            { providerGameId: 'stale-b', title: 'Stale Candidate B', releaseDate: '1995-01-01' },
          ],
        },
      }),
    );

    expect(screen.getByRole('list', { name: 'Metadata candidates' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'CHOOSE A METADATA MATCH' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'REVALIDATE METADATA' })).toBeInTheDocument();
    expect(screen.getByText('Stale Candidate A')).toBeInTheDocument();
    expect(screen.getByText('Stale Candidate B')).toBeInTheDocument();
    expect(screen.queryByText(/confidence|score|%/i)).not.toBeInTheDocument();
  });

  it('does not render historical candidate rows after an accepted match', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          candidates: [
            { providerGameId: 'historical-a', title: 'Historical Candidate', releaseDate: null },
          ],
        },
      }),
    );

    expect(screen.queryByRole('list', { name: 'Metadata candidates' })).not.toBeInTheDocument();
    expect(screen.queryByText('Historical Candidate')).not.toBeInTheDocument();
  });

  it('represents an existing user selection and clears only that provider choice', () => {
    const clearMetadataSelection = vi.fn().mockResolvedValue(undefined);
    const selectedMetadata: GameMetadataState = {
      ...metadata,
      matchType: 'heuristicUserConfirmed',
      userSelection: {
        gameId: 7,
        providerId: 'screenScraper',
        providerGameId: 'provider-id-hidden',
        updatedAt: 4,
      },
    };

    renderDetail(detailModel({ metadata: selectedMetadata, clearMetadataSelection }));

    expect(screen.getByText('USER-CONFIRMED MATCH')).toBeInTheDocument();
    expect(screen.getByText(/forget.*provider choice/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'FORGET PROVIDER CHOICE' }));
    expect(clearMetadataSelection).toHaveBeenCalledTimes(1);
    expect(screen.queryByText('provider-id-hidden')).not.toBeInTheDocument();
  });

  it('keeps cached metadata visible and redacts metadata action failures', () => {
    const actionError = {
      code: 'metadata_unavailable',
      message: 'https://provider.invalid?password=fake-secret-never-render',
    } as never;

    renderDetail(detailModel({ metadataActionError: actionError }));

    expect(screen.getByText('A fast arcade racing game.')).toBeInTheDocument();
    expect(screen.getByText('METADATA ACTION FAILED')).toBeInTheDocument();
    expect(screen.getByText(/cached metadata remains unchanged/i)).toBeInTheDocument();
    expect(
      screen.queryByText(/provider\.invalid|fake-secret-never-render/i),
    ).not.toBeInTheDocument();
  });

  it('keeps local detail visible when metadata fails and exposes an independent retry', () => {
    const retryMetadata = vi.fn().mockResolvedValue(undefined);
    renderDetail(
      detailModel({
        metadata: null,
        metadataError: { code: 'metadata_unavailable', message: 'Metadata unavailable.' } as never,
        retryMetadata,
      }),
    );

    expect(
      screen.getByRole('heading', { level: 1, name: 'Ridge Racer Local' }),
    ).toBeInTheDocument();
    expect(screen.getByText('METADATA UNAVAILABLE')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'RETRY METADATA' }));
    expect(retryMetadata).toHaveBeenCalledTimes(1);
  });

  it('keeps metadata visible when local detail fails and offers a primary local retry', () => {
    const retryLocal = vi.fn().mockResolvedValue(undefined);
    renderDetail(
      detailModel({
        localDetail: null,
        localError: { code: 'library_unavailable', message: 'Library unavailable.' } as never,
        retryLocal,
      }),
    );

    expect(screen.getByRole('heading', { level: 1, name: 'Ridge Racer' })).toBeInTheDocument();
    expect(screen.getByText('GAME DETAIL UNAVAILABLE')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'RETRY LOCAL DETAIL' }));
    expect(retryLocal).toHaveBeenCalledTimes(1);
  });

  it('shows truthful missing-content and dependency states separately', () => {
    const missingStatus = systemStatus({
      core: {
        policy: {
          defaultCoreId: 'pcsx_rearmed',
          approvedCoreIds: ['pcsx_rearmed'],
          decision: { kind: 'resolved' },
        },
        availability: {
          runtimeState: 'notInstalled',
          availableCoreIds: [],
          defaultCoreAvailable: false,
        },
      },
      bios: {
        policy: 'required',
        ready: false,
        requirements: [
          {
            requirementId: 'playstation-bios',
            systemId: 'playstation',
            required: true,
            state: 'missing',
            expectedFilenames: ['SCPH1001.BIN'],
            expectedSizeBytes: 524288,
            description: 'PlayStation BIOS',
            matchedFilename: null,
            fileSizeBytes: null,
            sha256: null,
          },
        ],
      },
      readiness: {
        ready: false,
        reasons: [
          { kind: 'runtimeUnavailable', state: 'notInstalled' },
          { kind: 'missingCore', coreId: 'pcsx_rearmed' },
          { kind: 'missingRequiredBios', requirementId: 'playstation-bios' },
        ],
      },
    });
    const unavailableGame = { ...localDetail, availability: 'unavailable' as const };

    renderDetail(detailModel({ localDetail: unavailableGame }), missingStatus);

    const readiness = screen.getByRole('region', { name: /emulation readiness/i });
    expect(within(readiness).getByText('MISSING CONTENT')).toBeInTheDocument();
    expect(within(readiness).getByText('RUNTIME')).toBeInTheDocument();
    expect(within(readiness).getByText('CORE')).toBeInTheDocument();
    expect(within(readiness).getByText('BIOS')).toBeInTheDocument();
    expect(within(readiness).getAllByText('MISSING').length).toBeGreaterThanOrEqual(1);
    expect(within(readiness).getByText('UNAVAILABLE')).toBeInTheDocument();
  });

  it('keeps the page usable when the readiness snapshot fails and offers a bounded retry', () => {
    const retryReadiness = vi.fn();
    render(
      <GameDetailPage
        detail={detailModel()}
        gameId={7}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={retryReadiness}
        readinessError={{ code: 'runtime_unavailable', message: 'Runtime unavailable.' } as never}
        systemStatus={null}
      />,
    );

    expect(screen.getByRole('heading', { level: 1, name: 'Ridge Racer' })).toBeInTheDocument();
    expect(screen.getByText('READINESS UNAVAILABLE')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'RETRY READINESS' }));
    expect(retryReadiness).toHaveBeenCalledTimes(1);
  });

  it('shows not-found state with semantic Library back navigation', () => {
    const onBackToLibrary = vi.fn();
    render(
      <GameDetailPage
        detail={detailModel({ localDetail: null })}
        gameId={7}
        onBackToLibrary={onBackToLibrary}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );

    expect(screen.getByRole('heading', { level: 1, name: 'GAME NOT FOUND' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('link', { name: /back to library/i }));
    expect(onBackToLibrary).toHaveBeenCalledTimes(1);
  });
});
