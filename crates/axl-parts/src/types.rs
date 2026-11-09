//! Part definition types (SPEC.md, "Part catalog"). Parts are stored as RON files under
//! `assets/parts/`; lengths in these files are metres and positions are in 0.1 mm steps.

use serde::{Deserialize, Serialize};

/// Stable snake_case part id, for example `brick_2x4`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PartId(pub String);

/// Colour id such as `red` (see `axl-render`'s colour table).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColorId(pub String);

/// Part family, used by the palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Category {
    /// Brick.
    Brick,
    /// Plate.
    Plate,
    /// Tile (smooth top).
    Tile,
    /// Brick with holes in its long side.
    HoleBrick,
    /// Beam.
    Beam,
    /// Axle.
    Axle,
    /// Pin.
    Pin,
    /// Spur gear.
    Gear,
    /// Wheel with a rubber tyre.
    Wheel,
    /// Ball caster.
    Caster,
    /// Controller hub.
    Hub,
    /// Motor or servo.
    Motor,
    /// Sensor.
    Sensor,
}

/// Surface material of a part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum MaterialKind {
    /// Opaque ABS plastic.
    Abs,
    /// Transparent ABS.
    AbsTransparent,
    /// Rubber.
    Rubber,
    /// Steel.
    Steel,
}

/// A signed axis. Hole and shaft connectors ignore the sign; studs and anti-studs use it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Axis {
    /// +X.
    PosX,
    /// -X.
    NegX,
    /// +Y.
    PosY,
    /// -Y.
    NegY,
    /// +Z.
    PosZ,
    /// -Z.
    NegZ,
}

impl Axis {
    /// The unit vector.
    pub fn unit(self) -> [f64; 3] {
        match self {
            Axis::PosX => [1.0, 0.0, 0.0],
            Axis::NegX => [-1.0, 0.0, 0.0],
            Axis::PosY => [0.0, 1.0, 0.0],
            Axis::NegY => [0.0, -1.0, 0.0],
            Axis::PosZ => [0.0, 0.0, 1.0],
            Axis::NegZ => [0.0, 0.0, -1.0],
        }
    }
}

/// How a mass number was obtained.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MassSource {
    /// Solid volume times density (ABS: 1040 kg/m3).
    Computed {
        /// Density, kg/m3.
        density_kg_m3: f64,
    },
    /// Measured on a real part.
    Measured {
        /// Where the number came from.
        note: String,
    },
    /// Placeholder: calibrate (see `docs/calibration.md`).
    Placeholder,
}

/// A collision primitive in the part frame (metres).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CollisionPrim {
    /// Box.
    Box {
        /// Centre.
        center: [f64; 3],
        /// Half sizes.
        half: [f64; 3],
    },
    /// Cylinder.
    Cylinder {
        /// Centre.
        center: [f64; 3],
        /// Axis.
        axis: Axis,
        /// Radius.
        radius: f64,
        /// Half length.
        half_len: f64,
    },
    /// Sphere.
    Sphere {
        /// Centre.
        center: [f64; 3],
        /// Radius.
        radius: f64,
    },
}

/// What a connector is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ConnKind {
    /// Stud (male), axis points out of the part.
    Stud,
    /// Anti-stud (female), axis points out of the part.
    AntiStud,
    /// Round hole (female).
    RoundHole,
    /// Axle hole, cross-shaped (female).
    AxleHole,
    /// Pin shaft (male); one per 8 mm slot.
    PinShaft {
        /// A friction pin adds joint friction.
        friction: bool,
    },
    /// Axle shaft (male); one per 8 mm slot. `len` is the whole axle length in units of 8 mm.
    AxleShaft {
        /// Total axle length in 8 mm units.
        len: u8,
    },
}

/// A connection point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connector {
    /// Kind.
    pub kind: ConnKind,
    /// Position in 0.1 mm steps, part frame.
    pub pos: [i32; 3],
    /// Axis.
    pub axis: Axis,
}

/// A device's hinge between housing and rotor (motors only).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct InternalJoint {
    /// Rotation axis.
    pub axis: Axis,
    /// Anchor, 0.1 mm steps, part frame.
    pub pos: [i32; 3],
    /// Travel limits in radians, or `None` for continuous rotation.
    pub range_rad: Option<[f64; 2]>,
}

/// One rigid piece of a part. Most parts have one; motors have a housing and a rotor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubBody {
    /// Mass, kg. Never zero.
    pub mass_kg: f64,
    /// Where the mass came from.
    pub mass_source: MassSource,
    /// Collision shapes, part frame.
    pub collision: Vec<CollisionPrim>,
    /// Connectors.
    pub connectors: Vec<Connector>,
}

