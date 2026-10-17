//! The small vocabulary every other part of the app shares: number formatting, lenient integer
//! reading for the wire, the arithmetic on series, the look of the interface and the two names and
//! scales the whole process agrees on.
//!
//! Nothing here knows about charts, orders or screens. A thing used by one part only lives in that
//! part.

pub mod appearance;
pub mod defaults;
pub mod flex;
pub mod format;
pub mod intern;
pub mod math;
pub mod ta;
pub mod text_scale;
