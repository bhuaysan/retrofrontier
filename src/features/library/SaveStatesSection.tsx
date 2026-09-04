import { useEffect, useRef, useState } from 'react';

import { InlineError } from '../../components/ui/InlineError';
import { PixelButton } from '../../components/ui/PixelButton';
import { FocusScope } from '../../focus/FocusProvider';
import { useFocusApi, useFocusNode, useFocusScope } from '../../focus/focusContext';
import { focusNodes, focusScopes } from '../../focus/focusNodes';
import type { SaveStatesModel } from '../../hooks/useSaveStates';
import type { SaveStateView } from '../../platform/ipc';
import {
  coreIdentityLabel,
  loadabilityHint,
  loadabilityLabel,
  saveStateTimeLabel,
  slotLabel,
} from './saveStateCopy';

interface SaveStatesSectionProps {
  saveStates: SaveStatesModel;
}

/** The card surface a Save State is currently showing. Exactly one of them at a time. */
type CardSurface = 'none' | 'options' | 'delete';

/** Which of this state's own actions is unresolved, if any. At most one ever is. */
type CardPending = 'load' | 'delete' | null;

/** How one state is named wherever it has to be named: in an action label, or in a confirmation. */
function stateLabel(view: SaveStateView): string {
  return `${slotLabel(view.slot)} · ${saveStateTimeLabel(view.updatedAt)}`;
}

/**
 * The 16/9 state thumbnail, or a neutral stand-in.
 *
 * `thumbnailRef` is an opaque native media reference, never a path, and the native side only serves
 * it after re-verifying the file. A reference that still fails to render therefore says nothing
 * about the state, so the placeholder is deliberately neutral — the same fallback pattern
 * `GameCover` uses, for the same reason: the frame must not read as a broken state.
 */
function SaveStateThumbnail({ view }: { view: SaveStateView }) {
  const [failedRef, setFailedRef] = useState<string | null>(null);
  const thumbnailRef = view.thumbnailRef;
  const label = stateLabel(view);

  if (thumbnailRef !== null && failedRef !== thumbnailRef) {
    return (
      <img
        alt={`Save state thumbnail for ${label}`}
        className="save-state-thumbnail"
        loading="lazy"
        onError={() => setFailedRef(thumbnailRef)}
        src={thumbnailRef}
      />
    );
  }

  return (
    <div
      aria-label={`No thumbnail for ${label}`}
      className="save-state-thumbnail-placeholder"
      role="img"
    />
  );
}

/**
 * One Save State's Options surface.
 *
 * Load and Delete are independent: retention may remove the only authenticated copy of a historical
 * core, which makes a state unloadable while it stays perfectly deletable. The two controls
 * therefore follow their own capability and never one shared "usable" flag.
 */
function SaveStateOptions({
  view,
  busy,
  onClose,
  onDelete,
  onLoad,
}: {
  view: SaveStateView;
  busy: boolean;
  onClose: () => void;
  onDelete: () => void;
  onLoad: () => void;
}) {
  const loadable = view.capabilities.loadability === 'ready' && !busy;
  const deletable = view.capabilities.deletable && !busy;
  const scopeRef = useFocusScope({
    id: focusScopes.saveStateOptions(view.id),
    dismissLabel: 'CLOSE',
    onDismiss: onClose,
    // Restoration is explicit, per user action, exactly as the launch scopes do it: the generic
    // mechanism cannot tell "the user closed this" from "the route or the card unmounted", and on a
    // removal neither this card nor its actions exist any more.
    restore: 'none',
  });
  const loadRef = useFocusNode({
    id: focusNodes.saveStateAction(view.id, 'load'),
    confirm: loadable ? { label: 'LOAD' } : null,
  });
  const deleteRef = useFocusNode({
    id: focusNodes.saveStateAction(view.id, 'delete'),
    confirm: deletable ? { label: 'DELETE' } : null,
  });

  return (
    <FocusScope id={focusScopes.saveStateOptions(view.id)}>
      <div
        aria-label={`Options for ${stateLabel(view)}`}
        className="save-state-menu"
        ref={scopeRef}
        role="group"
      >
        <button disabled={!loadable} onClick={onLoad} ref={loadRef} type="button">
          LOAD
        </button>
        <button disabled={!deletable} onClick={onDelete} ref={deleteRef} type="button">
          DELETE
        </button>
      </div>
    </FocusScope>
  );
}

