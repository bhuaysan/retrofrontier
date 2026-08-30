import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import type { ManagedRuntimeModel } from '../../hooks/useManagedRuntime';
import {
  installErrorTitle,
  isRetryableInstallError,
  runtimeSourceLabel,
  runtimeSummary,
} from './runtimeStatus';

interface RuntimePanelProps {
  runtime: ManagedRuntimeModel;
}

/**
 * The managed RetroArch runtime section of Settings.
 *
 * It is deliberately small. Its whole job is to make the route from `NOT INSTALLED` to `READY`
 * visible and honest: it never claims a state RuntimeManager has not verified, never enables an
 * action that cannot succeed, and never hides a failure to make the screen look tidier.
 */
export function RuntimePanel({ runtime }: RuntimePanelProps) {
  const { state, loading, stateError, installError, pending } = runtime;
  const summary = state ? runtimeSummary(state) : null;
  const busy = pending || Boolean(state?.installing);
  const actionDisabled = busy || summary === null || summary.action === 'none';

  const runAction = () => {
    if (summary?.action === 'install') {
      void runtime.install();
    } else if (summary?.action === 'repair') {
      void runtime.repair();
    }
  };

  return (
    <section className="settings-panel settings-group" aria-labelledby="runtime-heading">
      <div className="panel-heading">
        <h2 id="runtime-heading">RETROARCH RUNTIME</h2>
        <span aria-hidden="true" />
        <span className="panel-meta">
          {loading && !state ? 'CHECKING' : (summary?.badge ?? '')}
        </span>
      </div>

      {stateError && (
        <InlineError
          title="RUNTIME STATUS UNAVAILABLE"
          message="RetroFrontier could not read the managed runtime state."
          actionLabel="RETRY"
          onAction={() => void runtime.refresh()}
        />
      )}

      {loading && !state && (
        <p className="loading-inline" role="status">
          READING RUNTIME STATE…
        </p>
      )}

      {summary && (
        <>
          <p className="runtime-detail" role="status">
            {summary.detail}
          </p>
          <dl className="runtime-facts">
            {state?.status.releaseId && (
              <div>
                <dt>RELEASE</dt>
                <dd>{state.status.releaseId}</dd>
              </div>
            )}
            {state?.status.installationId && (
              <div>
                <dt>INSTALLATION</dt>
                <dd>{state.status.installationId}</dd>
              </div>
            )}
            {state?.sourceOrigin && (
              <div>
                <dt>SOURCE</dt>
                <dd>{runtimeSourceLabel(state.sourceOrigin)}</dd>
              </div>
            )}
          </dl>
          {busy && (
            <p className="runtime-progress" role="status" aria-live="polite">
              DOWNLOADING AND VERIFYING THE APPROVED RELEASE… THIS CAN TAKE SEVERAL MINUTES.
            </p>
          )}
          {summary.disabledReason && !busy && (
            <p className="empty-inline">{summary.disabledReason}</p>
          )}
        </>
      )}

      {installError && (
        <InlineError
          title={installErrorTitle(installError.code)}
          message={installError.message}
          actionLabel={isRetryableInstallError(installError.code) ? 'TRY AGAIN' : undefined}
          onAction={isRetryableInstallError(installError.code) ? runAction : undefined}
        />
      )}

      <div className="settings-panel-actions">
        <PixelButton type="button" disabled={actionDisabled} onClick={runAction}>
          {busy ? 'INSTALLING…' : (summary?.actionLabel ?? 'INSTALL RUNTIME')}
        </PixelButton>
      </div>
    </section>
  );
}
