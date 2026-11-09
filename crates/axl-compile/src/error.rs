//! Compile errors. The text of each is shown to the learner.

/// Why an assembly could not be compiled.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CompileError {
    /// A placed part is not in the catalog.
    #[error("There is no part called {0}.")]
    UnknownPart(String),
    /// Two placed parts share a uid.
    #[error("Two parts have the same id ({0}).")]
    DuplicateUid(u32),
    /// No hub.
    #[error("A robot needs exactly one hub.")]
    NoHub,
    /// More than one hub.
    #[error("A robot needs exactly one hub.")]
    MultipleHubs,
    /// Some parts are not connected to the hub.
    #[error("{0} parts are floating. Connect them or delete them.")]
    Floating(usize),
    /// Two meshed gears sit on axles held by different parts.
    #[error("Gears must turn on axles held by the same part")]
    GearParents {
        /// Uid of the first gear.
        a: u32,
        /// Uid of the second gear.
        b: u32,
    },
    /// A gear is not on a turning axle (it is locked into the frame or its axle is not parallel).
    #[error("Gears must turn on axles held by the same part")]
    GearNotOnAxle {
        /// Uid of the gear.
        gear: u32,
    },
    /// A part has no collision shape to carry its mass.
    #[error("The part {0} has no collision shape.")]
    NoCollision(String),
    /// A ball joint closes a loop, which is not supported.
    #[error("A loop through a ball joint is not supported.")]
    LoopThroughBall,
    /// The physics parameter file is invalid.
    #[error("Bad physics parameters: {0}")]
    Params(String),
}
