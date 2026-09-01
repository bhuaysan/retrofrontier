import { describe, expect, it } from 'vitest';

import { NO_SUPPORTED_ACTIONS, deriveFooterHints } from './footerHints';

describe('deriveFooterHints', () => {
  it('derives one hint per supported action, in a stable order', () => {
    expect(
      deriveFooterHints({ confirm: 'OPEN', back: 'LIBRARY', context: 'SELECT', search: 'SEARCH' }),
    ).toEqual([
      { action: 'confirm', button: 'A', label: 'OPEN' },
      { action: 'back', button: 'B', label: 'LIBRARY' },
      { action: 'context', button: 'X', label: 'SELECT' },
      { action: 'search', button: 'Y', label: 'SEARCH' },
    ]);
  });

  it('omits an action the focused node does not support', () => {
    expect(deriveFooterHints({ confirm: 'PLAY', back: null, context: null, search: null })).toEqual(
      [{ action: 'confirm', button: 'A', label: 'PLAY' }],
    );
    expect(deriveFooterHints(NO_SUPPORTED_ACTIONS)).toEqual([]);
  });

  it('treats a blank label as unsupported rather than showing an empty hint', () => {
    expect(
      deriveFooterHints({ confirm: '   ', back: 'CANCEL', context: null, search: '  ' }),
    ).toEqual([{ action: 'back', button: 'B', label: 'CANCEL' }]);
  });
});
