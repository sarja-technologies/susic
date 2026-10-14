//! Shared domain types and pure logic for Susic: track model, queue/shuffle
//! engine, LRC lyric parsing. No I/O, so it is trivially testable and shared by
//! the desktop and Android builds.

pub mod lyrics;
pub mod model;
pub mod queue;

pub use lyrics::{LyricLine, Lyrics};
pub use model::*;
pub use queue::{PlayQueue, RepeatMode};
