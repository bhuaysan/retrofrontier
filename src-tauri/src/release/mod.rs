//! Managed Runtime Release construction and publication (M7.5).
//!
//! This module is maintainer tooling, not client code. It is behind the non-default
//! `release-tools` feature so that release construction, TUF repository publication, and signing
//! never ship inside the RetroFrontier application binary.
//!
//! The flow it implements is:
//!
//! ```text
//! committed release definition
//!   -> pinned upstream inputs (verified length + SHA-256)
//!   -> derived component artefacts (verified against their own pins)
//!   -> canonical release manifest + runtime policy
//!   -> proof extraction through the real client extractor and inventory verification
//!   -> signed TUF 1.0 repository the production ToughTrustedReleaseSource can consume
//! ```

pub mod canonical;
pub mod construct;
pub mod definition;
pub mod inventory;
pub mod tuf;

#[cfg(test)]
mod qualification;
#[cfg(test)]
mod roundtrip_tests;
