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

  it('contains sidebar overflow inside its own grid track', () => {
    const desktopStyles = applicationStyles.slice(
      0,
      applicationStyles.indexOf('@media (max-width: 860px)'),
    );
    const sidebar = desktopStyles.slice(
      desktopStyles.indexOf('.app-sidebar {'),
      desktopStyles.indexOf('}', desktopStyles.indexOf('.app-sidebar {')),
    );
    expect(sidebar).toMatch(/min-height:\s*0;/);
    expect(sidebar).toMatch(/overflow-y:\s*auto;/);

    // The shell itself must never become the scroll surface, otherwise the header and footer
    // would leave the viewport at the 960x640 minimum.
    const shell = desktopStyles.slice(
      desktopStyles.indexOf('.app-shell {'),
      desktopStyles.indexOf('}', desktopStyles.indexOf('.app-shell {')),
    );
    expect(shell).toMatch(/overflow:\s*hidden;/);
    expect(shell).toMatch(/grid-template-rows:\s*auto minmax\(0, 1fr\) auto;/);
  });

  it('keeps .app-main the full-width scroll owner on every route', () => {
    // Settings caps only its inner content measure; narrowing `.app-main` would move the
    // main-region scrollbar inward on the Settings route only.
    expect(applicationStyles).not.toContain('.settings-main');
    const settingsContent = applicationStyles.slice(
      applicationStyles.indexOf('.settings-content {'),
      applicationStyles.indexOf('}', applicationStyles.indexOf('.settings-content {')),
    );
    expect(settingsContent).toMatch(/width:\s*min\(100%, 700px\);/);

    const main = applicationStyles.slice(
      applicationStyles.indexOf('.app-main {'),
      applicationStyles.indexOf('}', applicationStyles.indexOf('.app-main {')),
    );
    expect(main).not.toMatch(/(?<!-)\bwidth:/);
    expect(main).toMatch(/overflow-y:\s*auto;/);
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
