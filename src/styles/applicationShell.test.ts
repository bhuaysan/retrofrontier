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

  it('uses the compact desktop sidebar padding that fits at 1280x800', () => {
    const desktopStyles = applicationStyles.slice(
      0,
      applicationStyles.indexOf('@media (max-width: 860px)'),
    );
    const compactDesktopRule = desktopStyles.match(
      /@media \(min-width: 861px\) and \(min-height: 800px\)\s*\{\s*\.app-sidebar\s*\{([\s\S]*?)\}\s*\}/,
    );

    // With the 12 system rows and the Settings row, the normal 677px track needs 22px less
    // vertical padding. The compact rule preserves the full desktop spacing at 960px tall.
    expect(compactDesktopRule?.[1]).toMatch(/padding-block:\s*11px;/);
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

describe('A6 V5 focus language under controller input', () => {
  it('applies the accepted focus states to controller-driven focus without inventing a new one', () => {
    const focusRules = applicationStyles
      .split('\n')
      .filter((line) => line.includes(':focus-visible'))
      // Comment prose mentions the pseudo-class; only real selector lines are a contract.
      .filter((line) => /^[.:[a-zA-Z]/.test(line.trim()));
    expect(focusRules.length).toBeGreaterThan(0);

    // Every accepted focus selector carries a controller companion, because `:focus-visible` cannot
    // observe a gamepad.
    for (const rule of focusRules) {
      const companion = `[data-input-mode='controller'] ${rule.trim().replace(':focus-visible', ':focus')}`;
      expect(applicationStyles).toContain(companion.replace(/,$/, ''));
    }
  });

  it('introduces no focus ring and no new focus colour token', () => {
    const controllerRules = applicationStyles
      .split('}')
      .filter((block) => block.includes("[data-input-mode='controller']"));
    expect(controllerRules.length).toBeGreaterThan(0);
    for (const block of controllerRules) {
      expect(block).not.toMatch(/outline:(?!\s*none)/);
      expect(block).not.toMatch(/--focus[\w-]*:/);
    }
  });
});
