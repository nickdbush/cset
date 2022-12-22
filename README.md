# cset

Fine-grained and reversible struct transactions.

This crate offers a Track macro that structs can derive to generate the plumbing needed to precisely track changes to fields. Calling `Trackable::edit()` returns a `Draft` that stores edits separately from the underyling struct, such that no values are written.

When `.commit()` is called on the draft, edits are applied to the base struct. Each replaced value is returned to the caller as a `Change` in a `ChangeSet`. This changeset can then be re-applied to a struct of the same type, which replaces fields with values from the `ChangeSet`. This operation produces a new `ChangeSet`, allowing for the implementation of an undo-redo paradigm.

## Example

```rust
use cset::{Track, Trackable, Draft};

#[derive(Track)]
struct Foo {
    bar: i32,
}

// Create a new `Foo` as normal
let mut foo = Foo { bar: 0 };

// Make a tracked change
let undo_cset = foo.edit().set_bar(42).commit();
assert_eq!(foo.bar, 42);

// Undo the change by applying the returned changeset
let redo_cset = foo.apply_changeset(undo_cset);
assert_eq!(foo.bar, 0);

// Redo the change by applying the changeset produced by undoing
foo.apply_changeset(redo_cset);
assert_eq!(foo.bar, 42);
```

## Project status

This project is early in development and API changes should be expected.
