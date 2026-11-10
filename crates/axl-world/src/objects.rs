//! The standard objects (`assets/objects/`).

use serde::{Deserialize, Serialize};

/// The shape of an object.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ObjShape {
    /// A cube with this side, metres.
    Cube(f64),
    /// A cylinder (axis up).
    Cylinder {
        /// Diameter, metres.
        diameter: f64,
        /// Height, metres.
        height: f64,
    },
    /// A sphere with this diameter, metres.
    Sphere(f64),
}

/// A library object: a free body in geom group 2.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObjectDef {
    /// File format version.
    pub version: u32,
    /// Id, equal to the file name.
    pub id: String,
    /// Name for the UI.
    pub name: String,
    /// Shape.
    pub shape: ObjShape,
    /// Mass, kg (a typical real value).
    pub mass: f64,
    /// Sliding friction coefficient.
    pub friction: f64,
    /// Hollow shell (`shellinertia`): table tennis and tennis balls.
    #[serde(default)]
    pub hollow: bool,
    /// Where the mass and friction come from.
    pub source: String,
    /// sRGB colour.
    pub color: [u8; 3],
}

impl ObjectDef {
    /// Parse RON text.
    pub fn from_ron(text: &str) -> Result<Self, String> {
        ron::de::from_str(text).map_err(|e| e.to_string())
    }

    /// Height of the object, metres.
    pub fn height(&self) -> f64 {
        match self.shape {
            ObjShape::Cube(s) | ObjShape::Sphere(s) => s,
            ObjShape::Cylinder { height, .. } => height,
        }
    }

    /// The MJCF `<geom>` attributes of the shape (type and size).
    pub fn geom_shape(&self) -> String {
        match self.shape {
            ObjShape::Cube(s) => format!("type=\"box\" size=\"{0:.6} {0:.6} {0:.6}\"", s / 2.0),
            ObjShape::Cylinder { diameter, height } => format!(
                "type=\"cylinder\" size=\"{:.6} {:.6}\"",
                diameter / 2.0,
                height / 2.0
            ),
            ObjShape::Sphere(d) => format!("type=\"sphere\" size=\"{:.6}\"", d / 2.0),
        }
    }
}
