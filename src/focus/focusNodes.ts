/**
 * Stable semantic focus identities.
 *
 * Focus restoration keys off these identities, never off a DOM selector or an element reference,
 * so a re-rendered, re-queried, or re-paged surface can hand focus back to the same logical target.
 */
export type FocusNodeId = string;

/** Scopes partition the navigable surface. `root` is the ordinary application scope. */
export type FocusScopeId = string;

/**
 * Zones partition one *ordinary* screen into regions that semantic directional movement does not
 * cross on its own.
 *
 * A zone is not a scope. A scope is a temporary surface that owns focus and refuses reach-through
 * while it is open; a zone is a permanent part of a screen's layout that only answers one question:
 * which region does the focused element belong to. Crossing between zones is therefore an explicit
 * transition — a confirmed sidebar entry, a `back` — rather than a consequence of which element
 * happens to lie in the pressed direction.
 */
export type FocusZoneId = string;

export const ROOT_FOCUS_SCOPE: FocusScopeId = 'root';

export const focusNodes = {
  /** A Library game's own Game Detail target — the native card link. */
  libraryGame: (gameId: number): FocusNodeId => `library:game:${gameId}`,
  libraryHeading: 'library:heading' as FocusNodeId,
  /**
   * A system shelf's View All control in the All Systems browse view.
   *
   * The games on a shelf keep their ordinary `libraryGame` identity: one rendered Library view
   * shows a game once, so it needs one semantic identity, and a shelf-qualified game identity would
   * only make focus restoration guess which representation it meant. View All is genuinely new, so
   * it gets an identity of its own.
   */
  libraryShelfViewAll: (systemId: string): FocusNodeId => `library:shelf:view-all:${systemId}`,
  /** The shared top bar's Library Search field. Outside both Library zones by design. */
  librarySearch: 'library:search' as FocusNodeId,
  /** `null` is the "all systems" row. */
  sidebarSystem: (systemId: string | null): FocusNodeId => `sidebar:system:${systemId ?? 'all'}`,
  sidebarRoute: (route: string): FocusNodeId => `sidebar:route:${route}`,
  shell: (action: string): FocusNodeId => `shell:${action}`,
  detail: (action: string): FocusNodeId => `detail:${action}`,
  detailCandidate: (providerGameId: string): FocusNodeId => `detail:candidate:${providerGameId}`,
  /** A launch content choice, identified by its `ContentUnitId`. */
  launchContent: (contentUnitId: number): FocusNodeId => `launch:content:${contentUnitId}`,
  /** The Save States section heading — the deterministic fallback for the section. */
  saveStatesHeading: 'save-states:heading' as FocusNodeId,
  /** A Save State card, identified by its SaveStateId and never by array position. */
  saveState: (saveStateId: number): FocusNodeId => `save-state:${saveStateId}`,
  /** An action inside a Save State's own options or delete-confirmation surface. */
  saveStateAction: (saveStateId: number, action: string): FocusNodeId =>
    `save-state:${saveStateId}:${action}`,
  settings: (action: string): FocusNodeId => `settings:${action}`,
  /** The managed-runtime primary action. Its label and activatability follow runtime state. */
  settingsRuntime: (action: string): FocusNodeId => `settings:runtime:${action}`,
  settingsRoot: (rootId: number, action: string): FocusNodeId =>
    `settings:root:${rootId}:${action}`,
  /** A Library Scraper control. Its identity is the action, not the run it happens to describe. */
  settingsScrape: (action: string): FocusNodeId => `settings:scrape:${action}`,
} as const;

export const focusScopes = {
  launchContentSelection: 'scope:launch-content' as FocusScopeId,
  /** A normalized launch failure. Temporary, and it owns `back` while it is open. */
  launchFailure: 'scope:launch-failure' as FocusScopeId,
  rootRemoval: (rootId: number): FocusScopeId => `scope:settings-root-removal:${rootId}`,
  metadataAccountClear: 'scope:settings-metadata-clear' as FocusScopeId,
  /** Confirming a scraper stop. Temporary, and it owns `back` while it is open. */
  metadataScrapeStop: 'scope:settings-scrape-stop' as FocusScopeId,
  /** One Save State's Options surface. Temporary, and it owns `back` while it is open. */
  saveStateOptions: (saveStateId: number): FocusScopeId =>
    `scope:save-state-options:${saveStateId}`,
  /** Confirming a Save State deletion. Temporary; entry focus is deliberately Cancel. */
  saveStateDelete: (saveStateId: number): FocusScopeId => `scope:save-state-delete:${saveStateId}`,
} as const;

export const focusZones = {
  /** The Library's left sidebar: system filters and menu destinations. */
  librarySidebar: 'zone:library-sidebar' as FocusZoneId,
  /** The Library's main content: the filter bar, the game grid, and pagination. */
  libraryMain: 'zone:library-main' as FocusZoneId,
} as const;
