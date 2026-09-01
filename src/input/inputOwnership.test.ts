import { describe, expect, it } from 'vitest';

import type { RunningGameSession } from '../platform/ipc';
import { ownsApplicationInput, type ApplicationInputOwnership } from './inputOwnership';

const session: RunningGameSession = {
  sessionId: 4,
  gameId: 1,
  contentUnitId: 11,
  coreId: 'nestopia',
  startedAt: 1,
};

function state(overrides: Partial<ApplicationInputOwnership> = {}): ApplicationInputOwnership {
  return {
    windowFocused: true,
    running: null,
    blocked: false,
    pendingGameId: null,
    ...overrides,
  };
}

describe('ownsApplicationInput', () => {
  it('owns input only while the window is focused and no launch is in flight', () => {
    expect(ownsApplicationInput(state())).toBe(true);
  });

  it('gives up ownership while the application window is not confirmed focused', () => {
    expect(ownsApplicationInput(state({ windowFocused: false }))).toBe(false);
  });

  it('gives up ownership as soon as a launch request becomes pending', () => {
    // The backend may already have spawned RetroArch while React still reports `running === null`.
    // Ownership must be released at the launch request, not when the running state arrives.
    expect(ownsApplicationInput(state({ pendingGameId: 1 }))).toBe(false);
  });

  it('stays without ownership once the backend reports a running game', () => {
    expect(ownsApplicationInput(state({ running: session }))).toBe(false);
    expect(ownsApplicationInput(state({ running: session, pendingGameId: 1 }))).toBe(false);
  });

  it('gives up ownership while launch state is blocked', () => {
    expect(ownsApplicationInput(state({ blocked: true }))).toBe(false);
  });

  it('regains ownership once a launch request resolved without starting a process', () => {
    // A failed launch and a content-selection response both clear `pendingGameId`.
    expect(ownsApplicationInput(state({ pendingGameId: null, running: null }))).toBe(true);
  });
});
