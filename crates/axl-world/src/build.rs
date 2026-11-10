//! From a [`WorldDef`] to MJCF, zones and a rasterised mat.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::def::{MatDef, MatShape, Pose2, Primitive, WallDef, WorldDef, Zone, srgb_to_linear};
use crate::maze;
use crate::objects::ObjectDef;

/// Pixel size of the rasterised mat, metres (2 mm: a 19 mm line is about 9 pixels wide).
pub const MAT_PIXEL_M: f64 = 0.002;
/// Height above the floor at which loose objects are placed, metres (the robot spawns 0.5 mm up).
pub const OBJECT_LIFT_M: f64 = 0.0005;

/// Friction coefficients the materials map to (taken from the physics table by the caller).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldFriction {
    /// Painted wood walls.
    pub wall: f64,
    /// Plastic surfaces.
    pub plastic: f64,
    /// Rubber surfaces.
    pub rubber: f64,
}

/// The mat as pixels the colour sensor reads (linear RGB, row 0 at `min[1]`).
#[derive(Clone, Debug, PartialEq)]
pub struct MatRaster {
    /// Lower corner, metres.
    pub min: [f64; 2],
    /// Upper corner, metres.
    pub max: [f64; 2],
    /// Pixels along x.
    pub width: usize,
    /// Pixels along y.
    pub height: usize,
    /// Row-major linear RGB.
    pub pixels: Vec<[f32; 3]>,
}

/// The shape of a fixed visual (a wall, a ramp, a post).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShapeKind {
    /// Box with these half sizes, metres.
    Box([f64; 3]),
    /// Cylinder along z: radius, half height, metres.
    Cylinder(f64, f64),
}

/// How a fixed shape is drawn: where it is, its colour and what it is made of. The same numbers
/// that went into the MJCF, so the picture and the physics cannot disagree.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeVisual {
    /// Geom name in the model.
    pub name: String,
    /// Shape and size.
    pub kind: ShapeKind,
    /// Centre, metres.
    pub pos: [f64; 3],
    /// Orientation as `[w, x, y, z]`.
    pub quat: [f64; 4],
    /// sRGB colour.
    pub color: [u8; 3],
    /// Material name (`painted_wood`, `plastic`, `rubber`).
    pub material: String,
}

/// How a loose object is drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectVisual {
    /// Name of its body in the model.
    pub body: String,
    /// Shape and size.
    pub shape: crate::objects::ObjShape,
    /// sRGB colour.
    pub color: [u8; 3],
    /// Where it starts: x, y, yaw.
    pub start: Pose2,
}

/// Quaternion `[w, x, y, z]` of MuJoCo's default `xyz` Euler sequence (intrinsic rotations about
/// x, then y, then z), angles in radians.
pub fn euler_xyz_quat(e: [f64; 3]) -> [f64; 4] {
    let q = |half: f64, axis: usize| {
        let mut v = [half.cos(), 0.0, 0.0, 0.0];
        v[axis + 1] = half.sin();
        v
    };
    let mul = |a: [f64; 4], b: [f64; 4]| {
        [
            a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
            a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
            a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
            a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
        ]
    };
    mul(mul(q(e[0] / 2.0, 0), q(e[1] / 2.0, 1)), q(e[2] / 2.0, 2))
}

/// A world ready to go into a model.
#[derive(Clone, Debug, PartialEq)]
pub struct BuiltWorld {
    /// Walls and static shapes, as they are drawn.
    pub shapes: Vec<ShapeVisual>,
    /// Loose objects, as they are drawn.
    pub objects: Vec<ObjectVisual>,
    /// MJCF to place inside `<worldbody>` (walls, statics, loose objects).
    pub worldbody: String,
    /// Where the robot starts.
    pub spawn: Pose2,
    /// Grading zones (world and variant).
    pub zones: Vec<Zone>,
    /// The mat, rasterised.
    pub mat: Option<MatRaster>,
    /// Names of the MJCF bodies of the loose objects, in placement order.
    pub object_bodies: Vec<String>,
    /// Wall geom names, so missions can tag contacts.
    pub wall_geoms: Vec<String>,
    /// Names of the fixed obstacles (static shapes whose name starts with `obstacle`), which also
    /// count as walls for contact tagging.
    pub obstacle_geoms: Vec<String>,
    /// The guide line (closed loop) of a line-following world, for `FollowLine`.
    pub guide_path: Option<Vec<[f64; 2]>>,
}

