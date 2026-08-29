import { describe, expect, it } from 'vitest';

import designTokens from '../../docs/design/tokens.css?raw';
import applicationStyles from './index.css?raw';

function tokenValue(theme: 'dark' | 'light', token: string) {
  const themeSelector =
    theme === 'dark'
      ? ':root,\\s*\\[data-theme=["\']dark["\']\\]'
      : '\\n\\[data-theme=["\']light["\']\\]';
  const tokenPattern = new RegExp(`${themeSelector}[\\s\\S]*?${token}:\\s*([^;]+);`);
  return designTokens.match(tokenPattern)?.[1].trim();
}

function contrastRatio(foreground: string, background: string) {
  const luminance = (hex: string) => {
    const channels = hex.match(/[a-f\d]{2}/gi)?.map((value) => Number.parseInt(value, 16) / 255);
    if (!channels || channels.length !== 3) throw new Error(`Expected a six-digit colour: ${hex}`);
    const [red, green, blue] = channels.map((channel) =>
      channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
    );
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
  };
  const [lighter, darker] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

describe('design semantic tokens', () => {
  it('defines an AA negative-text colour in both themes without changing decorative accent-6', () => {
    expect(tokenValue('dark', '--negative-text')).toBe('#ffb26c');
    expect(tokenValue('light', '--negative-text')).toBe('#743c00');
    expect(tokenValue('dark', '--accent-6')).toBe('#c9834a');
    expect(tokenValue('light', '--accent-6')).toBe('#b5753d');
  });

  it('keeps the light negative text color above AA contrast on rendered light surfaces', () => {
    for (const background of ['#fbf5e8', '#f1e8d4', '#efe4cd']) {
      expect(contrastRatio('#743c00', background)).toBeGreaterThanOrEqual(4.5);
    }
  });

  it('uses the semantic negative text token for critical error text', () => {
    for (const selector of [
      '.inline-error-icon',
      '.inline-error-copy strong',
      '.scan-panel--failed .panel-heading h2',
      '.issue-row-heading strong',
      '.root-confirm-actions > span',
      '.metadata-account-error',
      '.game-detail-action-error',
    ]) {
      const start = applicationStyles.indexOf(`${selector} {`);
      expect(start).toBeGreaterThanOrEqual(0);
      expect(applicationStyles.slice(start, start + 220)).toContain('color: var(--negative-text)');
    }
  });
});
