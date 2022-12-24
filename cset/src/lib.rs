//! Fine-grained and reversible struct transactions.
//!
//! This crate offers a Track macro that structs can derive to generate the
//! plumbing needed to precisely track changes to fields. Calling
//! `edit()` returns a draft that stores edits separately from
//! the underyling struct, such that no values are written.
//!
//! When `apply()` is called on the draft, edits are applied to the
//! base struct. Each replaced value is returned to the caller as a [Change] in
//! a [ChangeSet]. This changeset can then be re-applied to a struct of the same
//! type, which replaces fields with values from the [ChangeSet]. This operation
//! produces a new [ChangeSet], allowing for the implementation of an undo-redo
//! paradigm.
//!
//! # Example
//!
//! ```
//! use cset::Track;
//!
//! #[derive(Track)]
//! # #[derive(Debug, PartialEq)]
//! struct Foo {
//!     x: usize,
//!     #[track(flatten)]
//!     bar: Bar,
//! }
//! # impl Foo {
//! #     fn new(x: usize, bar: Bar) -> Self {
//! #         Self { x, bar }
//! #     }
//! # }
//!
//! #[derive(Track)]
//! # #[derive(Debug, PartialEq)]
//! struct Bar {
//!     y: usize,
//! }
//! # impl Bar {
//! #     fn new(y: usize) -> Self {
//! #         Self { y }
//! #     }
//! # }
//!
//! let mut foo = Foo::new(10, Bar::new(42));
//!
//! // Enter the non-destructive editing mode
//! let mut foo_draft = foo.edit();
//! foo_draft.set_x(42);
//! foo_draft.edit_bar().set_y(1024);
//!
//! // Drop the draft to rollback, or apply the changes with `.apply()`
//! let undo_changeset = foo_draft.apply();
//! assert_eq!(foo, Foo::new(42, Bar::new(1024)));
//!
//! let redo_changeset = foo.apply(undo_changeset);
//! assert_eq!(foo, Foo::new(10, Bar::new(42)));
//!
//! foo.apply(redo_changeset);
//! assert_eq!(foo, Foo::new(42, Bar::new(1024)));
//! ```

use std::any::{Any, TypeId};

pub use cset_derive::Track;

#[derive(Debug)]
pub struct ChangeSet {
    pub target_type: TypeId,
    pub changes: Vec<Change>,
}

impl ChangeSet {
    pub fn new<T: 'static>(changes: Vec<Change>) -> Self {
        ChangeSet {
            target_type: TypeId::of::<T>(),
            changes,
        }
    }

    pub fn for_type<T: 'static>(&self) -> bool {
        self.target_type == TypeId::of::<T>()
    }
}

#[derive(Debug)]
pub enum ChangeValue {
    Value(Box<dyn Any>),
    ChangeSet(ChangeSet),
}

#[derive(Debug)]
pub struct Change {
    pub field_id: FieldId,
    pub value: ChangeValue,
}

#[derive(Debug)]
pub struct DraftField<'b, T: 'static> {
    pub original: &'b mut T,
    pub draft: Option<T>,
}

impl<'b, T> DraftField<'b, T> {
    pub fn new(original: &'b mut T) -> Self {
        Self {
            original,
            draft: None,
        }
    }

    pub fn apply(self, field_idx: FieldId) -> Option<Change> {
        self.draft.map(|new_value| {
            let old_value = std::mem::replace(self.original, new_value);
            let boxed: Box<dyn Any> = Box::new(old_value);
            Change {
                field_id: field_idx,
                value: ChangeValue::Value(boxed),
            }
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct FieldId(Vec<usize>);

impl FieldId {
    pub fn push_field(&self, child_field: usize) -> Self {
        let mut new = self.clone();
        new.0.push(child_field);
        new
    }

    pub fn field_index(&self, depth: usize) -> usize {
        self.0[depth]
    }
}
