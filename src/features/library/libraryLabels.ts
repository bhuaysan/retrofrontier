import type { SystemLabel } from '../../hooks/useSystemCatalog';

export function systemName(systemId: string, systems: SystemLabel[]) {
  return systems.find((system) => system.id === systemId)?.displayName ?? systemId;
}
