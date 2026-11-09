// Grid floor: StandardMaterial's PBR fragment shader with printed-ink grid lines.
//
// The lines are drawn analytically from the mesh UVs, which are the floor position
// in metres in the AXL frame (x, y). They darken the floor's base colour BEFORE
// lighting, so shadows, ambient occlusion and the environment light fall on them
// exactly as on printed ink. This is WESL (a superset of WGSL) because it imports
// Bevy's PBR modules; see docs/decisions/0005-wesl-and-procedural-environment.md.
//
// Every formula here is mirrored by a tested function in grid_math.rs.

import bevy_pbr::render::{
    pbr_types,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_fragment::pbr_input_from_standard_material,
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::view,
};

struct GridParams {
    // minor spacing, major spacing, bold spacing, line width in pixels (all but the last in metres)
    spacing: vec4<f32>,
    // ink strength of minor, major, bold lines (0..1), and the grid opacity (0..1)
    ink: vec4<f32>,
    // fade-near distance, fade-far distance, minimum spacing in pixels, axis width in pixels
    fade: vec4<f32>,
    // stud-dot cursor: x, y in metres (AXL frame), radius in metres, enabled (1 or 0)
    cursor: vec4<f32>,
    // measure tool: first point x, y, second point x, y (metres); drawn as ring markers
    measure: vec4<f32>,
    // measure tool: number of points set (0, 1 or 2), unused x3
    measure_count: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> grid: GridParams;

// mirrors grid_math::line_coverage
fn line_coverage(dist_px: f32, width_px: f32) -> f32 {
    return clamp(0.5 + 0.5 * width_px - dist_px, 0.0, 1.0);
}

// mirrors grid_math::spacing_fade: lines closer than `min_px` pixels vanish, full at twice that.
fn spacing_fade(spacing_px: f32, min_px: f32) -> f32 {
    return smoothstep(min_px, 2.0 * min_px, spacing_px);
}

// mirrors grid_math::distance_fade
fn distance_fade(distance_m: f32, near: f32, far: f32) -> f32 {
    return 1.0 - smoothstep(near, far, distance_m);
}

// Coverage (0..1) of one family of lines spaced `spacing` metres apart, anti-aliased with fwidth,
// multiplied by the fade for lines that are too close together on screen.
fn family(uv: vec2<f32>, spacing: f32, width_px: f32, min_px: f32) -> f32 {
    let coord = uv / spacing;
    let fw = max(fwidth(coord), vec2<f32>(1e-6));
    // distance to the nearest line, in pixels, along each axis
    let d = abs(fract(coord - 0.5) - 0.5) / fw;
    let cov = line_coverage(min(d.x, d.y), width_px);
    let px_between_lines = 1.0 / max(fw.x, fw.y);
    return cov * spacing_fade(px_between_lines, min_px);
}

// Ring marker used by the measure tool: a filled dot with a darker outline, `r_px` pixels wide.
fn marker(uv: vec2<f32>, centre: vec2<f32>, r_px: f32) -> f32 {
    let px = max(fwidth(uv.x), 1e-6);
    let d = length(uv - centre) / px;
    return clamp(r_px + 0.5 - d, 0.0, 1.0);
}

// The measured segment between the two markers, 2 pixels wide.
fn segment(uv: vec2<f32>, a: vec2<f32>, b: vec2<f32>, width_px: f32) -> f32 {
    let px = max(fwidth(uv.x), 1e-6);
    let ab = b - a;
    let len2 = max(dot(ab, ab), 1e-12);
    let t = clamp(dot(uv - a, ab) / len2, 0.0, 1.0);
    let d = length(uv - (a + ab * t)) / px;
    return line_coverage(d, width_px);
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(
        pbr_input.material.flags,
        pbr_input.material.alpha_cutoff,
        pbr_input.material.base_color
    );

    let uv = in.uv;
    let line_px = grid.spacing.w;
    let min_px = grid.fade.z;
    let opacity = grid.ink.w;
    let dfade = distance_fade(distance(view.world_position, in.world_position), grid.fade.x, grid.fade.y);

    // Grid lines: the strongest family wins at a crossing so ink never double-darkens.
    let minor = family(uv, grid.spacing.x, line_px, min_px) * grid.ink.x;
    let major = family(uv, grid.spacing.y, line_px, min_px) * grid.ink.y;
    let bold = family(uv, grid.spacing.z, line_px * 1.5, min_px) * grid.ink.z;
    let ink = max(max(minor, major), bold) * dfade * opacity;

    var rgb = pbr_input.material.base_color.rgb * (1.0 - ink);

    // World axes at the origin: X red (the line y = 0), Y green (the line x = 0).
    let axis_px = grid.fade.w;
    let fw_uv = max(fwidth(uv), vec2<f32>(1e-6));
    let on_x_axis = line_coverage(abs(uv.y) / fw_uv.y, axis_px);
    let on_y_axis = line_coverage(abs(uv.x) / fw_uv.x, axis_px);
    let axis_strength = clamp(opacity * 1.5, 0.0, 1.0) * dfade;
    // Linear-light versions of the axis colours (X red, Y green).
    let red = vec3<f32>(0.80, 0.05, 0.03);
    let green = vec3<f32>(0.05, 0.62, 0.10);
    rgb = mix(rgb, red, on_x_axis * axis_strength);
    rgb = mix(rgb, green, on_y_axis * axis_strength);

    // Stud-position dots around the cursor (builder only; enabled flag in cursor.w).
    if grid.cursor.w > 0.5 {
        let s = grid.spacing.x;
        let nearest = round(uv / s) * s;
        let px = max(fwidth(uv.x), 1e-6);
        let dot_cov = clamp(2.0 + 0.5 - length(uv - nearest) / px, 0.0, 1.0);
        let reach = grid.cursor.z;
        let falloff = 1.0 - smoothstep(0.5 * reach, reach, distance(nearest, grid.cursor.xy));
        rgb = rgb * (1.0 - 0.75 * dot_cov * falloff * spacing_fade(s / px, min_px));
    }

    // Measure tool markers and the segment between them (orange, the UI accent).
    if grid.measure_count.x > 0.5 {
        let accent = vec3<f32>(0.99, 0.25, 0.01);
        var m = marker(uv, grid.measure.xy, 4.0);
        if grid.measure_count.x > 1.5 {
            m = max(m, marker(uv, grid.measure.zw, 4.0));
            m = max(m, segment(uv, grid.measure.xy, grid.measure.zw, 2.0));
        }
        rgb = mix(rgb, accent, m);
    }

    pbr_input.material.base_color = vec4<f32>(rgb, pbr_input.material.base_color.a);

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
