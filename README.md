![drates.io](https://img.shields.io/crates/v/cset)
![docs.rs](https://img.shields.io/docsrs/cset)

# cset

Fine-grained and reversible struct transactions.

This crate offers a Track macro that structs can derive to generate the plumbing needed to precisely track changes to fields. Calling `Trackable::edit()` returns a `Draft` that stores edits separately from the underyling struct, such that no values are written.

When `.commit()` is called on the draft, edits are applied to the base struct. Each replaced value is returned to the caller as a `Change` in a `ChangeSet`. This changeset can then be re-applied to a struct of the same type, which replaces fields with values from the `ChangeSet`. This operation produces a new `ChangeSet`, allowing for the implementation of an undo-redo paradigm.

## Project status

This project is early in development and API changes should be expected.
