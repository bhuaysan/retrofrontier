import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { IpcError, type SaveStateView } from '../../platform/ipc';
import type { SaveStatesModel } from '../../hooks/useSaveStates';
import { SaveStatesSection } from './SaveStatesSection';

function view(overrides: Partial<SaveStateView> = {}): SaveStateView {
  return {
    id: 1,
    gameId: 7,
    contentUnitId: 11,
    slot: 1,
    coreId: 'beetle_psx',
    coreDisplayVersion: '0.9.44',
    coreSourceRevision: null,
    contentUnitLabel: null,
    createdAt: new Date(2026, 8, 1, 9, 15).getTime(),
    updatedAt: new Date(2026, 8, 3, 14, 32).getTime(),
    thumbnailRef: null,
    capabilities: { loadability: 'ready', deletable: true },
    ...overrides,
  };
}

function model(overrides: Partial<SaveStatesModel> = {}): SaveStatesModel {
  return {
    states: [view()],
    loaded: true,
    loading: false,
    error: null,
    deletePendingId: null,
    actionFailure: null,
    loadPendingId: null,
    retry: vi.fn().mockResolvedValue(undefined),
    load: vi.fn().mockResolvedValue(undefined),
    delete: vi.fn().mockResolvedValue(undefined),
    dismissActionFailure: vi.fn(),
    ...overrides,
  };
}

function renderSection(overrides: Partial<SaveStatesModel> = {}) {
  const saveStates = model(overrides);
  render(<SaveStatesSection saveStates={saveStates} />);
  return saveStates;
}

function section() {
  return screen.getByRole('region', { name: 'SAVE STATES' });
}

function cards() {
  return within(section()).getAllByRole('listitem');
}

