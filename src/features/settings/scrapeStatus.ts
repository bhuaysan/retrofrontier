import type {
  MetadataScrapeMode,
  MetadataScrapeProgress,
  MetadataScrapeRun,
} from '../../platform/ipc';

export type ScrapeModeCopy = {
  label: string;
  description: string;
};

export type ScrapeRunCopy = {
  label: string;
  description: string;
};

export function scrapeModeCopy(mode: MetadataScrapeMode): ScrapeModeCopy {
  switch (mode) {
    case 'missingMetadata':
      return {
        label: 'MISSING METADATA',
        description: 'Games that have not been scraped yet',
      };
    case 'refreshMatched':
      return {
        label: 'REFRESH MATCHED GAMES',
        description: 'Refresh metadata and covers for accepted matches',
      };
  }
}

/**
 * Status copy for one run.
 *
 * `providerWaiting` must come from the provider's own persisted deferral, never be inferred from a
 * run having nothing in flight — a run that has just started also has nothing in flight, and saying
 * it is waiting for capacity would be a guess dressed as a fact.
 *
 * Nothing here predicts a finishing time or a provider reset instant either. ScreenScraper supplies
 * neither, and RetroFrontier's own waits are locally scheduled probes, so the honest statement is
 * that work is waiting for capacity — not when capacity will return.
 */
export function scrapeRunCopy(run: MetadataScrapeRun, providerWaiting = false): ScrapeRunCopy {
  switch (run.status) {
    case 'preparing':
      return {
        label: 'PREPARING SCRAPE',
        description: 'Collecting the games this run will cover.',
      };
    case 'running':
      return providerWaiting
        ? {
            label: 'WAITING FOR PROVIDER CAPACITY',
            description:
              'ScreenScraper is not accepting more requests right now. The run continues on its own when capacity returns.',
          }
        : {
            label: 'SCRAPER RUNNING',
            description: 'This continues in the background. You can leave this screen.',
          };
    case 'stopping':
      return {
        label: 'STOPPING SCRAPER',
        description: 'No new work is being sent. Requests already in flight are finishing.',
      };
    case 'completed':
      return {
        label: 'SCRAPE COMPLETE',
        description: 'Every game in this run has an answer.',
      };
    case 'stopped':
      return {
        label: 'SCRAPE STOPPED',
        description:
          'Metadata already fetched was kept. Games this run did not reach are still available for a later run.',
      };
  }
}

/**
 * Why START SCRAPER cannot be pressed, or `null` when it can.
 *
 * A disabled control with no stated reason is a dead end: the user is left to guess whether the
 * scraper is broken, still loading, or simply has nothing to do. Each branch names the actual
 * condition rather than restating that the button is unavailable.
 */
export function scrapeStartBlockedReason({
  mode,
  eligibleGames,
  providerConfigured,
  loading,
}: {
  mode: MetadataScrapeMode;
  eligibleGames: number | null;
  providerConfigured: boolean;
  loading: boolean;
}): string | null {
  if (loading) return null;
  if (!providerConfigured) {
    return 'This build has no ScreenScraper application configuration, so no metadata can be fetched. A personal account cannot supply it.';
  }
  if (eligibleGames === null) {
    return 'RetroFrontier could not count the games this run would cover.';
  }
  if (eligibleGames === 0) {
    return mode === 'missingMetadata'
      ? 'Every game in the library has already been through ScreenScraper. Games that were matched, came back with no match, need review, or are an unsupported format are not scraped again.'
      : 'There are no accepted ScreenScraper matches to refresh yet.';
  }
  return null;
}

export type ScrapeResultRow = {
  label: string;
  value: number;
};

/**
 * The five result buckets, in the order the completion summary reads them.
 *
 * Always all five, including the zeroes: a run that reports only its successes invites the reader to
 * assume the rest succeeded too.
 */
export function scrapeResultRows(progress: MetadataScrapeProgress): ScrapeResultRow[] {
  return [
    { label: 'MATCHED', value: progress.matched },
    { label: 'NEEDS REVIEW', value: progress.needsReview },
    { label: 'NO MATCH', value: progress.noMatch },
    { label: 'UNSUPPORTED', value: progress.unsupported },
    { label: 'FAILED', value: progress.failed },
  ];
}