/// Which beam outline to make.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeamShape {
    /// Straight beam with `holes` holes.
    Straight {
        /// Hole count.
        holes: u32,
    },
    /// L-shaped beam: `long` holes along X, `short` holes along Y, sharing the corner hole.
    L {
        /// Holes along the long arm.
        long: u32,
        /// Holes along the short arm.
        short: u32,
    },
    /// Rectangular frame with holes around the edge.
    Frame {
        /// Holes along X.
        w: u32,
        /// Holes along Y.
        h: u32,
    },
}

/// Pin variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinKind {
    /// Pin with friction ridges.
    Friction,
    /// Smooth pin.
    Smooth,
    /// A pin on one end, an axle on the other.
    AxlePin,
}

/// Device housings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DeviceModel {
    /// Controller hub.
    Hub,
    /// Medium motor.
    MotorMedium,
    /// Large motor.
    MotorLarge,
    /// 180 degree servo.
    Servo,
    /// Ultrasonic distance sensor.
    Ultrasonic,
    /// Time-of-flight distance sensor.
    TimeOfFlight,
    /// Colour sensor.
    Color,
    /// Touch sensor.
    Touch,
}

/// The procedural generator and its parameters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShapeGen {
    /// Brick, plate, tile or hole brick.
    Block {
        /// Studs along X.
        nx: u32,
        /// Studs along Y.
        ny: u32,
        /// Height in plates (a brick is 3).
        plates: u32,
        /// Studs on top.
        studs: bool,
        /// Technic-style holes through the long walls.
        holes: bool,
    },
    /// Beam.
    Beam {
        /// Outline.
        shape: BeamShape,
    },
    /// Axle of `units` x 8 mm.
    Axle {
        /// Length in 8 mm units.
        units: u32,
    },
    /// Pin.
    Pin {
        /// Length in 8 mm units.
        units: u32,
        /// Variant.
        kind: PinKind,
    },
    /// Spur gear, module 1 mm.
    Gear {
        /// Tooth count.
        teeth: u32,
        /// Thickness, m.
        thickness: f64,
    },
    /// Wheel with a rubber tyre.
    Wheel {
        /// Outer diameter, m.
        diameter: f64,
        /// Width, m.
        width: f64,
    },
    /// Ball caster.
    Caster {
        /// Ball diameter, m.
        ball_diameter: f64,
    },
    /// Device housing.
    Device {
        /// Which device.
        model: DeviceModel,
    },
}

/// Where a sensor looks from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SiteDef {
    /// Position, 0.1 mm steps, part frame.
    pub pos: [i32; 3],
    /// Viewing axis.
    pub axis: Axis,
}

/// A motor, sensor or hub (parameters live in `assets/physics/hardware.ron`, milestone M6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DeviceDef {
    /// Which device this is.
    pub kind: DeviceModel,
    /// Sensor frame, if it has one.
    pub site: Option<SiteDef>,
}

/// One part of the catalog.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PartDef {
    /// File format version.
    pub version: u32,
    /// Stable id.
    pub id: PartId,
    /// Name shown in the UI.
    pub name: String,
    /// Family.
    pub category: Category,
    /// Generator.
    pub shape: ShapeGen,
    /// Rigid pieces.
    pub bodies: Vec<SubBody>,
    /// Hinge between the two bodies of a motor.
    pub internal_joint: Option<InternalJoint>,
    /// Main surface material.
    pub material: MaterialKind,
    /// Allowed colours; the first is the default.
    pub colors: Vec<ColorId>,
    /// Device data.
    pub device: Option<DeviceDef>,
}

/// Which surface of a part a mesh belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    /// The part's own material and chosen colour.
    Body,
    /// Dark moulded plastic (ports, lenses, shrouds).
    Dark,
    /// Rubber tyre.
    Rubber,
    /// Steel.
    Steel,
}

/// One mesh of a part: a surface on a body.
#[derive(Clone, Debug)]
pub struct MeshSlot {
    /// Index into [`PartDef::bodies`].
    pub body: usize,
    /// Surface.
    pub surface: Surface,
    /// Vertex data, metres.
    pub mesh: crate::trimesh::MeshData,
}

/// All meshes of a part.
#[derive(Clone, Debug, Default)]
pub struct PartMesh {
    /// The slots.
    pub slots: Vec<MeshSlot>,
}

impl PartMesh {
    /// Total triangles.
    pub fn triangles(&self) -> usize {
        self.slots.iter().map(|s| s.mesh.triangles()).sum()
    }
    /// Bounds over all slots (min, max), metres.
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for s in &self.slots {
            for p in &s.mesh.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
        }
        (lo, hi)
    }
}
