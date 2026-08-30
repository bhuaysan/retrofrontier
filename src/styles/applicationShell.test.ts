import { describe, expect, it } from 'vitest';

import applicationStyles from './index.css?raw';

describe('application shell layout contract', () => {
  it('keeps the shared desktop shell track and responsive shell rules', () => {
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

  it('reserves the measured Library search height only for desktop', () => {
    const desktopStyles = applicationStyles.slice(
      0,
      applicationStyles.indexOf('@media (max-width: 860px)'),
    );
    expect(desktopStyles).toMatch(/\.app-header\s*\{[\s\S]*?min-height:\s*82px;/);

    const responsiveStyles = applicationStyles.slice(
      applicationStyles.indexOf('@media (max-width: 860px)'),
    );
    expect(responsiveStyles).toMatch(
      /@media \(max-width: 860px\)\s*\{[\s\S]*?\.app-header\s*\{[\s\S]*?min-height:\s*0;/,
    );
  });
});
