//! `fsmp` — FSM Prompter: prompt-driven workflows backed by extended finite
//! state machines.
//!
//! A **definition** ([`model::Definition`]) is a static, human-authored
//! workflow: states carrying prose guidance, guarded transitions with effects,
//! read-only params, and mutable context. An **instance**
//! ([`model::Instance`]) is one live run — a snapshot of the definition taken
//! at creation, plus the current state, the context, and a transition log.
//!
//! The pieces:
//!
//! - [`model`] — the data types for definitions and instances.
//! - [`engine`] — guard evaluation, effect application, and `{var}`
//!   interpolation, implemented as methods on [`model::Instance`].
//! - [`render`] — turns an instance into the step text an agent acts on
//!   (prose or JSON).
//! - [`lint`] — checks a definition for authoring problems (unreachable
//!   states, dead ends, unknown targets) without instantiating it.
//! - [`store`] — definition parsing/validation and the on-disk instance
//!   layout under `$FSMP_HOME` (default `~/.fsmp`).
//! - [`guide`] — the embedded authoring/driving reference docs.
//!
//! The primary interface is the `fsmp` CLI (this crate's binary), whose
//! primary user is an AI coding agent driving one transition at a time. This
//! library exposes the same engine for embedding in another transport or
//! harness. The API is pre-1.0 and may change.

pub mod engine;
pub mod guide;
pub mod lint;
pub mod model;
pub mod render;
pub mod store;
