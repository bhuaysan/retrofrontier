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
    refresh: vi.fn().mockResolvedValue(undefined),
    retryLocal: vi.fn().mockResolvedValue(undefined),
    retryMetadata: vi.fn().mockResolvedValue(undefined),
    toggleFavorite: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  };
}

function renderDetail(
  model: GameDetailModel = detailModel(),
  status: SystemStatus | null = systemStatus(),
) {
  return render(
    <GameDetailPage
      detail={model}
      gameId={7}
      onBackToLibrary={vi.fn()}
      onRetryReadiness={vi.fn()}
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
    expect(screen.getByText(/revalidation is needed/i)).toBeInTheDocument();
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
