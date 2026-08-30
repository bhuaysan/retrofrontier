import { PixelButton } from './PixelButton';
import { WarningIcon } from './PixelIcon';

interface InlineErrorProps {
  title?: string;
  message: string;
  actionLabel?: string;
  onAction?: () => void;
}

export function InlineError({
  title = 'ACTION UNAVAILABLE',
  message,
  actionLabel,
  onAction,
}: InlineErrorProps) {
  return (
    <aside className="inline-error" role="alert">
      <WarningIcon className="inline-error-icon" />
      <div className="inline-error-copy">
        <strong>{title}</strong>
        <p>{message}</p>
      </div>
      {actionLabel && onAction && (
        <PixelButton type="button" variant="secondary" onClick={onAction}>
          {actionLabel}
        </PixelButton>
      )}
    </aside>
  );
}
