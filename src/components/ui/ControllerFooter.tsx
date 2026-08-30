import { useFocusApi, useFocusedNodeId } from '../../focus/focusContext';
import { deriveFooterHints, NO_SUPPORTED_ACTIONS } from '../../focus/footerHints';

interface ControllerFooterProps {
  /** The existing shell status line. */
  status: string;
  /** False while a managed game or another window owns input; no action may be claimed then. */
  interactive: boolean;
  controllerConnected: boolean;
  /** True while a managed game is running, which is why RetroFrontier is not consuming input. */
  gameRunning: boolean;
}

/**
 * The shell footer.
 *
 * Hints are derived from the focus model — the focused node's declared actions and the active
 * scope's back behaviour — never hard-coded per page. An action that the current focus target does
 * not support is simply absent, so the footer cannot claim a button does something it does not.
 */
export function ControllerFooter({
  status,
  interactive,
  controllerConnected,
  gameRunning,
}: ControllerFooterProps) {
  const api = useFocusApi();
  // Subscribing to the focused identity is what re-renders the footer as focus moves.
  useFocusedNodeId();
  const hints = deriveFooterHints(interactive ? api.getSupportedActions() : NO_SUPPORTED_ACTIONS);

  return (
    <footer className="app-footer">
      <span>LOCAL LIBRARY</span>
      <span aria-hidden="true">·</span>
      <span>{status}</span>
      <span className="footer-spacer" />
      {gameRunning ? (
        <span className="footer-note">RETROARCH HAS CONTROLLER INPUT</span>
      ) : (
        <ul aria-label="Controller actions" className="footer-hints">
          {hints.map((hint) => (
            <li className="footer-hint" key={hint.action}>
              <span aria-hidden="true" className="footer-hint-button">
                {hint.button}
              </span>
              <span className="visually-hidden">Button {hint.button}:</span>
              <span className="footer-hint-label">{hint.label}</span>
            </li>
          ))}
        </ul>
      )}
      <span className="footer-note">
        {controllerConnected ? 'CONTROLLER CONNECTED' : 'ROM files stay on your disk'}
      </span>
    </footer>
  );
}