fn rgba(c: [u8; 3]) -> String {
    format!(
        "{:.3} {:.3} {:.3} 1",
        srgb_to_linear(c[0]),
        srgb_to_linear(c[1]),
        srgb_to_linear(c[2])
    )
}

fn material(name: &str, fr: &WorldFriction) -> Result<(f64, [u8; 3]), String> {
    match name {
        "painted_wood" => Ok((fr.wall, [0xD6, 0xB4, 0x84])),
        "plastic" => Ok((fr.plastic, [0xC8, 0xC8, 0xC8])),
        "rubber" => Ok((fr.rubber, [0x2A, 0x2A, 0x2A])),
        other => Err(format!("unknown material {other:?}")),
    }
}

fn wall_geom(
    i: usize,
    w: &WallDef,
    fr: &WorldFriction,
    out: &mut String,
    visuals: &mut Vec<ShapeVisual>,
) -> Result<String, String> {
    let (mu, col) = material(&w.material, fr)?;
    let (dx, dy) = (w.to[0] - w.from[0], w.to[1] - w.from[1]);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return Err(format!("wall {i} has no length"));
    }
    let name = format!("wall_{i}");
    let yaw = dy.atan2(dx); // the compiler's MJCF uses radians
    let _ = writeln!(
        out,
        "    <geom name=\"{name}\" type=\"box\" pos=\"{:.6} {:.6} {:.6}\" size=\"{:.6} {:.6} {:.6}\" euler=\"0 0 {yaw:.6}\" contype=\"2\" conaffinity=\"1\" group=\"0\" friction=\"{mu} 0.005 0.0001\" rgba=\"{}\"/>",
        (w.from[0] + w.to[0]) / 2.0,
        (w.from[1] + w.to[1]) / 2.0,
        w.height / 2.0,
        (len + w.thickness) / 2.0,
        w.thickness / 2.0,
        w.height / 2.0,
        rgba(col)
    );
    visuals.push(ShapeVisual {
        name: name.clone(),
        kind: ShapeKind::Box([(len + w.thickness) / 2.0, w.thickness / 2.0, w.height / 2.0]),
        pos: [
            (w.from[0] + w.to[0]) / 2.0,
            (w.from[1] + w.to[1]) / 2.0,
            w.height / 2.0,
        ],
        quat: euler_xyz_quat([0.0, 0.0, yaw]),
        color: col,
        material: w.material.clone(),
    });
    Ok(name)
}

/// Paint `shapes` over `pixels`.
fn paint(
    m: &MatDef,
    shapes: &[MatShape],
    width: usize,
    height: usize,
    pixel_m: f64,
    px: &mut [[f32; 3]],
) {
    let min = [
        m.center[0] - m.size_m[0] / 2.0,
        m.center[1] - m.size_m[1] / 2.0,
    ];
    let to_ix = |v: f64, lo: f64, n: usize| -> i64 {
        (((v - lo) / pixel_m).floor() as i64).clamp(-1, n as i64)
    };
    let mut stamp = |lo: [f64; 2], hi: [f64; 2], inside: &dyn Fn(f64, f64) -> bool, c: [f32; 3]| {
        let (i0, i1) = (
            to_ix(lo[0], min[0], width).max(0),
            to_ix(hi[0], min[0], width).min(width as i64 - 1),
        );
        let (j0, j1) = (
            to_ix(lo[1], min[1], height).max(0),
            to_ix(hi[1], min[1], height).min(height as i64 - 1),
        );
        for j in j0..=j1 {
            for i in i0..=i1 {
                let x = min[0] + (i as f64 + 0.5) * pixel_m;
                let y = min[1] + (j as f64 + 0.5) * pixel_m;
                if inside(x, y) {
                    px[j as usize * width + i as usize] = c;
                }
            }
        }
    };
    let lin = |c: [u8; 3]| {
        [
            srgb_to_linear(c[0]),
            srgb_to_linear(c[1]),
            srgb_to_linear(c[2]),
        ]
    };
    for s in shapes {
        match s {
            MatShape::Rect {
                min: a,
                max: b,
                color,
            } => {
                stamp(
                    *a,
                    *b,
                    &|x, y| x >= a[0] && x <= b[0] && y >= a[1] && y <= b[1],
                    lin(*color),
                );
            }
            MatShape::Circle {
                center,
                radius,
                color,
            } => {
                let (c, r) = (*center, *radius);
                stamp(
                    [c[0] - r, c[1] - r],
                    [c[0] + r, c[1] + r],
                    &|x, y| (x - c[0]).hypot(y - c[1]) <= r,
                    lin(*color),
                );
            }
            MatShape::Line {
                points,
                width_mm,
                color,
            } => {
                let r = width_mm / 2000.0;
                for seg in points.windows(2) {
                    let (a, b) = (seg[0], seg[1]);
                    let lo = [a[0].min(b[0]) - r, a[1].min(b[1]) - r];
                    let hi = [a[0].max(b[0]) + r, a[1].max(b[1]) + r];
                    stamp(
                        lo,
                        hi,
                        &|x, y| dist_to_segment([x, y], a, b) <= r,
                        lin(*color),
                    );
                }
            }
        }
    }
}

