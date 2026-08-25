# ADR-008: Controller navigation is a core UX capability
- Status: Accepted

## Context
RetroFrontier may be used as a couch/TV library. Retrofitting controller focus later would be expensive.

## Decision
Treat controller/keyboard focus as foundational. Map hardware to semantic actions (Navigate, Confirm, Back, Context, Search, Menu). Primary UI supports controller, keyboard, mouse.

## Consequences
Better console-like UX, but focus behavior must be designed/tested from the start.
