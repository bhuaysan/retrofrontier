# ADR-001: Tauri, React, TypeScript, and Rust
- Status: Accepted

## Context
RetroFrontier needs a cross-platform desktop UI plus deep filesystem, process, SQLite, download, and OS integration.

## Decision
Use Tauri 2, React + TypeScript + Vite, Rust, and pnpm.

## Consequences
Benefits: lightweight desktop shell, strong native capabilities, shared UI stack, type safety.

Constraints: clear IPC boundaries and real cross-platform testing are required. React must not own native/domain behavior.