/**
 * The delete confirmation.
 *
 * CANCEL is first in DOM order on purpose, so the scope's own `initialFocus: 'auto'` lands on it:
 * entry focus on a destructive default would let one stray `confirm` delete a state the user only
 * meant to look at.
 */
function SaveStateDeleteConfirmation({
  view,
  busy,
  onCancel,
  onConfirm,
}: {
  view: SaveStateView;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const scopeRef = useFocusScope({
    id: focusScopes.saveStateDelete(view.id),
    dismissLabel: 'CANCEL',
    onDismiss: onCancel,
    restore: 'none',
  });
  const cancelRef = useFocusNode({
    id: focusNodes.saveStateAction(view.id, 'cancel-delete'),
    confirm: { label: 'CANCEL' },
  });
  const confirmRef = useFocusNode({
    id: focusNodes.saveStateAction(view.id, 'confirm-delete'),
    confirm: busy ? null : { label: 'DELETE' },
  });

  return (
    <FocusScope id={focusScopes.saveStateDelete(view.id)}>
      <div
        aria-label={`Delete ${stateLabel(view)}`}
        className="save-state-confirm"
        ref={scopeRef}
        role="group"
      >
        <strong>DELETE SAVE STATE?</strong>
        <p>{`${stateLabel(view)} is removed permanently and cannot be restored.`}</p>
        <div className="save-state-confirm-actions">
          <PixelButton onClick={onCancel} ref={cancelRef} type="button" variant="secondary">
            CANCEL
          </PixelButton>
          <PixelButton disabled={busy} onClick={onConfirm} ref={confirmRef} type="button">
            {busy ? 'DELETING…' : 'DELETE'}
          </PixelButton>
        </div>
      </div>
    </FocusScope>
  );
}

/**
 * One Save State card.
 *
 * The card's semantic identity is carried by its LOAD control, exactly as a Library card's identity
 * is carried by its detail link: that is the control the card exists for, and a `confirm` it does
 * not declare then has no native activation to fall back to either. A state that may not be loaded
 * therefore refuses `confirm` completely, rather than quietly doing something else — its Options
 * control stays reachable, which is where Delete lives.
 */
function SaveStateCard({
  view,
  pending,
  onDelete,
  onLoad,
}: {
  view: SaveStateView;
  pending: CardPending;
  onDelete: (saveStateId: number) => void;
  onLoad: (saveStateId: number) => void;
}) {
  const api = useFocusApi();
  const [surface, setSurface] = useState<CardSurface>('none');
  const busy = pending !== null;
  const loadable = view.capabilities.loadability === 'ready' && !busy;
  const label = stateLabel(view);

  // The card's semantic identity is declared *on its LOAD control*, not on the card element. That
  // is what makes a refused load a complete refusal: with no declared `confirm` and a `disabled`
  // button there is no native activation left to fall back to either, so `confirm` performs
  // nothing at all rather than quietly doing something else. `context` therefore carries the run
  // itself — unlike `confirm`, it has no native fallback to delegate to.
  const loadRef = useFocusNode({
    id: focusNodes.saveState(view.id),
    confirm: loadable ? { label: 'LOAD' } : null,
    context: { label: 'OPTIONS', run: () => setSurface('options') },
  });
  const optionsRef = useFocusNode({
    id: focusNodes.saveStateAction(view.id, 'options'),
    confirm: { label: 'OPTIONS' },
  });

  /**
   * Hands focus back to this card after one of its own surfaces closes.
   *
   * A single immediate attempt, never `requestFocus`: a request would outlive the card, and a card
   * this delete is about to remove must not leave a pending request behind for a later Game Detail
   * to satisfy. The Options control is the truthful second choice, because an unloadable state's
   * LOAD is disabled and cannot take focus at all.
   */
  const restoreCardFocus = () => {
    if (api.focusNode(focusNodes.saveState(view.id))) return;
    if (api.focusNode(focusNodes.saveStateAction(view.id, 'options'))) return;
    api.focusNode(focusNodes.saveStatesHeading);
  };

  const closeSurface = () => {
    restoreCardFocus();
    setSurface('none');
  };

  const confirmDelete = () => {
    // Ordered deliberately: focus first, while this card is certainly still mounted. If the delete
    // succeeds the section then moves focus on to the card that took this one's place.
    restoreCardFocus();
    setSurface('none');
    onDelete(view.id);
  };

  return (
    <li className="save-state-card">
      <div className="save-state-media">
        <SaveStateThumbnail view={view} />
        <h3 className="save-state-slot">{slotLabel(view.slot)}</h3>
        <button
          aria-label={`Options for ${label}`}
          className="save-state-options"
          onClick={() => setSurface('options')}
          ref={optionsRef}
          type="button"
        >
          <span aria-hidden="true">⋮</span>
        </button>
      </div>

      <div className="save-state-copy">
        <span className="save-state-when">{saveStateTimeLabel(view.updatedAt)}</span>
        <span className="save-state-core">{coreIdentityLabel(view)}</span>
        {/* Only present when the game has more than one content unit, so a disc label appears
            exactly where it disambiguates and never as decoration. */}
        {view.contentUnitLabel !== null ? (
          <span className="save-state-disc">{view.contentUnitLabel}</span>
        ) : null}
        <p className="save-state-loadability" data-loadability={view.capabilities.loadability}>
          <span className="save-state-loadability-label">
            {loadabilityLabel(view.capabilities.loadability)}
          </span>
          <span className="save-state-loadability-hint">
            {loadabilityHint(view.capabilities.loadability)}
          </span>
        </p>
        <button
          aria-label={`Load ${label}`}
          className="save-state-load"
          disabled={!loadable}
          onClick={() => onLoad(view.id)}
          ref={loadRef}
          type="button"
        >
          {pending === 'load' ? 'LOADING…' : 'LOAD'}
        </button>
      </div>

      {surface === 'options' ? (
        <SaveStateOptions
          busy={busy}
          onClose={closeSurface}
          onDelete={() => setSurface('delete')}
          onLoad={() => {
            restoreCardFocus();
            setSurface('none');
            onLoad(view.id);
          }}
          view={view}
        />
      ) : null}

      {surface === 'delete' ? (
        <SaveStateDeleteConfirmation
          busy={busy}
          onCancel={closeSurface}
          onConfirm={confirmDelete}
          view={view}
        />
      ) : null}
    </li>
  );
}

/**
 * Game Detail's Save States section.
 *
 * It renders exactly what the backend delivered, in the order the backend delivered it: the
 * `updated_at DESC` ordering is the backend's decision, and a second opinion here would diverge
 * from it silently. Every capability value is treated as a UI snapshot — it decides what a control
 * says and whether it is offered, never whether an action is permitted, which only the backend
 * decides when it is actually asked.
 */
export function SaveStatesSection({ saveStates }: SaveStatesSectionProps) {
  const api = useFocusApi();
  const headingRef = useFocusNode({ id: focusNodes.saveStatesHeading });
  const states = saveStates.states;
  const pendingFor = (saveStateId: number): CardPending =>
    saveStates.loadPendingId === saveStateId
      ? 'load'
      : saveStates.deletePendingId === saveStateId
        ? 'delete'
        : null;
  /**
   * Where focus goes once a delete really removed a row, captured *before* the row is gone.
   *
   * The neighbours are recorded as `SaveStateId`s rather than positions, because the list is
   * reloaded from the backend and a position may then mean a different state entirely.
   */
  const deleteFocusPlan = useRef<{
    removedId: number;
    next: number | null;
    previous: number | null;
  } | null>(null);

  const requestDelete = (saveStateId: number) => {
    const index = states.findIndex((state) => state.id === saveStateId);
    deleteFocusPlan.current = {
      removedId: saveStateId,
      next: states[index + 1]?.id ?? null,
      previous: index > 0 ? states[index - 1].id : null,
    };
    void saveStates.delete(saveStateId);
  };

  useEffect(() => {
    const plan = deleteFocusPlan.current;
    if (plan === null || saveStates.deletePendingId !== null) return;
    deleteFocusPlan.current = null;
    // The row is still listed, so nothing was removed: the failure is rendered above and focus
    // stays where the card's own confirmation left it.
    if (saveStates.states.some((state) => state.id === plan.removedId)) return;
    for (const candidate of [plan.next, plan.previous]) {
      if (candidate !== null && api.focusNode(focusNodes.saveState(candidate))) return;
    }
    api.focusNode(focusNodes.saveStatesHeading);
  }, [api, saveStates.deletePendingId, saveStates.states]);

  return (
    <section aria-labelledby="save-states-heading" className="game-detail-section">
      {/* The section heading is spelled out here rather than reused from Game Detail's own helper,
          because it is also `focusNodes.saveStatesHeading` — the deterministic focus fallback for
          this section — and therefore needs the registry's callback ref. */}
      <div className="game-detail-section-heading">
        <h2 id="save-states-heading" ref={headingRef} tabIndex={-1}>
          SAVE STATES
        </h2>
        <span aria-hidden="true" className="game-detail-section-rule" />
        {states.length > 0 ? (
          <span className="game-detail-section-meta">
            {states.length} {states.length === 1 ? 'STATE' : 'STATES'}
          </span>
        ) : null}
      </div>

      {saveStates.error ? (
        <InlineError
          title="SAVE STATES UNAVAILABLE"
          message="RetroFrontier could not read this game's save states. The states themselves are untouched; local content and readiness remain available."
          actionLabel="RETRY SAVE STATES"
          onAction={() => void saveStates.retry()}
        />
      ) : null}

      {/* The backend already normalized this, so its message is rendered as it stands rather than
          being re-interpreted from a code the UI would have to guess the meaning of. */}
      {saveStates.actionFailure ? (
        <InlineError
          title="SAVE STATE ACTION FAILED"
          message={saveStates.actionFailure.message}
          actionLabel="DISMISS"
          onAction={saveStates.dismissActionFailure}
        />
      ) : null}

      {saveStates.loading && !saveStates.loaded ? (
        <p className="game-detail-inline-status" role="status">
          READING SAVE STATES…
        </p>
      ) : null}

      {states.length > 0 ? (
        <ul aria-label="Save states" className="save-state-grid">
          {states.map((view) => (
            <SaveStateCard
              key={view.id}
              onDelete={requestDelete}
              onLoad={(saveStateId) => void saveStates.load(saveStateId)}
              pending={pendingFor(view.id)}
              view={view}
            />
          ))}
        </ul>
      ) : saveStates.loaded && saveStates.error === null ? (
        <div className="save-state-empty">
          <strong>NO SAVE STATES YET</strong>
          <p>
            A save state appears here once the managed session that wrote it has ended. It is made
            with the controller, inside the game.
          </p>
          <span className="game-detail-kicker">IN GAME</span>
          <dl className="save-state-hotkeys">
            <div>
              <dt>SELECT + R1</dt>
              <dd>SAVE STATE</dd>
            </div>
            <div>
              <dt>SELECT + ← / →</dt>
              <dd>CHANGE SLOT</dd>
            </div>
          </dl>
        </div>
      ) : null}
    </section>
  );
}
