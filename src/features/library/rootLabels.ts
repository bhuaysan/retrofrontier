import type { ContentRoot } from '../../platform/ipc';

export function rootAvailabilityLabel(root: ContentRoot) {
  if (!root.enabled || root.availability === 'disabled') return 'DISABLED';
  switch (root.availability) {
    case 'available':
      return 'AVAILABLE';
    case 'partiallyAvailable':
      return 'PARTIALLY AVAILABLE';
    case 'unavailable':
      return 'UNAVAILABLE';
    case 'unsafe':
      return 'UNSAFE';
  }
}
