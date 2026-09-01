import { describe, expect, it } from 'vitest';

import type { MetadataScrapeRun, MetadataScrapeRunStatus } from '../../platform/ipc';
import {
  scrapeModeCopy,
  scrapeResultRows,
  scrapeRunCopy,
  scrapeStartBlockedReason,
} from './scrapeStatus';

function run(status: MetadataScrapeRunStatus): MetadataScrapeRun {
  return {
    id: 1,
    providerId: 'screenScraper',
    mode: 'missingMetadata',
    status,
    progress: {
      totalGames: 148,
      matched: 0,
      needsReview: 0,
      noMatch: 0,
      unsupported: 0,
      failed: 0,
      running: 0,
      waiting: 148,
    },
    createdAt: 1,
    updatedAt: 1,
    finishedAt: null,
  };
}

const everyStatus: MetadataScrapeRunStatus[] = [
  'preparing',
  'running',
  'stopping',
  'completed',
  'stopped',
];

describe('scrapeRunCopy', () => {
  it('describes every run state', () => {
    for (const status of everyStatus) {
      const copy = scrapeRunCopy(run(status));
      expect(copy.label.length).toBeGreaterThan(0);
      expect(copy.description.length).toBeGreaterThan(0);
    }
  });

  it('claims a provider wait only when the provider actually reported one', () => {
    // Nothing in flight is not evidence of a provider wait — a run that has only just started looks
    // exactly the same.
    expect(scrapeRunCopy(run('running')).label).toBe('SCRAPER RUNNING');
    expect(scrapeRunCopy(run('running'), true).label).toBe('WAITING FOR PROVIDER CAPACITY');
  });

  it('never predicts a reset instant, a finishing time, or a percentage', () => {
    for (const status of everyStatus) {
      for (const waiting of [false, true]) {
        const copy = scrapeRunCopy(run(status), waiting);
        const text = `${copy.label} ${copy.description}`;
        expect(text).not.toMatch(/resets? at|reset time|resets in/i);
        expect(text).not.toMatch(/\bETA\b/i);
        expect(text).not.toMatch(/\d+\s*%/);
        expect(text).not.toMatch(/minutes? (?:left|remaining)|estimated/i);
      }
    }
  });

  it('promises a stopped run kept what it had already fetched', () => {
    const stopped = scrapeRunCopy(run('stopped'));
    expect(stopped.description).toMatch(/kept/i);
    expect(stopped.description).toMatch(/later run/i);
  });
});

describe('scrapeModeCopy', () => {
  it('describes missing metadata as unscraped games rather than missing covers', () => {
    const copy = scrapeModeCopy('missingMetadata');
    expect(copy.description).toMatch(/not been scraped/i);
    expect(copy.description).not.toMatch(/cover/i);
  });

  it('describes refresh as applying to accepted matches only', () => {
    expect(scrapeModeCopy('refreshMatched').description).toMatch(/accepted matches/i);
  });
});

describe('scrapeResultRows', () => {
  it('always reports all five result buckets, including the zeroes', () => {
    const rows = scrapeResultRows({
      totalGames: 148,
      matched: 119,
      needsReview: 14,
      noMatch: 10,
      unsupported: 0,
      failed: 0,
      running: 0,
      waiting: 5,
    });

    expect(rows.map((row) => row.label)).toEqual([
      'MATCHED',
      'NEEDS REVIEW',
      'NO MATCH',
      'UNSUPPORTED',
      'FAILED',
    ]);
    // Reporting only the non-zero buckets would invite the reader to assume the rest succeeded.
    expect(rows.map((row) => row.value)).toEqual([119, 14, 10, 0, 0]);
    // The buckets are the whole of the processed figure; nothing waiting is folded into them.
    expect(rows.reduce((total, row) => total + row.value, 0)).toBe(143);
  });
});

describe('scrapeStartBlockedReason', () => {
  const base = {
    mode: 'missingMetadata' as const,
    eligibleGames: 148,
    providerConfigured: true,
    loading: false,
  };

  it('gives no reason when the scraper can actually start', () => {
    expect(scrapeStartBlockedReason(base)).toBeNull();
  });

  it('stays silent while the count is still being read rather than guessing', () => {
    expect(scrapeStartBlockedReason({ ...base, eligibleGames: null, loading: true })).toBeNull();
  });

  it('names a missing application configuration ahead of the game count', () => {
    // Without configuration the count is beside the point: nothing can be fetched either way.
    const reason = scrapeStartBlockedReason({ ...base, providerConfigured: false });
    expect(reason).toMatch(/application configuration/i);
    expect(scrapeStartBlockedReason({ ...base, providerConfigured: false, eligibleGames: 0 })).toBe(
      reason,
    );
  });

  it('explains an empty missing-metadata target without implying the games are broken', () => {
    const reason = scrapeStartBlockedReason({ ...base, eligibleGames: 0 });
    expect(reason).toMatch(/already been through ScreenScraper/i);
    // It must say why those games are excluded, not merely that there are none.
    expect(reason).toMatch(/no match|need review|unsupported/i);
  });

  it('explains an empty refresh target in its own terms', () => {
    expect(scrapeStartBlockedReason({ ...base, mode: 'refreshMatched', eligibleGames: 0 })).toMatch(
      /no accepted ScreenScraper matches/i,
    );
  });

  it('reports a failed count as a failed count', () => {
    expect(scrapeStartBlockedReason({ ...base, eligibleGames: null })).toMatch(/could not count/i);
  });
});
