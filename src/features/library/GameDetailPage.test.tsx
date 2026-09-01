import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { GameMetadataState, LibraryGameDetail, SystemStatus } from '../../platform/ipc';
import type { GameDetailModel } from '../../hooks/useGameDetail';
import type { GameLaunchModel } from '../../hooks/useGameLaunch';
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

function launchModel(overrides: Partial<GameLaunchModel> = {}): GameLaunchModel {
  return {
    phase: 'idle',
    running: null,
    blocked: false,
    pendingGameId: null,
    interaction: null,
    failure: null,
    contentOptions: null,
    diagnostics: [],
    launch: vi.fn().mockResolvedValue(undefined),
    dismissFailure: vi.fn(),
    cancelContentSelection: vi.fn(),
    abandonInteraction: vi.fn(),
    ...overrides,
  };
}

function renderDetail(
  model: GameDetailModel = detailModel(),
  status: SystemStatus | null = systemStatus(),
  readinessLoading = false,
  launch: GameLaunchModel = launchModel(),
) {
  return render(
    <GameDetailPage
      detail={model}
      gameId={7}
      launch={launch}
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
    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
    expect(within(metadataPanel).getByRole('status')).toHaveTextContent('METADATA MATCHED');
    expect(screen.getAllByText('Namco')).toHaveLength(2);
    expect(screen.getByText('1994-12-03')).toBeInTheDocument();
    expect(
      screen.getByRole('img', { name: 'No cover available for Ridge Racer' }),
    ).toBeInTheDocument();

    const content = screen.getByRole('region', { name: 'LOCAL CONTENT' });
    expect(within(content).getByText('2 CONTENT UNITS')).toBeInTheDocument();
    expect(within(content).getByText('CUE / BIN')).toBeInTheDocument();
    expect(within(content).getByText('M3U PLAYLIST')).toBeInTheDocument();
    expect(within(content).getByText('PlayStation/Ridge Racer.m3u')).toBeInTheDocument();
    expect(within(content).getAllByText('CONTENT ROOT #2')).toHaveLength(2);

    // The fixture mixes one available and one incomplete unit, so the overall claim must stay
    // negative even though the game-level availability is `available` and every system
    // requirement is satisfied.
    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
    expect(within(readiness).getByText('INCOMPLETE CONTENT')).toBeInTheDocument();
    expect(
      within(readiness).queryByText('EMULATION REQUIREMENTS SATISFIED'),
    ).not.toBeInTheDocument();
    expect(within(readiness).getByText('PARTIALLY AVAILABLE')).toBeInTheDocument();
    expect(within(readiness).getAllByText('AVAILABLE')).toHaveLength(3);
    expect(within(readiness).queryByText('NOT REQUIRED')).not.toBeInTheDocument();
    // M7 owns the Play action; save states and controller mapping remain later milestones.
    expect(screen.getByRole('button', { name: /^play /i })).toBeInTheDocument();
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

    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
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

    // The state truth is stated once, in the compact secondary metadata surface.
    expect(screen.getAllByText('METADATA STALE')).toHaveLength(1);
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
    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
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
    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
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
    const heading = screen.getByRole('heading', { name: 'METADATA' });

    view.rerender(
      <GameDetailPage
        detail={detailModel({ metadataActionPending: true })}
        launch={launchModel()}
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
        launch={launchModel()}
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

    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
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
        launch={launchModel()}
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

    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
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

    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
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
        launch={launchModel()}
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
        launch={launchModel()}
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

describe('GameDetailPage — B6 hero fidelity', () => {
  const cachedCover = {
    gameId: 7,
    providerId: 'screenScraper' as const,
    kind: 'cover' as const,
    state: 'cached' as const,
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
  };

  function hero() {
    const node = document.querySelector('.game-detail-hero');
    if (!(node instanceof HTMLElement)) throw new Error('hero not rendered');
    return node;
  }

  it('renders exactly one dominant game title, inside the hero', () => {
    renderDetail(detailModel({ metadata: { ...metadata, cover: cachedCover } }));

    const headings = screen.getAllByRole('heading', { level: 1 });
    expect(headings).toHaveLength(1);
    expect(headings[0]).toHaveAttribute('id', 'game-detail-title');
    expect(headings[0]).toHaveTextContent('Ridge Racer');
    expect(hero().contains(headings[0])).toBe(true);
    // The cover is a real image here, so the only remaining occurrence of the title inside the
    // hero must be the heading itself — no second dominant copy of the same identity.
    expect(within(hero()).getAllByText('Ridge Racer')).toHaveLength(1);
    expect(screen.getByRole('main')).toHaveAttribute('aria-labelledby', 'game-detail-title');
  });

  it('renders the game title before the identity chips in DOM order', () => {
    renderDetail();

    const heading = screen.getByRole('heading', { level: 1, name: 'Ridge Racer' });
    const chips = hero().querySelector('.game-detail-chips');
    expect(chips).not.toBeNull();
    // B6 leads with the title. DOM order must agree with the visual order rather than relying on
    // a CSS reorder, so the heading precedes the chip row in the document.
    expect(
      heading.compareDocumentPosition(chips as HTMLElement) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const heroChildren = Array.from((chips as HTMLElement).parentElement!.children);
    expect(heroChildren.indexOf(heading)).toBeLessThan(heroChildren.indexOf(chips as HTMLElement));
  });

  it('presents the compact system identity with the full catalog name still accessible', () => {
    renderDetail();

    const badge = within(hero()).getByTitle('PlayStation');
    expect(badge).toHaveTextContent('PS1');
    expect(within(badge).getByText('PlayStation')).toHaveClass('visually-hidden');
    expect(within(hero()).queryByText('Nintendo Entertainment System')).not.toBeInTheDocument();
  });

  it('places the favorite action in the cover action area and offers no launch action', () => {
    const toggleFavorite = vi.fn().mockResolvedValue(undefined);
    renderDetail(detailModel({ toggleFavorite }));

    const favorite = screen.getByRole('button', { name: 'Add Ridge Racer to favorites' });
    expect(favorite).toHaveAttribute('aria-pressed', 'false');
    expect(favorite.closest('.game-detail-cover-column')).not.toBeNull();
    expect(document.querySelector('.game-detail-cover-column')?.contains(favorite)).toBe(true);

    fireEvent.click(favorite);
    expect(toggleFavorite).toHaveBeenCalledTimes(1);

    expect(
      screen.queryByRole('button', { name: /^(start|play|launch|run game)$/i }),
    ).not.toBeInTheDocument();
    expect(screen.queryByText(/coming soon/i)).not.toBeInTheDocument();
  });

  it('renders genre and release-year chips only from real normalized metadata', () => {
    renderDetail();

    const chips = hero().querySelector('.game-detail-chips');
    expect(chips).not.toBeNull();
    expect(within(chips as HTMLElement).getByText('Racing')).toBeInTheDocument();
    expect(within(chips as HTMLElement).getByText('1994')).toBeInTheDocument();
  });

  it('omits optional hero chips instead of rendering placeholder values', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          metadata: {
            ...metadata.metadata!,
            metadata: {
              ...metadata.metadata!.metadata,
              genre: null,
              releaseDate: null,
              synopsis: null,
              developer: null,
              publisher: null,
              players: null,
              region: null,
            },
          },
        },
      }),
    );

    expect(within(hero()).queryByText(/unknown|n\/a|no genre|no synopsis|----/i)).toBeNull();
    expect(within(hero()).getByTitle('PlayStation')).toHaveTextContent('PS1');

    // Nothing normalized survives, so the hero falls back to the truthful local/readiness
    // projection instead of either fabricating provider fields or leaving the right half empty.
    const info = hero().querySelector('.game-detail-info');
    expect(info).not.toBeNull();
    const scoped = within(info as HTMLElement);
    expect(scoped.queryByText('DEVELOPER')).toBeNull();
    expect(scoped.queryByText('PUBLISHER')).toBeNull();
    expect(scoped.queryByText('RELEASE')).toBeNull();
    expect(scoped.getByText('CONTENT')).toBeInTheDocument();
    expect(scoped.getByText('RUNTIME')).toBeInTheDocument();
    expect(scoped.getByText('CORE')).toBeInTheDocument();
    expect(scoped.getByText('BIOS')).toBeInTheDocument();
    // Two content units cannot be honestly collapsed into one FORMAT/PATH pair.
    expect(scoped.queryByText('FORMAT')).toBeNull();
    expect(scoped.queryByText('PATH')).toBeNull();
  });

  it('projects the real synopsis and key/value information into the hero once', () => {
    renderDetail();

    expect(within(hero()).getByText('A fast arcade racing game.')).toBeInTheDocument();
    const info = hero().querySelector('.game-detail-info');
    expect(info).not.toBeNull();
    const scoped = within(info as HTMLElement);
    expect(scoped.getByText('RELEASE')).toBeInTheDocument();
    expect(scoped.getByText('1994-12-03')).toBeInTheDocument();
    expect(scoped.getByText('DEVELOPER')).toBeInTheDocument();
    expect(scoped.getByText('PUBLISHER')).toBeInTheDocument();
    expect(scoped.getByText('PLAYERS')).toBeInTheDocument();
    expect(scoped.getByText('1-2')).toBeInTheDocument();
    expect(scoped.getByText('REGION')).toBeInTheDocument();
    expect(scoped.getByText('US')).toBeInTheDocument();
    // Genre is already a hero chip; it must not be repeated as an information row.
    expect(scoped.queryByText('GENRE')).toBeNull();
    // The synopsis and normalized fields no longer live in the metadata workflow section.
    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
    expect(within(metadataPanel).queryByText('A fast arcade racing game.')).toBeNull();
    expect(within(metadataPanel).queryByText('DEVELOPER')).toBeNull();
  });

  it('suppresses a redundant local title and keeps a genuinely distinct one secondary', () => {
    renderDetail(
      detailModel({
        localDetail: { ...localDetail, localTitle: 'Ridge Racer' },
      }),
    );
    expect(within(hero()).queryByText(/LOCAL TITLE/)).toBeNull();

    screen.getByRole('heading', { level: 1, name: 'Ridge Racer' });
  });

  it('keeps the distinct local title as clearly secondary hero information', () => {
    renderDetail();
    expect(within(hero()).getByText(/LOCAL TITLE · Ridge Racer Local/)).toBeInTheDocument();
  });

  it('keeps route-entry focus on the hero title and semantic Library navigation', () => {
    const onBackToLibrary = vi.fn();
    render(
      <GameDetailPage
        detail={detailModel()}
        launch={launchModel()}
        gameId={7}
        onBackToLibrary={onBackToLibrary}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );

    const heading = screen.getByRole('heading', { level: 1, name: 'Ridge Racer' });
    expect(document.activeElement).toBe(heading);

    const back = screen.getByRole('link', { name: /back to library/i });
    expect(back).toHaveAttribute('href', '/library');
    fireEvent.click(back);
    expect(onBackToLibrary).toHaveBeenCalledTimes(1);
  });

  it('falls back to the C4 accent placeholder when the cached cover cannot be rendered', () => {
    renderDetail(detailModel({ metadata: { ...metadata, cover: cachedCover } }));

    fireEvent.error(screen.getByRole('img', { name: 'Cover art for Ridge Racer' }));
    expect(
      screen.getByRole('img', { name: 'No cover available for Ridge Racer' }),
    ).toBeInTheDocument();
  });

  it('renders readiness as one compact requirement list rather than dashboard cards', () => {
    renderDetail();

    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
    expect(readiness.querySelectorAll('article')).toHaveLength(0);
    const rows = within(readiness).getByRole('list', { name: 'Emulation requirements' });
    expect(within(rows).getAllByRole('listitem')).toHaveLength(4);
    expect(within(rows).getByText('LOCAL CONTENT')).toBeInTheDocument();
    expect(within(rows).getByText('RUNTIME')).toBeInTheDocument();
    expect(within(rows).getByText('CORE')).toBeInTheDocument();
    expect(within(rows).getByText('BIOS')).toBeInTheDocument();
  });

  it('keeps a stable matched state compact and free of the old metadata data panel', () => {
    renderDetail();

    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
    expect(within(metadataPanel).queryByText('NORMALIZED METADATA')).toBeNull();
    expect(within(metadataPanel).queryByText('ENRICHMENT')).toBeNull();
    expect(metadataPanel.querySelector('.game-detail-metadata-list')).toBeNull();
    // The single state truth stays visible exactly once.
    expect(within(metadataPanel).getAllByText('METADATA MATCHED')).toHaveLength(1);
    expect(within(metadataPanel).getByText(/ScreenScraper/)).toBeInTheDocument();
    expect(within(metadataPanel).getByRole('button', { name: 'REFRESH METADATA' })).toBeVisible();
  });

  it('keeps an unavailable metadata state compact, truthful, and secondary', () => {
    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'failed',
          metadata: null,
          cover: null,
          lastFailure: 'credentialsUnavailable',
        },
      }),
    );

    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
    expect(within(metadataPanel).getAllByText('METADATA UNAVAILABLE')).toHaveLength(1);
    expect(
      within(metadataPanel).getByText(/provider is not configured for this build/i),
    ).toBeInTheDocument();
    // "remains usable" is one product truth; the compact state surface must not echo it twice.
    const callout = within(metadataPanel).getByRole('status');
    expect(callout.textContent?.match(/remains usable/gi) ?? []).toHaveLength(1);
    // The hero still reads as a game, using the local identity.
    expect(
      screen.getByRole('heading', { level: 1, name: 'Ridge Racer Local' }),
    ).toBeInTheDocument();
  });

  it('shares one Detail content column and widens only for a real candidate workflow', () => {
    const { unmount } = renderDetail();
    // Hero and every secondary section share the same B6-derived content column.
    expect(document.querySelector('.game-detail-hero')).not.toBeNull();
    for (const section of document.querySelectorAll('.game-detail-section')) {
      expect(section.classList.contains('game-detail-section--wide')).toBe(false);
    }
    unmount();

    renderDetail(
      detailModel({
        metadata: {
          ...metadata,
          status: 'ambiguous',
          metadata: null,
          candidates: [
            { providerGameId: 'a', title: 'Ridge Racer', releaseDate: '1994' },
            { providerGameId: 'b', title: 'Ridge Racer Revolution', releaseDate: null },
          ],
        },
      }),
    );
    const metadataPanel = screen.getByRole('region', { name: 'METADATA' });
    expect(metadataPanel).toHaveClass('game-detail-section--wide');
    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
    expect(readiness).not.toHaveClass('game-detail-section--wide');
  });

  it('uses product section language instead of implementation labels', () => {
    renderDetail();

    expect(screen.getByRole('heading', { name: 'LOCAL CONTENT' })).toBeInTheDocument();
    expect(screen.queryByText('ASSOCIATED CONTENT')).toBeNull();
    expect(screen.queryByText('LOCAL UNITS SUMMARIZED')).toBeNull();
    const content = screen.getByRole('region', { name: 'LOCAL CONTENT' });
    expect(within(content).getByText('PlayStation/Ridge Racer.m3u')).toBeInTheDocument();
    expect(within(content).getAllByText('CONTENT ROOT #2')).toHaveLength(2);
  });
});

