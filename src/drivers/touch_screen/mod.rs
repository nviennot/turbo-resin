// SPDX-License-Identifier: GPL-3.0-or-later

#[cfg(feature="saturn")]
mod saturn;
#[cfg(feature="saturn")]
pub use saturn::*;

#[cfg(feature="mono4k")]
mod mono4k;
#[cfg(feature="mono4k")]
pub use mono4k::*;

// TODO Merge the two implementation in one.
