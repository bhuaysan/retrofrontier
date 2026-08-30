interface LibrarySelectionBarProps {
  count: number;
  onClear: () => void;
}

/**
 * B1 selection bar. It sits between the filter toolbar and the LIBRARY heading and only exists
 * while something is selected.
 *
 * B1 also shows a "METADATEN KORRIGIEREN" action next to it. That is deliberately omitted: there
 * is no accepted metadata-correction workflow behind it yet, and a disabled or placeholder button
 * would fabricate functionality. The bar therefore carries only real current behavior.
 */
export function LibrarySelectionBar({ count, onClear }: LibrarySelectionBarProps) {
  return (
    <div aria-label="Library selection" className="library-selection-bar" role="group">
      <p aria-live="polite" className="library-selection-count">
        {count} SELECTED
      </p>
      <span aria-hidden="true" className="library-selection-spacer" />
      <button className="library-selection-clear" onClick={onClear} type="button">
        CLEAR SELECTION
      </button>
    </div>
  );
}