/**
 * The local-only SNES fixture from the M6.7C corrective comparison: a real scanned game with no
 * provider metadata, no cover, one single-file unit, an unavailable runtime, an unresolved core
 * policy, and a system that requires no BIOS.
 */
describe('GameDetailPage — local-only hero', () => {
  const localOnlyDetail: LibraryGameDetail = {
    gameId: 21,
    systemId: 'snes',
    localTitle: 'Gradius III (USA)',
    availability: 'available',
    favorite: false,
    contentUnits: [
      {
        unitId: 31,
        rootId: 1,
        kind: 'singleFile',
        localTitle: 'Gradius III (USA)',
        primaryRelativePath: 'SNES/Gradius III (USA).sfc',
        fileCount: 1,
        availability: 'available',
      },
    ],
  };

  const localOnlyStatus: SystemStatus = {
    id: 'snes',
    displayName: 'Super Nintendo Entertainment System',
    manufacturer: 'Nintendo',
    aliases: ['SNES'],
    supportedExtensions: ['.sfc', '.smc'],
    core: {
      policy: {
        defaultCoreId: null,
        approvedCoreIds: [],
        decision: { kind: 'unresolved', researchItem: 'snes-core-policy' },
      },
      availability: {
        runtimeState: 'notInstalled',
        availableCoreIds: [],
        defaultCoreAvailable: null,
      },
    },
    bios: { policy: 'notRequired', ready: true, requirements: [] },
    readiness: {
      ready: false,
      reasons: [{ kind: 'runtimeUnavailable', state: 'notInstalled' }],
    },
  };

  function renderLocalOnly() {
    return render(
      <GameDetailPage
        detail={detailModel({
          localDetail: localOnlyDetail,
          metadata: null,
          gameId: 21,
        } as Partial<GameDetailModel>)}
        gameId={21}
        launch={launchModel()}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={localOnlyStatus}
      />,
    );
  }

  function localHero() {
    const node = document.querySelector('.game-detail-hero');
    if (!(node instanceof HTMLElement)) throw new Error('hero not rendered');
    return node;
  }

  it('composes a useful hero from real local and readiness truth without inventing metadata', () => {
    renderLocalOnly();

    const headings = screen.getAllByRole('heading', { level: 1 });
    expect(headings).toHaveLength(1);
    expect(headings[0]).toHaveTextContent('Gradius III (USA)');

    const chips = localHero().querySelector('.game-detail-chips');
    expect(chips).not.toBeNull();
    const heroChildren = Array.from((chips as HTMLElement).parentElement!.children);
    expect(heroChildren.indexOf(headings[0])).toBeLessThan(
      heroChildren.indexOf(chips as HTMLElement),
    );

    const badge = within(localHero()).getByTitle('Super Nintendo Entertainment System');
    expect(badge).toHaveTextContent('SNES');

    const info = localHero().querySelector('.game-detail-info');
    expect(info).not.toBeNull();
    const scoped = within(info as HTMLElement);
    expect(scoped.getByText('CONTENT')).toBeInTheDocument();
    expect(scoped.getByText('AVAILABLE')).toBeInTheDocument();
    expect(scoped.getByText('FORMAT')).toBeInTheDocument();
    expect(scoped.getByText('SINGLE FILE')).toBeInTheDocument();
    expect(scoped.getByText('PATH')).toBeInTheDocument();
    // The path is the real scanned relative path, not a fabricated absolute host path.
    expect(scoped.getByText('SNES/Gradius III (USA).sfc')).toBeInTheDocument();
    expect(scoped.getByText('RUNTIME')).toBeInTheDocument();
    expect(scoped.getByText('UNAVAILABLE')).toBeInTheDocument();
    expect(scoped.getByText('CORE')).toBeInTheDocument();
    expect(scoped.getByText('UNKNOWN')).toBeInTheDocument();
    expect(scoped.getByText('BIOS')).toBeInTheDocument();
    expect(scoped.getByText('NOT REQUIRED')).toBeInTheDocument();
  });

  it('fabricates no provider metadata, launch action, or play history for a local-only game', () => {
    renderLocalOnly();

    const hero = localHero();
    expect(hero.querySelector('.game-detail-chip')).toBeNull();
    expect(hero.querySelector('.game-detail-year')).toBeNull();
    expect(hero.querySelector('.game-detail-synopsis')).toBeNull();
    expect(within(hero).queryByText('DEVELOPER')).toBeNull();
    expect(within(hero).queryByText('PUBLISHER')).toBeNull();
    expect(within(hero).queryByText('PLAYERS')).toBeNull();
    expect(within(hero).queryByText('REGION')).toBeNull();
    expect(within(hero).queryByText('RELEASE')).toBeNull();

    expect(screen.queryByRole('button', { name: /^(start|play|launch|run game)$/i })).toBeNull();
    expect(
      screen.queryByText(/playtime|last played|save state|screenshot|coming soon/i),
    ).toBeNull();

    // The C4 accent placeholder stands in for the missing cover; no provider cover is claimed.
    expect(
      screen.getByRole('img', { name: 'No cover available for Gradius III (USA)' }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole('button', { name: 'Add Gradius III (USA) to favorites' }),
    ).toBeInTheDocument();
  });

  it('keeps every readiness requirement explained in the readiness section', () => {
    renderLocalOnly();

    const readiness = screen.getByRole('region', { name: 'EMULATION READINESS' });
    expect(readiness.querySelectorAll('article')).toHaveLength(0);
    const rows = within(readiness).getByRole('list', { name: 'Emulation requirements' });
    expect(within(rows).getAllByRole('listitem')).toHaveLength(4);
    // The hero carries the at-a-glance statuses; the section keeps the explanations behind them.
    expect(
      within(readiness).getAllByText('Managed runtime is not installed.').length,
    ).toBeGreaterThan(0);
    expect(
      within(readiness).getAllByText('The approved system core policy is unresolved.').length,
    ).toBeGreaterThan(0);
    expect(
      within(readiness).getByText('This system does not require a BIOS file.'),
    ).toBeInTheDocument();
  });

  it('keeps the single local content unit truthful and compact', () => {
    renderLocalOnly();

    const content = screen.getByRole('region', { name: 'LOCAL CONTENT' });
    expect(within(content).getByText('1 UNIT')).toBeInTheDocument();
    expect(within(content).getByText('SINGLE FILE')).toBeInTheDocument();
    expect(within(content).getByText('CONTENT ROOT #1')).toBeInTheDocument();
    expect(within(content).getByText('1 FILE')).toBeInTheDocument();
    const path = within(content).getByText('SNES/Gradius III (USA).sfc');
    expect(path).toHaveAttribute('title', 'SNES/Gradius III (USA).sfc');
  });
});

describe('GameDetailPage launch interaction', () => {
  it('starts the game through the semantic launch contract, never a path', () => {
    const launch = launchModel();
    renderDetail(detailModel(), systemStatus(), false, launch);

    fireEvent.click(screen.getByRole('button', { name: /^play /i }));

    expect(launch.launch).toHaveBeenCalledWith(7);
    expect(launch.launch).toHaveBeenCalledTimes(1);
  });

  it('shows the launching state while the backend has not answered', () => {
    renderDetail(
      detailModel(),
      systemStatus(),
      false,
      launchModel({ phase: 'launching', pendingGameId: 7 }),
    );

    const play = screen.getByRole('button', { name: /is running|^play /i });
    expect(play).toHaveTextContent('LAUNCHING…');
    expect(play).toBeDisabled();
    expect(screen.getByText('STARTING RETROARCH…')).toBeInTheDocument();
  });

  it('shows the running state and its diagnostics while RetroArch owns the screen', () => {
    renderDetail(
      detailModel(),
      systemStatus(),
      false,
      launchModel({
        phase: 'running',
        running: {
          sessionId: 3,
          gameId: 7,
          contentUnitId: 1,
          coreId: 'beetle-psx',
          startedAt: 1_756_000_000_000,
        },
        diagnostics: [{ kind: 'audioService', message: 'No audio service was available.' }],
      }),
    );

    const play = screen.getByRole('button', { name: /is running/i });
    expect(play).toHaveTextContent('RUNNING');
    expect(play).toBeDisabled();
    expect(
      screen.getByText('RETROARCH IS RUNNING · RETROFRONTIER RETURNS WHEN IT EXITS'),
    ).toBeInTheDocument();
    expect(screen.getByText('No audio service was available.')).toBeInTheDocument();
  });

  it('refuses a second launch while another game is running', () => {
    renderDetail(
      detailModel(),
      systemStatus(),
      false,
      launchModel({
        phase: 'running',
        running: {
          sessionId: 3,
          gameId: 99,
          contentUnitId: 1,
          coreId: 'nestopia',
          startedAt: 1_756_000_000_000,
        },
      }),
    );

    const play = screen.getByRole('button', { name: /^play /i });
    expect(play).toHaveTextContent('ANOTHER GAME IS RUNNING');
    expect(play).toBeDisabled();
  });

  it('renders a normalized launch failure with an actionable hint', () => {
    const launch = launchModel({
      failure: {
        code: 'biosMissing',
        message: 'A required BIOS file is missing.',
        context: {
          systemId: 'playstation',
          coreId: null,
          biosRequirementIds: ['playstation-bios'],
          runtimeState: null,
          hostPrerequisite: null,
          exitCode: null,
          contentOptions: [],
        },
      },
    });
    renderDetail(detailModel(), systemStatus(), false, launch);

    expect(screen.getByText('BIOS MISSING')).toBeInTheDocument();
    expect(
      screen.getByText(/A required BIOS file is missing\. Check the BIOS requirements/),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: 'DISMISS' }));
    expect(launch.dismissFailure).toHaveBeenCalled();
  });

  it('offers the launchable versions and relaunches with the chosen unit', () => {
    const launch = launchModel({
      contentOptions: [
        {
          contentUnitId: 11,
          kind: 'chd',
          localTitle: 'Disc 1',
          fileCount: 1,
          availability: 'available',
        },
        {
          contentUnitId: 12,
          kind: 'm3u',
          localTitle: 'Full set',
          fileCount: 3,
          availability: 'available',
        },
      ],
    });
    renderDetail(detailModel(), systemStatus(), false, launch);

    expect(screen.getByRole('heading', { name: 'CHOOSE A VERSION' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: /Full set · M3U PLAYLIST · 3 FILES/ }));

    expect(launch.launch).toHaveBeenCalledWith(7, 12);
  });

  it('reports an unverifiable previous game process instead of offering a launch', () => {
    renderDetail(detailModel(), systemStatus(), false, launchModel({ blocked: true }));

    expect(screen.getByRole('button', { name: /^play /i })).toBeDisabled();
    expect(
      screen.getByText('A PREVIOUS GAME PROCESS COULD NOT BE VERIFIED · RESTART RETROFRONTIER'),
    ).toBeInTheDocument();
  });

  it('returns to the normal detail state once the game has exited', () => {
    const { rerender } = render(
      <GameDetailPage
        detail={detailModel()}
        gameId={7}
        launch={launchModel({
          phase: 'running',
          running: {
            sessionId: 3,
            gameId: 7,
            contentUnitId: 1,
            coreId: 'beetle-psx',
            startedAt: 1_756_000_000_000,
          },
        })}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );
    expect(screen.getByRole('button', { name: /is running/i })).toBeDisabled();

    rerender(
      <GameDetailPage
        detail={detailModel()}
        gameId={7}
        launch={launchModel()}
        onBackToLibrary={vi.fn()}
        onRetryReadiness={vi.fn()}
        readinessError={null}
        systemStatus={systemStatus()}
      />,
    );

    const play = screen.getByRole('button', { name: /^play /i });
    expect(play).toHaveTextContent('PLAY');
    expect(play).toBeEnabled();
    expect(screen.queryByText(/RETROARCH IS RUNNING/)).not.toBeInTheDocument();
  });
});
