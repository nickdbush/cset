//! Fine-grained and reversible struct transactions.
//!
//! This crate offers a Track macro that structs can derive to generate the
//! plumbing needed to precisely track changes to fields. Calling
//! [`Trackable::edit()`] returns a [Draft] that stores edits separately from
//! the underyling struct, such that no values are written.
//!
//! When [`Draft::commit()`] is called on the draft, edits are applied to the
//! base struct. Each replaced value is returned to the caller as a [Change] in
//! a [ChangeSet]. This changeset can then be re-applied to a struct of the same
//! type, which replaces fields with values from the [ChangeSet]. This operation
//! produces a new [ChangeSet], allowing for the implementation of an undo-redo
//! paradigm.
//!
//! # Example
//!
//! ```
//! use cset::{Track, Trackable, Draft};
//!
//! #[derive(Track)]
//! struct Foo {
//!     bar: i32,
//! }
//!
//! // Create a new `Foo` as normal
//! let mut foo = Foo { bar: 0 };
//!
//! // Make a tracked change
//! let undo_cset = foo.edit().set_bar(42).commit();
//! assert_eq!(foo.bar, 42);
//!
//! // Undo the change by applying the returned changeset
//! let redo_cset = foo.apply_changeset(undo_cset);
//! assert_eq!(foo.bar, 0);
//!
//! // Redo the change by applying the changeset produced by undoing
//! foo.apply_changeset(redo_cset);
//! assert_eq!(foo.bar, 42);
//! ```

use std::any::{Any, TypeId};

pub use cset_derive::Track;

/// Auto-implemented by the [Track] macro.
pub trait Trackable<'a> {
    type Draft: Draft<'a>;

    fn edit(&'a mut self) -> Self::Draft;

    fn apply_changeset(&mut self, changeset: ChangeSet) -> ChangeSet;
}

/// An interface for non-destructive and tracked changes to underlying data.
///
/// Four methods are generated for each field in the underlying [Trackable]
/// struct (which produced this draft).
///
/// - `.get_{field}()` (gets the modified value, or the value from the
///         underlying struct)
/// - `.set_{field}(new_value: T)` (sets a new value for the field, obscuring
///         the value from the underyling struct)
/// - `.is_{field}_dirty() -> bool` (tests whether a new value has been set
///         directly on this draft)
/// - `.reset_{field}() -> Option<T>` (returns ownership of the value set on the
///         draft if one exists)
pub trait Draft<'a> {
    /// Replaces values on the underyling [Trackable] struct with those from
    /// this draft. Replaced values are returned in the [ChangeSet].
    fn commit(self) -> ChangeSet;
}

/// Generic storage of values replaced by [`Draft::commit()`].
pub struct ChangeSet {
    target_type: TypeId,
    changes: Vec<Change>,
}

impl ChangeSet {
    pub const fn new(target_type: TypeId, changes: Vec<Change>) -> Self {
        Self {
            target_type,
            changes,
        }
    }

    /// The [TypeId] of the struct that this [ChangeSet] relates to.
    ///
    /// [`Trackable::apply_changeset()`] panics when called with a changeset
    /// produced for another [Trackable] struct.
    pub const fn target_type(&self) -> TypeId {
        self.target_type
    }

    pub fn changes(&self) -> &[Change] {
        &self.changes
    }

    pub fn take_changes(self) -> Vec<Change> {
        self.changes
    }
}

/// A change to one field of a [Trackable] struct.
pub struct Change {
    field: &'static str,
    old_value: Box<dyn Any>,
}

impl Change {
    pub const fn new(field: &'static str, old_value: Box<dyn Any>) -> Self {
        Self { field, old_value }
    }

    pub const fn field(&self) -> &str {
        self.field
    }

    pub fn take_old_value(self) -> Box<dyn Any> {
        self.old_value
    }
}

/// Tracks whether the fields of a [Draft] have been modified.
pub enum DraftField<T> {
    /// The value of the underlying base struct has not changed.
    Unchanged,
    /// The value of the underyling base struct has been changed. If the
    /// transaction is committed, the old value will be returned to the caller
    /// as part of the [ChangeSet].
    Changed(T),
}
