import { useEffect, useRef, useState } from 'react';

import { getSystems, normalizeIpcError, type IpcError, type SystemId } from '../platform/ipc';

export interface SystemLabel {
  id: SystemId;
  displayName: string;
}

const FALLBACK_SYSTEMS: SystemLabel[] = [
  { id: 'nes', displayName: 'Nintendo Entertainment System' },
  { id: 'snes', displayName: 'Super Nintendo Entertainment System' },
  { id: 'nintendo_64', displayName: 'Nintendo 64' },
  { id: 'game_boy', displayName: 'Game Boy' },
  { id: 'game_boy_color', displayName: 'Game Boy Color' },
  { id: 'game_boy_advance', displayName: 'Game Boy Advance' },
  { id: 'mega_drive', displayName: 'Mega Drive' },
  { id: 'playstation', displayName: 'PlayStation' },
  { id: 'sega_saturn', displayName: 'Sega Saturn' },
  { id: 'sega_dreamcast', displayName: 'Sega Dreamcast' },
  { id: 'nintendo_gamecube', displayName: 'Nintendo GameCube' },
];

export function useSystemCatalog() {
  const mounted = useRef(true);
  const [systems, setSystems] = useState<SystemLabel[]>(FALLBACK_SYSTEMS);
  const [error, setError] = useState<IpcError | null>(null);

  useEffect(() => {
    mounted.current = true;
    getSystems()
      .then((response) => {
        if (mounted.current) {
          setSystems(response.systems.map(({ id, displayName }) => ({ id, displayName })));
          setError(null);
        }
      })
      .catch((reason: unknown) => {
        if (mounted.current) {
          setError(normalizeIpcError(reason));
        }
      });

    return () => {
      mounted.current = false;
    };
  }, []);

  return { systems, error };
}