fn dist_to_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (abx, aby) = (b[0] - a[0], b[1] - a[1]);
    let len2 = abx * abx + aby * aby;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p[0] - a[0]) * abx + (p[1] - a[1]) * aby) / len2).clamp(0.0, 1.0)
    };
    (p[0] - (a[0] + t * abx)).hypot(p[1] - (a[1] + t * aby))
}

fn raster(m: &MatDef, shapes: &[MatShape]) -> MatRaster {
    raster_at(m, shapes, MAT_PIXEL_M)
}

fn raster_at(m: &MatDef, shapes: &[MatShape], pixel_m: f64) -> MatRaster {
    let width = (m.size_m[0] / pixel_m).round() as usize;
    let height = (m.size_m[1] / pixel_m).round() as usize;
    let bg = [
        srgb_to_linear(m.background[0]),
        srgb_to_linear(m.background[1]),
        srgb_to_linear(m.background[2]),
    ];
    let mut pixels = vec![bg; width * height];
    paint(m, shapes, width, height, pixel_m, &mut pixels);
    let min = [
        m.center[0] - m.size_m[0] / 2.0,
        m.center[1] - m.size_m[1] / 2.0,
    ];
    MatRaster {
        min,
        max: [
            min[0] + width as f64 * pixel_m,
            min[1] + height as f64 * pixel_m,
        ],
        width,
        height,
        pixels,
    }
}

/// Pixel size of the mat picture, metres (1 mm: a 19 mm line is 19 pixels wide). The colour
/// sensor reads the coarser [`MatRaster`] of [`build_world`]; this one is only drawn.
pub const MAT_DISPLAY_PIXEL_M: f64 = 0.001;

/// The mat of `def` and `variant` at [`MAT_DISPLAY_PIXEL_M`], for drawing.
pub fn display_mat(def: &WorldDef, variant: Option<&str>) -> Option<MatRaster> {
    let m = def.mat.as_ref()?;
    let var = variant.and_then(|id| def.variants.iter().find(|v| v.id == id));
    let shapes = var
        .and_then(|v| v.mat_shapes.as_deref())
        .unwrap_or(&m.shapes);
    Some(raster_at(m, shapes, MAT_DISPLAY_PIXEL_M))
}

