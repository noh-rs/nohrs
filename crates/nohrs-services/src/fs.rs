//! Directory listing with stable, case-insensitive ordering and cursor paging,
//! plus synchronous file mutation operations (see [`ops`]).

/// Synchronous directory listing.
pub mod listing;
/// Synchronous file mutation operations with cross-volume and conflict handling.
pub mod ops;
/// Listing, restoring from, and emptying the trash.
pub mod trash;
/// The record of what nohrs moved to the trash, used to restore it.
pub mod trash_ledger;
