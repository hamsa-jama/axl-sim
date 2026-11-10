//! The logic behind the Code screen, free of any UI so it can be tested without a window:
//! the API reference parsed from `axl.h`, syntax highlighting, project folders with file
//! watching, and build sessions that stream a log and collect diagnostics.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod apiref;
pub mod highlight;
pub mod project;
pub mod session;
