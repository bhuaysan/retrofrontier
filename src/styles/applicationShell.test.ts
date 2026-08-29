import { describe, expect, it } from 'vitest';

import applicationStyles from './index.css?raw';

describe('application shell layout contract', () => {
  it('widens only the desktop library shell track and preserves responsive shell rules', () => {
    expect(applicationStyles).toMatch(
      /\.app-shell\s*\{[\s\S]*?grid-template-columns:\s*264px minmax\(0, 1fr\);/,
    );
    expect(applicationStyles).toMatch(/\.pixel-row-cursor\s*\{[\s\S]*?width:\s*18px;/);

    const mobileStyles = applicationStyles.slice(
      applicationStyles.indexOf('@media (max-width: 860px)'),
    );
    expect(mobileStyles).toMatch(/\.app-shell\s*\{[\s\S]*?grid-template-columns:\s*1fr;/);
    expect(mobileStyles).toMatch(/\.app-sidebar\s*\{[\s\S]*?display:\s*none;/);
  });
});
