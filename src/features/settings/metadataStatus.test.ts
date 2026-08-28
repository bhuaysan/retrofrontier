import { describe, expect, it } from 'vitest';

import type { MetadataProviderStatus, ProviderAccountStatus } from '../../platform/ipc';
import { accountStatusCopy, providerStatusCopy, quotaSummary } from './metadataStatus';

function status(overrides: Partial<MetadataProviderStatus> = {}): MetadataProviderStatus {
  return {
    providerId: 'screenScraper',
    credentialsConfigured: true,
    userAccount: 'notConfigured',
    userAccountName: null,
    quota: {
      maxThreads: 1,
      maxRequestsPerMinute: 60,
      maxRequestsPerDay: 1000,
      maxNegativeRequestsPerDay: 100,
      requestsToday: 4,
      negativeRequestsToday: 1,
    },
    quotaObservedAt: 100,
    deferredUntil: null,
    deferReason: null,
    offline: false,
    pendingJobs: 2,
    deferredJobs: 0,
    failedJobs: 0,
    ...overrides,
  };
}

describe('metadata provider status copy', () => {
  it('distinguishes online, offline, and application-credential-unavailable states', () => {
    expect(providerStatusCopy(status())).toEqual({
      tone: 'online',
      label: 'PROVIDER ONLINE',
      description: 'ScreenScraper is available for metadata work.',
    });
    expect(
      providerStatusCopy(
        status({
          offline: true,
          deferReason: 'transport',
        }),
        100,
      ),
    ).toEqual({
      tone: 'offline',
      label: 'PROVIDER OFFLINE',
      description:
        'ScreenScraper is currently unreachable. Cached metadata remains available; new work will wait.',
    });
    expect(providerStatusCopy(status({ credentialsConfigured: false }))).toEqual({
      tone: 'unavailable',
      label: 'PROVIDER UNAVAILABLE',
      description:
        'This build has no RetroFrontier application provider configuration. The personal account form cannot configure developer access.',
    });
  });

  it.each([
    ['capacityDeferred', 'CAPACITY DEFERRED'],
    ['dailyQuotaExceeded', 'DAILY QUOTA DEFERRED'],
    ['negativeQuotaExceeded', 'NO-MATCH QUOTA DEFERRED'],
    ['providerRestricted', 'PROVIDER ACCESS DEFERRED'],
    ['providerUnavailable', 'PROVIDER UNAVAILABLE'],
  ] as const)('maps %s without exposing provider internals', (reason, label) => {
    const copy = providerStatusCopy(status({ deferReason: reason, deferredUntil: 200 }), 100);

    expect(copy.label).toBe(label);
    expect(copy.description).not.toContain(reason);
    expect(copy.description).not.toMatch(/429|430|431|https?:\/\//i);
  });

  it('distinguishes active and expired provider deferrals from normalized time fields', () => {
    expect(
      providerStatusCopy(status({ deferReason: 'capacityDeferred', deferredUntil: 200 }), 100).tone,
    ).toBe('deferred');
    expect(
      providerStatusCopy(status({ deferReason: 'capacityDeferred', deferredUntil: 100 }), 100),
    ).toEqual({
      tone: 'online',
      label: 'PROVIDER ONLINE',
      description: 'ScreenScraper is available for metadata work.',
    });
    expect(
      providerStatusCopy(status({ deferReason: 'capacityDeferred', deferredUntil: null }), 100)
        .tone,
    ).toBe('online');
  });

  it('summarizes normalized quota and gives its snapshot a truthful recency', () => {
    expect(quotaSummary(status(), 100)).toEqual({
      quota: 'DAILY 4 / 1,000 · NO-MATCH 1 / 100',
      jobs: '2 PENDING · 0 DEFERRED · 0 NEED RETRY',
      observed: 'UPDATED JUST NOW',
    });
    expect(
      quotaSummary(
        status({
          quota: {
            maxThreads: null,
            maxRequestsPerMinute: null,
            maxRequestsPerDay: null,
            maxNegativeRequestsPerDay: null,
            requestsToday: null,
            negativeRequestsToday: null,
          },
          pendingJobs: 0,
          deferredJobs: 3,
          failedJobs: 1,
          quotaObservedAt: null,
        }),
        100,
      ),
    ).toEqual({
      quota: 'QUOTA NOT REPORTED',
      jobs: '0 PENDING · 3 DEFERRED · 1 NEED RETRY',
      observed: 'SNAPSHOT TIME NOT REPORTED',
    });
    expect(quotaSummary(status({ quotaObservedAt: 100 - 60 * 60 * 1000 }), 100)).toMatchObject({
      observed: 'UPDATED 1H AGO',
    });
    expect(
      quotaSummary(status({ quotaObservedAt: 100 - 2 * 24 * 60 * 60 * 1000 }), 100),
    ).toMatchObject({
      observed: 'SNAPSHOT MAY BE STALE',
    });
  });
});

describe('metadata provider account copy', () => {
  it.each([
    [
      { configured: false, state: 'notConfigured', username: null },
      'OPTIONAL ACCOUNT NOT CONFIGURED',
    ],
    [{ configured: true, state: 'configured', username: 'test-user' }, 'PERSONAL ACCOUNT SAVED'],
    [
      { configured: true, state: 'invalid', username: 'test-user' },
      'PERSONAL ACCOUNT NEEDS ATTENTION',
    ],
    [
      { configured: false, state: 'vaultUnavailable', username: null },
      'SECURE ACCOUNT STORAGE UNAVAILABLE',
    ],
  ] as const)('maps %s to safe account copy', (account, label) => {
    const copy = accountStatusCopy(account as ProviderAccountStatus);

    expect(copy.label).toBe(label);
    expect(copy.description).not.toContain('password');
    expect(copy.description).not.toContain('secret');
  });

  it('includes only the safely returned account name for a configured account', () => {
    expect(
      accountStatusCopy({ configured: true, state: 'configured', username: 'test-user' }),
    ).toEqual({
      tone: 'neutral',
      label: 'PERSONAL ACCOUNT SAVED',
      description:
        'Credentials for test-user are saved in secure OS storage; provider authentication is not verified here.',
    });
  });
});
