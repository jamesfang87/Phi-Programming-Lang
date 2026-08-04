//! This file embeds the core library's source into the compiler binary and registers it with
//! the `SrcMap`.
//!
//! Phi has no notion of a separately compiled library yet, so `core` is compiled into the same
//! unit as the user's own files, from source, on every build. Its files carry ordinary
//! `module core::..;` declarations, so lowering assembles them into the module tree exactly as
//! it does the user's -- nothing downstream needs to know `core` is special.
//!
//! The one thing that is special is *when* they're registered: [`CoreLib::register`] runs after
//! the `FileCollector` has walked the project, so `core` sits at the end of the offset space and
//! editing it doesn't shift the span of every user file in the build.
//!
//! Which items `core` is expected to declare is not recorded here but in
//! [`crate::langitems`], which resolves each one to its `DefId` after name resolution has built
//! `core`'s namespace.

use crate::driver::src_file::FileOrigin;
use crate::driver::src_map::SrcMap;

/// Every file of the core library, as `(name, source)`.
///
/// The names are what diagnostics pointing into `core` are rendered against, so they're written
/// as paths relative to the compiler's own `lib/` directory rather than the absolute paths the
/// files had when the binary was built.
const CORE_FILES: &[(&str, &str)] = &[
    ("core/iter.phi", include_str!("../../lib/core/iter.phi")),
    ("core/ops.phi", include_str!("../../lib/core/ops.phi")),
    ("core/option.phi", include_str!("../../lib/core/option.phi")),
    (
        "core/prelude.phi",
        include_str!("../../lib/core/prelude.phi"),
    ),
    ("core/result.phi", include_str!("../../lib/core/result.phi")),
];

/// Registers every core library file with the `SrcMap`, in the order [`CORE_FILES`] lists
/// them.
pub fn register() {
    for &(name, source) in CORE_FILES {
        SrcMap::add_file(name.to_string(), source.chars().collect(), FileOrigin::Core);
    }
}
