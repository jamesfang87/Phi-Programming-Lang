mod check_main;
mod mutability;

pub use check_main::check as check_entry_point;
pub use mutability::check as check_mutability;
pub(crate) use check_main::crate_root_main_candidates;
