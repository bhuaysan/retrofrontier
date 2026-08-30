import type {
  MetadataProviderStatus,
  ProviderAccountStatus,
  ProviderFailureClass,
} from '../../platform/ipc';

export type ProviderStatusView = {
  tone: 'online' | 'offline' | 'deferred' | 'unavailable';
  label: string;
  description: string;
};

export type AccountStatusView = {
  tone: 'online' | 'attention' | 'unavailable' | 'neutral';
  label: string;
  description: string;
};

export function hasActiveProviderDeferral(
  status: MetadataProviderStatus,
  now = Date.now(),
): boolean {
  return status.deferReason !== null && status.deferredUntil !== null && status.deferredUntil > now;
}

export function providerDeferralCopy(reason: ProviderFailureClass): ProviderStatusView | null {
  switch (reason) {
    case 'capacityDeferred':
      return {
        tone: 'deferred',
        label: 'CAPACITY DEFERRED',
        description: 'ScreenScraper is temporarily limiting requests. New work will wait.',
      };
    case 'dailyQuotaExceeded':
      return {
        tone: 'deferred',
        label: 'DAILY QUOTA DEFERRED',
        description: 'The provider daily capacity is temporarily exhausted. New work will wait.',
      };
    case 'negativeQuotaExceeded':
      return {
        tone: 'deferred',
        label: 'NO-MATCH QUOTA DEFERRED',
        description: 'The provider no-match capacity is temporarily exhausted. New work will wait.',
      };
    case 'providerRestricted':
      return {
        tone: 'deferred',
        label: 'PROVIDER ACCESS DEFERRED',
        description: 'ScreenScraper has temporarily restricted access. New work will wait.',
      };
    case 'providerUnavailable':
      return {
        tone: 'unavailable',
        label: 'PROVIDER UNAVAILABLE',
        description:
          'ScreenScraper is not accepting metadata work right now. Cached data remains available.',
      };
    // Transport/authentication failures are normally job dispositions rather than provider-wide
    // scheduler deferrals. Keep these defensive mappings redacted if an older/native DTO exposes
    // one here, but do not treat them as a new frontend policy.
    case 'transport':
    case 'transientServer':
      return {
        tone: 'offline',
        label: 'PROVIDER UNAVAILABLE',
        description: 'ScreenScraper is not reachable right now. Cached data remains available.',
      };
    case 'developerAuthenticationFailed':
    case 'credentialsUnavailable':
      return {
        tone: 'unavailable',
        label: 'PROVIDER UNAVAILABLE',
        description:
          'The application provider configuration is unavailable. Personal account settings cannot change it.',
      };
    case 'userAuthenticationFailed':
      return {
        tone: 'deferred',
        label: 'PERSONAL ACCOUNT NEEDS ATTENTION',
        description:
          'The optional personal account needs attention before personal access can be used.',
      };
    default:
      return null;
  }
}

export function providerStatusCopy(
  status: MetadataProviderStatus,
  now = Date.now(),
): ProviderStatusView {
  if (!status.credentialsConfigured) {
    return {
      tone: 'unavailable',
      label: 'PROVIDER UNAVAILABLE',
      description:
        'This build has no RetroFrontier application provider configuration. The personal account form cannot configure developer access.',
    };
  }

  if (status.offline) {
    return {
      tone: 'offline',
      label: 'PROVIDER OFFLINE',
      description:
        'ScreenScraper is currently unreachable. Cached metadata remains available; new work will wait.',
    };
  }

  if (hasActiveProviderDeferral(status, now) && status.deferReason !== null) {
    const deferred = providerDeferralCopy(status.deferReason);
    if (deferred) return deferred;
  }

  return {
    tone: 'online',
    label: 'PROVIDER ONLINE',
    description: 'ScreenScraper is available for metadata work.',
  };
}

function formatCount(value: number) {
  return value.toLocaleString('en-US');
}

function quotaObservationCopy(observedAt: number | null, now: number) {
  if (observedAt === null) return 'SNAPSHOT TIME NOT REPORTED';
  const age = Math.max(0, now - observedAt);
  if (age < 60 * 1000) return 'UPDATED JUST NOW';
  if (age < 60 * 60 * 1000) return `UPDATED ${Math.floor(age / (60 * 1000))}M AGO`;
  if (age < 24 * 60 * 60 * 1000) return `UPDATED ${Math.floor(age / (60 * 60 * 1000))}H AGO`;
  return 'SNAPSHOT MAY BE STALE';
}

export function quotaSummary(status: MetadataProviderStatus, now = Date.now()) {
  const daily =
    status.quota.requestsToday !== null && status.quota.maxRequestsPerDay !== null
      ? `DAILY ${formatCount(status.quota.requestsToday)} / ${formatCount(status.quota.maxRequestsPerDay)}`
      : null;
  const negative =
    status.quota.negativeRequestsToday !== null && status.quota.maxNegativeRequestsPerDay !== null
      ? `NO-MATCH ${formatCount(status.quota.negativeRequestsToday)} / ${formatCount(status.quota.maxNegativeRequestsPerDay)}`
      : null;

  return {
    quota:
      [daily, negative].filter((item): item is string => item !== null).join(' · ') ||
      'QUOTA NOT REPORTED',
    jobs: `${formatCount(status.pendingJobs)} PENDING · ${formatCount(status.deferredJobs)} DEFERRED · ${formatCount(status.failedJobs)} NEED RETRY`,
    observed: quotaObservationCopy(status.quotaObservedAt, now),
  };
}

export function accountStatusCopy(account: ProviderAccountStatus | null): AccountStatusView {
  if (!account) {
    return {
      tone: 'unavailable',
      label: 'ACCOUNT STATUS UNAVAILABLE',
      description: 'RetroFrontier could not read the optional personal provider account state.',
    };
  }

  switch (account.state) {
    case 'notConfigured':
      return {
        tone: 'neutral',
        label: 'OPTIONAL ACCOUNT NOT CONFIGURED',
        description:
          'No personal ScreenScraper account is configured. The local library remains usable.',
      };
    case 'configured':
      return {
        tone: 'neutral',
        label: 'PERSONAL ACCOUNT SAVED',
        description: account.username
          ? `Credentials for ${account.username} are saved in secure OS storage; provider authentication is not verified here.`
          : 'Personal credentials are saved in secure OS storage; provider authentication is not verified here.',
      };
    case 'invalid':
      return {
        tone: 'attention',
        label: 'PERSONAL ACCOUNT NEEDS ATTENTION',
        description:
          'The personal provider account was rejected. Enter the account credentials again.',
      };
    case 'vaultUnavailable':
      return {
        tone: 'unavailable',
        label: 'SECURE ACCOUNT STORAGE UNAVAILABLE',
        description:
          'Secure OS storage is unavailable, so personal account settings cannot be persisted right now.',
      };
  }
}
