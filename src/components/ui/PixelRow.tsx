import type { ButtonHTMLAttributes } from 'react';

import { PixelArrow } from './PixelIcon';

interface PixelRowProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  label: string;
  count?: number;
  accent: string;
  active?: boolean;
}

export function PixelRow({ label, count, accent, active = false, ...props }: PixelRowProps) {
  return (
    <li className="pixel-row-shell">
      <span className="pixel-row-cursor" aria-hidden="true">
        <PixelArrow width={10} height={14} />
      </span>
      <button
        className={`pixel-row${active ? ' pixel-row--active' : ''}`}
        aria-current={active ? 'page' : undefined}
        {...props}
      >
        <span className="system-swatch" style={{ background: accent }} aria-hidden="true" />
        <span className="pixel-row-label">{label}</span>
        {count !== undefined && <span className="pixel-row-count">{count}</span>}
      </button>
    </li>
  );
}
