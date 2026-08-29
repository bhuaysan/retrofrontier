import type { SVGProps } from 'react';

type PixelIconProps = SVGProps<SVGSVGElement> & { label?: string };

export function PixelArrow({
  direction = 'right',
  label,
  ...props
}: PixelIconProps & { direction?: 'left' | 'right' }) {
  // A filled directional triangle with continuous diagonals keeps the shared arrow language smooth
  // at both the small sidebar cursor size and the larger section-heading size.
  const path = direction === 'left' ? 'M8.5 1L1.5 7L8.5 13Z' : 'M1.5 1L8.5 7L1.5 13Z';
  return (
    <svg
      {...props}
      viewBox="0 0 10 14"
      width={props.width ?? 10}
      height={props.height ?? 14}
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path d={path} fill="currentColor" />
    </svg>
  );
}

export function FolderIcon({ label, ...props }: PixelIconProps) {
  return (
    <svg
      {...props}
      viewBox="0 0 16 14"
      width={props.width ?? 18}
      height={props.height ?? 16}
      shapeRendering="crispEdges"
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path d="M1 1h5l2 2h7v10H1z" fill="currentColor" />
      <path d="M2 4h12v7H2z" fill="var(--surface-2)" />
    </svg>
  );
}

export function LibraryIcon({ label, ...props }: PixelIconProps) {
  return (
    <svg
      {...props}
      viewBox="0 0 7 6"
      width={props.width ?? 64}
      height={props.height ?? 55}
      shapeRendering="crispEdges"
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path
        d="M0 0h1v1h-1zM1 0h1v1h-1zM2 0h1v1h-1zM0 1h1v1h-1zM3 1h1v1h-1zM4 1h1v1h-1zM5 1h1v1h-1zM6 1h1v1h-1zM0 2h1v1h-1zM6 2h1v1h-1zM0 3h1v1h-1zM6 3h1v1h-1zM0 4h1v1h-1zM6 4h1v1h-1zM0 5h1v1h-1zM1 5h1v1h-1zM2 5h1v1h-1zM3 5h1v1h-1zM4 5h1v1h-1zM5 5h1v1h-1zM6 5h1v1h-1z"
        fill="currentColor"
      />
    </svg>
  );
}

export function ExternalLinkIcon({ label, ...props }: PixelIconProps) {
  return (
    <svg
      {...props}
      viewBox="0 0 12 12"
      width={props.width ?? 14}
      height={props.height ?? 14}
      shapeRendering="crispEdges"
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path d="M7 0h5v5h-2V3.5L5 8.5 3.5 7 8.5 2H7zM0 2h5v2H2v6h6V7h2v5H0z" fill="currentColor" />
    </svg>
  );
}

export function WarningIcon({ label, ...props }: PixelIconProps) {
  return (
    <svg
      {...props}
      viewBox="0 0 12 12"
      width={props.width ?? 14}
      height={props.height ?? 14}
      shapeRendering="crispEdges"
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path
        d="M5 0h2v2h1v2h1v2h1v2h1v3H0V8h1V6h1V4h1V2h2zM5 4h2v3H5zm0 4h2v2H5z"
        fill="currentColor"
      />
    </svg>
  );
}

/**
 * Hard-edged 12×12 pixel star for the Favorite action. The unfilled variant is the same silhouette
 * with a one-pixel interior cut out through `evenodd`, so both states stay crisp and unmistakably a
 * star at small sizes instead of closing into a blob the way a stroked outline did.
 */
export function PixelStar({ filled }: { filled: boolean }) {
  const silhouette =
    'M5 0h2v2h-2zM4 2h4v2h-4zM0 4h12v1h-12zM1 5h10v1h-10zM2 6h8v1h-8z' +
    'M3 7h6v1h-6zM2 8h8v1h-8zM1 9h3v1h-3zM8 9h3v1h-3zM0 10h3v1h-3z' +
    'M9 10h3v1h-3zM0 11h2v1h-2zM10 11h2v1h-2z';
  const interior =
    'M5 2h2v2h-2zM4 4h4v1h-4zM2 5h8v1h-8zM3 6h6v1h-6zM4 7h4v1h-4z' +
    'M3 8h1v1h-1zM8 8h1v1h-1zM2 9h1v1h-1zM9 9h1v1h-1zM1 10h1v1h-1zM10 10h1v1h-1z';

  return (
    <svg aria-hidden="true" shapeRendering="crispEdges" viewBox="0 0 12 12">
      <path
        d={filled ? silhouette : `${silhouette}${interior}`}
        fill="currentColor"
        fillRule="evenodd"
      />
    </svg>
  );
}

/**
 * B1 selection checkmark: the design reference's 5×5 pixel tick, drawn crisp at any size. It is
 * project-owned geometry rather than a Unicode glyph or user-agent checkbox chrome, both of which
 * render inconsistently against the 22px hard-edged control.
 */
export function PixelCheck({ label, ...props }: PixelIconProps) {
  return (
    <svg
      {...props}
      viewBox="0 0 5 5"
      width={props.width ?? 10}
      height={props.height ?? 10}
      shapeRendering="crispEdges"
      aria-hidden={label ? undefined : true}
      aria-label={label}
      role={label ? 'img' : undefined}
    >
      <path
        d="M4 0h1v1h-1zM3 1h1v1h-1zM4 1h1v1h-1zM0 2h1v1h-1zM2 2h1v1h-1zM3 2h1v1h-1zM0 3h1v1h-1zM1 3h1v1h-1zM2 3h1v1h-1zM1 4h1v1h-1z"
        fill="currentColor"
      />
    </svg>
  );
}