describe('SaveStatesSection', () => {
  it('renders the states the backend delivered, in the order it delivered them', () => {
    // The backend sorts `updated_at DESC`; re-sorting here would be a second, silently diverging
    // opinion about an order the backend owns.
    renderSection({
      states: [
        view({ id: 5, slot: 2, updatedAt: new Date(2026, 8, 3, 14, 32).getTime() }),
        view({ id: 9, slot: 7, updatedAt: new Date(2026, 8, 9, 8, 5).getTime() }),
        view({ id: 2, slot: 4, updatedAt: new Date(2026, 8, 5, 21, 0).getTime() }),
      ],
    });

    expect(cards().map((card) => card.querySelector('h3')?.textContent)).toEqual([
      'SLOT 2',
      'SLOT 7',
      'SLOT 4',
    ]);
  });

  it('renders the slot, the save time, and the compact core identity', () => {
    renderSection({ states: [view({ slot: 3 })] });

    const [card] = cards();
    expect(within(card).getByRole('heading', { level: 3, name: 'SLOT 3' })).toBeInTheDocument();
    expect(within(card).getByText('2026-09-03 14:32')).toBeInTheDocument();
    expect(within(card).getByText('BEETLE PSX · 0.9.44')).toBeInTheDocument();
  });

  it('renders the verified thumbnail through its opaque reference', () => {
    renderSection({ states: [view({ slot: 3, thumbnailRef: 'rfmedia://localhost/x/1' })] });

    const thumbnail = within(cards()[0]).getByRole('img');
    expect(thumbnail).toHaveAttribute('src', 'rfmedia://localhost/x/1');
    expect(thumbnail).toHaveAccessibleName('Save state thumbnail for SLOT 3 · 2026-09-03 14:32');
  });

  it('renders a neutral placeholder when there is no thumbnail', () => {
    renderSection({ states: [view({ slot: 3 })] });

    const placeholder = within(cards()[0]).getByRole('img', {
      name: 'No thumbnail for SLOT 3 · 2026-09-03 14:32',
    });
    expect(placeholder.tagName).toBe('DIV');
  });

  it('falls back to the placeholder when a referenced thumbnail cannot be rendered', () => {
    renderSection({ states: [view({ slot: 3, thumbnailRef: 'rfmedia://localhost/x/1' })] });

    fireEvent.error(within(cards()[0]).getByRole('img'));

    expect(
      within(cards()[0]).getByRole('img', {
        name: 'No thumbnail for SLOT 3 · 2026-09-03 14:32',
      }),
    ).toBeInTheDocument();
  });

  it('renders a content-unit label only when it disambiguates', () => {
    renderSection({
      states: [
        view({ id: 1, contentUnitLabel: 'Disc 2' }),
        view({ id: 2, slot: 2, contentUnitLabel: null }),
      ],
    });

    expect(within(cards()[0]).getByText('Disc 2')).toBeInTheDocument();
    expect(cards()[1].querySelector('.save-state-disc')).toBeNull();
  });

  it('renders each loadability value with its own truthful copy', () => {
    renderSection({
      states: [
        view({ id: 1, slot: 1, capabilities: { loadability: 'ready', deletable: true } }),
        view({ id: 2, slot: 2, capabilities: { loadability: 'coreUnavailable', deletable: true } }),
        view({
          id: 3,
          slot: 3,
          capabilities: { loadability: 'temporarilyBlocked', deletable: false },
        }),
      ],
    });

    expect(within(cards()[0]).getByText('READY TO LOAD')).toBeInTheDocument();
    expect(within(cards()[1]).getByText('REQUIRED CORE UNAVAILABLE')).toBeInTheDocument();
    expect(within(cards()[2]).getByText('TEMPORARILY UNAVAILABLE')).toBeInTheDocument();
    // Only a ready state offers the action; the others say why, and say it about the core or the
    // session rather than about the state.
    expect(within(cards()[0]).getByRole('button', { name: /^Load SLOT 1/ })).toBeEnabled();
    expect(within(cards()[1]).getByRole('button', { name: /^Load SLOT 2/ })).toBeDisabled();
    expect(within(cards()[2]).getByRole('button', { name: /^Load SLOT 3/ })).toBeDisabled();
  });

  it('claims compatibility nowhere', () => {
    renderSection({
      states: [
        view({ id: 1, capabilities: { loadability: 'ready', deletable: true } }),
        view({ id: 2, slot: 2, capabilities: { loadability: 'coreUnavailable', deletable: true } }),
        view({
          id: 3,
          slot: 3,
          capabilities: { loadability: 'temporarilyBlocked', deletable: false },
        }),
      ],
    });

    expect(section().textContent?.toLocaleLowerCase()).not.toContain('compatible');
  });

  it('exposes no digest-shaped value', () => {
    renderSection({
      states: [
        view({
          coreDisplayVersion: null,
          coreSourceRevision: 'c'.repeat(64),
          thumbnailRef: `rfmedia://localhost/save-state-thumbnail/1`,
        }),
      ],
    });

    expect(section().textContent ?? '').not.toMatch(/[0-9a-f]{64}/i);
  });

  it('distinguishes two states that share a slot but differ in core provenance', () => {
    renderSection({
      states: [
        view({ id: 1, slot: 2, coreDisplayVersion: '0.9.44' }),
        view({ id: 2, slot: 2, coreDisplayVersion: '0.9.45' }),
      ],
    });

    const [first, second] = cards();
    expect(within(first).getByText('BEETLE PSX · 0.9.44')).toBeInTheDocument();
    expect(within(second).getByText('BEETLE PSX · 0.9.45')).toBeInTheDocument();
  });

  it('teaches the ingame save workflow when there are no states yet', () => {
    renderSection({ states: [] });

    const empty = within(section()).getByText('NO SAVE STATES YET').closest('div');
    expect(empty).not.toBeNull();
    expect(within(empty as HTMLElement).getByText('IN GAME')).toBeInTheDocument();
    expect(within(empty as HTMLElement).getByText('SELECT + R1')).toBeInTheDocument();
    expect(within(empty as HTMLElement).getByText('SAVE STATE')).toBeInTheDocument();
    expect(within(empty as HTMLElement).getByText('SELECT + ← / →')).toBeInTheDocument();
    expect(within(empty as HTMLElement).getByText('CHANGE SLOT')).toBeInTheDocument();
    // There is deliberately no ingame Load hotkey, so none is advertised.
    expect((empty as HTMLElement).textContent ?? '').not.toMatch(/load/i);
    expect(within(section()).queryByRole('list')).not.toBeInTheDocument();
  });

  it('offers a bounded retry when the list could not be read', () => {
    const saveStates = renderSection({
      states: [],
      error: new IpcError('database_unavailable', 'The local database is unavailable.'),
    });

    fireEvent.click(within(section()).getByRole('button', { name: 'RETRY SAVE STATES' }));

    expect(saveStates.retry).toHaveBeenCalledTimes(1);
  });

  it('renders the normalized action failure and lets it be dismissed', () => {
    const saveStates = renderSection({
      actionFailure: {
        code: 'integrityMismatch',
        message: 'The registered identity of this save state no longer matches.',
      },
    });

    expect(
      within(section()).getByText('The registered identity of this save state no longer matches.'),
    ).toBeInTheDocument();
    fireEvent.click(within(section()).getByRole('button', { name: 'DISMISS' }));

    expect(saveStates.dismissActionFailure).toHaveBeenCalledTimes(1);
  });

  it('loads a ready state through the semantic model action', () => {
    const saveStates = renderSection({ states: [view({ id: 4, slot: 5 })] });

    fireEvent.click(within(cards()[0]).getByRole('button', { name: /^Load SLOT 5/ }));

    expect(saveStates.load).toHaveBeenCalledWith(4);
  });

  it('reports an in-flight load truthfully', () => {
    renderSection({ states: [view({ id: 4, slot: 5 })], loadPendingId: 4 });

    expect(within(cards()[0]).getByRole('button', { name: /^Load SLOT 5/ })).toBeDisabled();
    expect(within(cards()[0]).getByText('LOADING…')).toBeInTheDocument();
  });

  it('never describes an in-flight delete as a load', () => {
    renderSection({ states: [view({ id: 4, slot: 5 })], deletePendingId: 4 });

    // Both actions block the card, but they are different actions and the card says which one.
    expect(within(cards()[0]).getByRole('button', { name: /^Load SLOT 5/ })).toBeDisabled();
    expect(within(cards()[0]).queryByText('LOADING…')).not.toBeInTheDocument();
  });
});
