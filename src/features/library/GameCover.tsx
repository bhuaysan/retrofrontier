import { useState, type CSSProperties } from 'react';

interface GameCoverProps {
  title: string;
  coverRef: string | null;
  accent: string;
  resetKey: object;
  loading?: 'lazy' | 'eager';
  className?: string;
  placeholderClassName?: string;
}

export function GameCover({
  title,
  coverRef,
  accent,
  resetKey,
  loading = 'lazy',
  className = 'game-cover',
  placeholderClassName = className,
}: GameCoverProps) {
  const [failedCover, setFailedCover] = useState<{
    coverRef: string;
    resetKey: object;
  } | null>(null);
  const coverFailed =
    coverRef !== null && failedCover?.coverRef === coverRef && failedCover.resetKey === resetKey;

  if (coverRef !== null && !coverFailed) {
    return (
      <img
        alt={`Cover art for ${title}`}
        className={className}
        loading={loading}
        onError={() => setFailedCover({ coverRef, resetKey })}
        src={coverRef}
      />
    );
  }

  return (
    <div
      aria-label={`No cover available for ${title}`}
      className={placeholderClassName}
      role="img"
      style={{ '--system-accent': accent } as CSSProperties}
    >
      <span>{title}</span>
    </div>
  );
}
