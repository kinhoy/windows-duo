struct Uniforms {
    column0: vec4<f32>,
    column1: vec4<f32>,
    column2: vec4<f32>,
    screen_and_origin: vec4<f32>,
    padded_and_blur: vec4<f32>,
    shape: vec4<f32>,
    light: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var picture_tex: texture_2d<f32>;
@group(0) @binding(2) var picture_smp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> VsOut {
    var pts = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 3.0,  1.0),
    );
    var out: VsOut;
    out.pos = vec4<f32>(pts[vid], 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let screen_size   = u.screen_and_origin.xy;
    let padded_origin = u.screen_and_origin.zw;
    let padded_size   = u.padded_and_blur.xy;
    let max_radius    = u.padded_and_blur.z;
    let strength      = u.padded_and_blur.w;
    let blur_floor    = u.shape.x;
    let max_dim       = u.shape.y;
    let pixel_scale   = u.shape.z;
    let max_level     = u.shape.w;
    let dim_floor     = u.light.x;
    let dim_strength  = u.light.y;
    let dim_reach     = u.light.z;

    // 片元坐标是像素、y 向下；几何是点、y 向上。
    let screen_point = vec2<f32>(
        in.pos.x / pixel_scale,
        screen_size.y - in.pos.y / pixel_scale
    );

    // 从三个 uniform 列构造逆矩阵（列主序，和 Metal/WGSL 一致）。
    let m = mat3x3<f32>(u.column0.xyz, u.column1.xyz, u.column2.xyz);
    let mapped = m * vec3<f32>(screen_point, 1.0);
    if (abs(mapped.z) < 1e-6) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let picture_point = mapped.xy / mapped.z;

    let unit = (picture_point - padded_origin) / padded_size;
    if (unit.x < 0.0 || unit.x > 1.0 || unit.y < 0.0 || unit.y > 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let tex_coord = vec2<f32>(unit.x, 1.0 - unit.y);

    // 高度：0 在铰链侧，1 在远端。
    let h = clamp(picture_point.y / screen_size.y, 0.0, 1.0);

    let blur = strength * (blur_floor + (1.0 - blur_floor) * h);
    let mip_level = clamp(log2(max(blur * max_radius, 1.0)), 0.0, max_level);

    // 从 mip 链采样，近似原 Gaussian 金字塔。
    var col = textureSampleLevel(picture_tex, picture_smp, tex_coord, mip_level);

    let spread = smoothstep(0.0, max(dim_reach, 0.02), h);
    let fade = dim_strength * (dim_floor + (1.0 - dim_floor) * spread);
    // 采样是线性光，指数 2.2 让暗化比例和原实现一致。
    col = vec4<f32>(col.rgb * pow(1.0 - max_dim * fade, 2.2), 1.0);
    return col;
}