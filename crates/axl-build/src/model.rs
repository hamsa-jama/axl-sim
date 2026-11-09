//! The assembly data model (SPEC.md, "Builder"): placed parts only. Connections are derived.

use axl_parts::types::{ColorId, PartId};
use serde::{Deserialize, Serialize};

use crate::orient::Orient;

/// A hub port, A to F.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Port {
    /// Port A.
    A,
    /// Port B.
    B,
    /// Port C.
    C,
    /// Port D.
    D,
    /// Port E.
    E,
    /// Port F.
    F,
}

impl Port {
    /// All six ports in order.
    pub const ALL: [Port; 6] = [Port::A, Port::B, Port::C, Port::D, Port::E, Port::F];

    /// The letter.
    pub fn letter(self) -> char {
        match self {
            Port::A => 'A',
            Port::B => 'B',
            Port::C => 'C',
            Port::D => 'D',
            Port::E => 'E',
            Port::F => 'F',
        }
    }
}

/// The name and port of a motor, sensor or hub.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInstance {
    /// Device name, `[a-z][a-z0-9_]{0,23}`.
    pub name: String,
    /// Hub port; `None` only for the hub.
    pub port: Option<Port>,
}

/// Is `name` a legal device name (`[a-z][a-z0-9_]{0,23}`)?
pub fn valid_device_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && b.len() <= 24
        && b[0].is_ascii_lowercase()
        && b.iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// One placed part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlacedPart {
    /// Unique within the assembly, never reused.
    pub uid: u32,
    /// Catalog part.
    pub part: PartId,
    /// Part-frame origin in the robot frame, 0.1 mm steps.
    pub pos: [i32; 3],
    /// Orientation.
    pub orient: Orient,
    /// Colour.
    pub color: ColorId,
    /// Device data for motors, sensors and the hub.
    pub device: Option<DeviceInstance>,
}

/// The robot as the builder stores it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assembly {
    /// Robot name.
    pub name: String,
    /// Placed parts, in placement order.
    pub parts: Vec<PlacedPart>,
    /// The next uid to hand out. Grows only, so uids are never reused (also across undo).
    pub next_uid: u32,
}

impl Default for Assembly {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

impl Assembly {
    /// An empty assembly.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            parts: Vec::new(),
            next_uid: 1,
        }
    }

    /// A part by uid.
    pub fn get(&self, uid: u32) -> Option<&PlacedPart> {
        self.parts.iter().find(|p| p.uid == uid)
    }

    /// Mutable access by uid.
    pub fn get_mut(&mut self, uid: u32) -> Option<&mut PlacedPart> {
        self.parts.iter_mut().find(|p| p.uid == uid)
    }

    /// Append a part and give it the next uid.
    pub fn push(&mut self, mut p: PlacedPart) -> u32 {
        p.uid = self.next_uid;
        self.next_uid += 1;
        let uid = p.uid;
        self.parts.push(p);
        uid
    }
}

/// A connector on a placed part: part uid, sub-body index, connector index within the sub-body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConnRef {
    /// Part uid.
    pub uid: u32,
    /// Sub-body index.
    pub body: u8,
    /// Connector index in that sub-body.
    pub conn: u16,
}

/// What a connection does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JointKind {
    /// Locked together.
    Rigid,
    /// Turns about the shared axis.
    Hinge {
        /// A friction pin adds joint friction.
        friction: bool,
    },
}

/// A derived connection. `a < b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Connection {
    /// First connector.
    pub a: ConnRef,
    /// Second connector.
    pub b: ConnRef,
    /// Joint type.
    pub kind: JointKind,
}
