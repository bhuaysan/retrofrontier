import type { ButtonHTMLAttributes } from 'react';

import { useFocusNode } from '../../focus/focusContext';
import type { FocusNodeId } from '../../focus/focusNodes';
import { PixelArrow } from './PixelIcon';

interface PixelRowProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'children'> {
  label: string;
  count?: number;
  accent: string;
  active?: boolean;
  activeMode?: 'current' | 'pressed';
  /** The stable identity this row is known by for controller navigation and focus restoration. */
  focusId?: FocusNodeId;
  /** Footer copy for `confirm` on this row. */
  confirmLabel?: string;
}

export function PixelRow({
  label,
  count,
  accent,
  active = false,
  activeMode = 'current',
  focusId,
  confirmLabel = 'OPEN',
  ...props
}: PixelRowProps) {
  const focusRef = useFocusNode({
    id: focusId ?? `row:${label}`,
    confirm: focusId === undefined ? null : { label: confirmLabel },
  });

  return (
    <li className="pixel-row-shell">
      <span className="pixel-row-cursor" aria-hidden="true">
        <PixelArrow />
      </span>
      <button
        className={`pixel-row${active ? ' pixel-row--active' : ''}`}
        aria-current={active && activeMode === 'current' ? 'page' : undefined}
        aria-pressed={activeMode === 'pressed' ? active : undefined}
        ref={focusId === undefined ? undefined : focusRef}
        {...props}
      >
        <span className="system-swatch" style={{ background: accent }} aria-hidden="true" />
        <span className="pixel-row-label">{label}</span>
        {count !== undefined && <span className="pixel-row-count">{count}</span>}
      </button>
    </li>
  );
}
