import type { SVGProps } from 'react';

type PixelIconProps = SVGProps<SVGSVGElement> & { label?: string };

export function PixelArrow({
  direction = 'right',
  label,
  ...props
}: PixelIconProps & { direction?: 'left' | 'right' }) {
  const path =
    direction === 'left'
      ? 'M1 0h1v1h1v1h1v1h-1v1h-1v1h-1v1h-1v-1h1v-1h1v-1h-1v-1h-1v-1h1z'
      : 'M0 0h1v1h1v1h1v1h-1v1h-1v1h-1v1h-1v-1h1v-1h1v-1h-1v-1h-1v-1h1z';
  return (
    <svg
      {...props}
      viewBox="0 0 4 7"
      width={props.width ?? 9}
      height={props.height ?? 12}
      shapeRendering="crispEdges"
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
