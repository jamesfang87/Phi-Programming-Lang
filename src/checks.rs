//! Checks that run over a type-checked program but are not themselves type checking.

// TODO: I don't like how theres these very little, trivial checks which are elevated
// to the global root module. that is not something I'd want
pub mod entry_point;
pub mod mutability;