/// Build `def` (with `variant`, and `seed` for worlds whose layout depends on it).
pub fn build_world(
    def: &WorldDef,
    variant: Option<&str>,
    seed: u64,
    objects: &BTreeMap<String, ObjectDef>,
    fr: &WorldFriction,
) -> Result<BuiltWorld, String> {
    let var = match variant {
        Some(id) => Some(
            def.variants
                .iter()
                .find(|v| v.id == id)
                .ok_or_else(|| format!("world {} has no variant {id:?}", def.id))?,
        ),
        None => None,
    };
    let mut spawn = var.and_then(|v| v.spawn).unwrap_or(def.spawn);
    let mut walls: Vec<WallDef> = var
        .and_then(|v| v.walls.clone())
        .unwrap_or_else(|| def.walls.clone());
    let mut zones = def.zones.clone();
    if let Some(v) = var {
        zones.extend(v.zones.iter().cloned());
    }
    if let Some(mz) = &def.maze {
        let idx = (seed.wrapping_sub(1) % mz.layout_seeds.len() as u64) as usize;
        let grid = maze::generate(mz.cells, mz.layout_seeds[idx]);
        walls.extend(
            grid.segments(mz.cell_m)
                .into_iter()
                .map(|(from, to)| WallDef {
                    from,
                    to,
                    height: mz.height,
                    thickness: mz.thickness,
                    material: "painted_wood".to_owned(),
                }),
        );
        let c = mz.cell_m;
        spawn = Pose2 {
            x: c / 2.0,
            y: c / 2.0,
            yaw: if grid.open(0, 0, 1, 0) {
                0.0
            } else {
                std::f64::consts::FRAC_PI_2
            },
        };
        let n = mz.cells as f64;
        zones.push(Zone {
            name: "goal".to_owned(),
            shape: crate::def::ZoneShape::Rect {
                min: [(n - 1.0) * c, (n - 1.0) * c],
                max: [n * c, n * c],
            },
        });
    }

    let mut worldbody = String::new();
    let mut wall_geoms = Vec::new();
    let mut shapes = Vec::new();
    for (i, w) in walls.iter().enumerate() {
        wall_geoms.push(wall_geom(i, w, fr, &mut worldbody, &mut shapes)?);
    }
    for s in &def.statics {
        let (mu, col) = material(&s.material, fr)?;
        let shape = match &s.primitive {
            Primitive::Box(h) => {
                format!("type=\"box\" size=\"{:.6} {:.6} {:.6}\"", h[0], h[1], h[2])
            }
            Primitive::Cylinder(r, hh) => format!("type=\"cylinder\" size=\"{r:.6} {hh:.6}\""),
        };
        let _ = writeln!(
            worldbody,
            "    <geom name=\"{}\" {shape} pos=\"{:.6} {:.6} {:.6}\" euler=\"{:.6} {:.6} {:.6}\" contype=\"2\" conaffinity=\"1\" group=\"0\" friction=\"{mu} 0.005 0.0001\" rgba=\"{}\"/>",
            s.name,
            s.pos[0],
            s.pos[1],
            s.pos[2],
            s.euler_deg[0].to_radians(),
            s.euler_deg[1].to_radians(),
            s.euler_deg[2].to_radians(),
            rgba(col)
        );
        shapes.push(ShapeVisual {
            name: s.name.clone(),
            kind: match &s.primitive {
                Primitive::Box(h) => ShapeKind::Box(*h),
                Primitive::Cylinder(r, hh) => ShapeKind::Cylinder(*r, *hh),
            },
            pos: s.pos,
            quat: euler_xyz_quat(s.euler_deg.map(f64::to_radians)),
            color: col,
            material: s.material.clone(),
        });
    }
    let mut object_bodies = Vec::new();
    let mut object_visuals = Vec::new();
    for (i, p) in def.objects.iter().enumerate() {
        let o = objects
            .get(&p.object)
            .ok_or_else(|| format!("unknown object {:?}", p.object))?;
        let name = format!("obj_{i}_{}", p.object);
        let shell = if o.hollow {
            " shellinertia=\"true\""
        } else {
            ""
        };
        let _ = writeln!(
            worldbody,
            "    <body name=\"{name}\" pos=\"{:.6} {:.6} {:.6}\" euler=\"0 0 {:.6}\">\n      <freejoint name=\"{name}_free\"/>\n      <geom name=\"{name}_geom\" {} mass=\"{}\" friction=\"{} 0.005 0.0001\" contype=\"3\" conaffinity=\"3\" group=\"2\"{shell} rgba=\"{}\"/>\n    </body>",
            p.xy[0],
            p.xy[1],
            o.height() / 2.0 + OBJECT_LIFT_M,
            p.yaw,
            o.geom_shape(),
            o.mass,
            o.friction,
            rgba(o.color)
        );
        object_visuals.push(ObjectVisual {
            body: name.clone(),
            shape: o.shape,
            color: o.color,
            start: Pose2 {
                x: p.xy[0],
                y: p.xy[1],
                yaw: p.yaw,
            },
        });
        object_bodies.push(name);
    }
    let mat = def.mat.as_ref().map(|m| {
        let shapes = var
            .and_then(|v| v.mat_shapes.as_deref())
            .unwrap_or(&m.shapes);
        raster(m, shapes)
    });
    let guide_path = var
        .and_then(|v| v.path.clone())
        .or_else(|| (!def.path.is_empty()).then(|| def.path.clone()));
    let obstacle_geoms = def
        .statics
        .iter()
        .filter(|s| s.name.starts_with("obstacle"))
        .map(|s| s.name.clone())
        .collect();
    Ok(BuiltWorld {
        shapes,
        objects: object_visuals,
        worldbody,
        spawn,
        zones,
        mat,
        object_bodies,
        wall_geoms,
        obstacle_geoms,
        guide_path,
    })
}
