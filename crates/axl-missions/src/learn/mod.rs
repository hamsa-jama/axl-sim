//! The learning design: warm-up questions with generated numbers, scheduled by FSRS-4.5.

pub mod fsrs;
pub mod warmup;

pub use fsrs::{Card, Grade};
pub use warmup::{Question, generate, pick_due};
