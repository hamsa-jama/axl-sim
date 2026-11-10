//! The world file format.

use serde::{Deserialize, Serialize};

/// Current `version` of the world files.
pub const WORLD_VERSION: u32 = 1;

/// A pose on the floor: x, y metres, yaw radians (counter-clockwise from +X).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pose2 {
    /// Metres.
    pub x: f64,
    /// Metres.
    pub y: f64,
    /// Radians.
    pub yaw: f64,
}

/// One shape printed on the mat. Later shapes are drawn over earlier ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MatShape {
    /// A line through `points` (metres).
    Line {
        /// Corner points.
        points: Vec<[f64; 2]>,
        /// Width, millimetres.
        width_mm: f64,
        /// sRGB colour.
        color: [u8; 3],
    },
    /// A filled rectangle.
    Rect {
        /// Lower corner.
        min: [f64; 2],
        /// Upper corner.
        max: [f64; 2],
        /// sRGB colour.
        color: [u8; 3],
    },
    /// A filled circle.
    Circle {
        /// Centre.
        center: [f64; 2],
        /// Radius, metres.
        radius: f64,
        /// sRGB colour.
        color: [u8; 3],
    },
}

/// Printed graphics on the floor. The floor plane itself is infinite.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MatDef {
    /// Width (x) and depth (y), metres.
    pub size_m: [f64; 2],
    /// Where the middle of the mat is (an addition to the spec's struct: the spec's mats do not all
    /// start at the origin).
    #[serde(default)]
    pub center: [f64; 2],
    /// sRGB background colour.
    pub background: [u8; 3],
    /// Printed shapes.
    #[serde(default)]
    pub shapes: Vec<MatShape>,
}

/// A wall: a box standing on the floor along a line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallDef {
    /// Start, metres.
    pub from: [f64; 2],
    /// End, metres.
    pub to: [f64; 2],
    /// Height, metres.
    pub height: f64,
    /// Thickness, metres.
    pub thickness: f64,
    /// Material name (`painted_wood`).
    pub material: String,
}

/// A primitive of a static shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Primitive {
    /// Box with these half sizes.
    Box([f64; 3]),
    /// Cylinder (axis along z): radius, half height.
    Cylinder(f64, f64),
}

/// A fixed shape: a ramp, a step, a post.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StaticShape {
    /// Name (unique in the world).
    pub name: String,
    /// Shape.
    pub primitive: Primitive,
    /// Centre, metres.
    pub pos: [f64; 3],
    /// Rotation about x, y, z, degrees (applied in that order).
    #[serde(default)]
    pub euler_deg: [f64; 3],
    /// Material name (`painted_wood`, `rubber`, `plastic`).
    pub material: String,
}

/// A loose object from the library standing on the floor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectPlacement {
    /// Object id (`assets/objects/`).
    pub object: String,
    /// Position of the object's centre on the floor, metres (it is placed resting on the floor).
    pub xy: [f64; 2],
    /// Yaw, radians.
    #[serde(default)]
    pub yaw: f64,
}

/// A named area used for grading; not physical.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    /// Name.
    pub name: String,
    /// Shape.
    pub shape: ZoneShape,
}

/// The shape of a zone.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ZoneShape {
    /// Axis-aligned rectangle.
    Rect {
        /// Lower corner.
        min: [f64; 2],
        /// Upper corner.
        max: [f64; 2],
    },
    /// Circle.
    Circle {
        /// Centre.
        center: [f64; 2],
        /// Radius.
        radius: f64,
    },
}

impl Zone {
    /// Whether the point is inside.
    pub fn contains(&self, p: [f64; 2]) -> bool {
        match &self.shape {
            ZoneShape::Rect { min, max } => {
                p[0] >= min[0] && p[0] <= max[0] && p[1] >= min[1] && p[1] <= max[1]
            }
            ZoneShape::Circle { center, radius } => {
                (p[0] - center[0]).hypot(p[1] - center[1]) <= *radius
            }
        }
    }
}

/// A variant of a world: extra content, and optionally another spawn.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VariantDef {
    /// Variant id (`a`, `b`, `ramp_15`, ...).
    pub id: String,
    /// Replaces the spawn.
    #[serde(default)]
    pub spawn: Option<Pose2>,
    /// Replaces the mat's shapes.
    #[serde(default)]
    pub mat_shapes: Option<Vec<MatShape>>,
    /// Replaces the walls.
    #[serde(default)]
    pub walls: Option<Vec<WallDef>>,
    /// Added zones.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// Replaces the guide line.
    #[serde(default)]
    pub path: Option<Vec<[f64; 2]>>,
}

/// A maze: cells and a layout per seed (see [`crate::maze`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MazeDef {
    /// Cells per side.
    pub cells: usize,
    /// Cell size, metres.
    pub cell_m: f64,
    /// Wall height, metres.
    pub height: f64,
    /// Wall thickness, metres.
    pub thickness: f64,
    /// Generator seed of each fixed layout; the run seed picks one.
    pub layout_seeds: Vec<u64>,
}

/// A world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldDef {
    /// File format version.
    pub version: u32,
    /// Id (`test_track`, ...), equal to the file name.
    pub id: String,
    /// Name for the UI.
    pub name: String,
    /// Printed floor graphics.
    #[serde(default)]
    pub mat: Option<MatDef>,
    /// Walls.
    #[serde(default)]
    pub walls: Vec<WallDef>,
    /// Static shapes.
    #[serde(default)]
    pub statics: Vec<StaticShape>,
    /// Loose objects.
    #[serde(default)]
    pub objects: Vec<ObjectPlacement>,
    /// Where the robot starts.
    pub spawn: Pose2,
    /// Grading zones.
    #[serde(default)]
    pub zones: Vec<Zone>,
    /// The guide line of a line-following world: a closed loop of points, metres. Missions grade
    /// `FollowLine` against it; it is not drawn.
    #[serde(default)]
    pub path: Vec<[f64; 2]>,
    /// Variants.
    #[serde(default)]
    pub variants: Vec<VariantDef>,
    /// Maze generator settings (the `maze` world).
    #[serde(default)]
    pub maze: Option<MazeDef>,
}

impl WorldDef {
    /// Parse RON text.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        let w: Self = ron::de::from_str(text).map_err(|e| e.to_string())?;
        if w.version > WORLD_VERSION {
            return Err("This file was made by a newer version of AXL.".to_owned());
        }
        Ok(w)
    }

    /// The zone called `name`.
    pub fn zone(&self, name: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.name == name)
    }
}

/// Linear value of an sRGB channel, 0 to 1.
pub fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
