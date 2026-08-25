# ADR-002: SQLite behind the Rust application layer
- Status: Accepted

## Context
Direct frontend SQL would spread domain logic across frontend/backend.

## Decision
Use SQLite with `sqlx` in Rust. React uses typed application IPC and never issues SQL directly.

## Consequences
Domain rules/migrations/repositories stay testable and centralized. Simple UI queries still require an application path.
