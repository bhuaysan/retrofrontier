import { PixelButton } from '../../components/ui/PixelButton';
import { WarningIcon } from '../../components/ui/PixelIcon';
import type { IpcError } from '../../platform/ipc';
import { rootErrorCopy } from './RootErrorMessage';

interface RootActionErrorProps {
  error: IpcError;
  onAction: () => void;
  actionLabel?: string;
}

export function RootActionError({ error, onAction, actionLabel }: RootActionErrorProps) {
  const copy = rootErrorCopy(error);
  return (
    <aside className="root-action-error" role="alert">
      <WarningIcon className="inline-error-icon" />
      <div className="inline-error-copy">
        <strong>{copy.title}</strong>
        <p>{copy.message}</p>
      </div>
      <PixelButton type="button" variant="secondary" onClick={onAction}>
        {actionLabel ?? copy.actionLabel}
      </PixelButton>
    </aside>
  );
}
