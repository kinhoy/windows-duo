use crate::config::DepthMode;

pub struct DepthGeometry {
    pub max_separation_degrees: f64,
}

impl Default for DepthGeometry {
    fn default() -> Self {
        Self { max_separation_degrees: 88.0 }
    }
}

impl DepthGeometry {
    /// 按 `mode` 选择几何算法。四个角点顺序
    /// [左下, 右下, 右上, 左上]，对应单位正方形 (0,0),(1,0),(1,1),(0,1)。
    pub fn corners(
        &self,
        mode: DepthMode,
        start_angle: f64,
        current_angle: f64,
        viewing_distance_ratio: f64,
        recession: f64,
        screen_width: f64,
        screen_height: f64,
    ) -> [[f64; 2]; 4] {
        match mode {
            DepthMode::Stretch => self.corners_stretch(
                start_angle,
                current_angle,
                viewing_distance_ratio,
                recession,
                screen_width,
                screen_height,
            ),
            DepthMode::Perspective => self.corners_perspective(
                start_angle,
                current_angle,
                viewing_distance_ratio,
                recession,
                screen_width,
                screen_height,
            ),
        }
    }

    /// 旧版：底边以中线为中心做水平收缩，形成梯形。
    /// 逐行水平压缩（越靠下压得越扁），形变夸张，但不是真透视。
    #[allow(clippy::too_many_arguments)]
    fn corners_stretch(
        &self,
        start_angle: f64,
        current_angle: f64,
        viewing_distance_ratio: f64,
        recession: f64,
        screen_width: f64,
        screen_height: f64,
    ) -> [[f64; 2]; 4] {
        let travel = (start_angle - current_angle).max(0.0);
        let sep_deg = (recession * travel).min(self.max_separation_degrees);
        let sep = sep_deg.to_radians();
        let k = sep.sin();

        let persp = (1.0 / viewing_distance_ratio.max(0.5)).clamp(0.0, 1.5);
        let shrink = k * persp * 3.0;
        let narrow_scale = (1.0 - shrink).max(0.15);

        let cx = screen_width / 2.0;

        [
            [0.0, 0.0],
            [screen_width, 0.0],
            [cx + cx * narrow_scale, screen_height],
            [cx - cx * narrow_scale, screen_height],
        ]
    }

    /// 新版：把画面当作绕屏幕底边（铰链）向外旋转 θ 角的平面，
    /// 从相机位置 (cx, H/2, D) 做真透视投影回 z=0 平面。
    ///
    /// 对于原始屏幕点 (x, y)：
    ///   P = (x, y·cosθ, -y·sinθ)
    ///   t = D / (D + y·sinθ)
    ///   px = cx + t·(x - cx)
    ///   py = H/2 + t·(y·cosθ - H/2)
    ///
    /// 底边 (y=0) 不动，顶边 (y=H) 等比收窄并下沉，
    /// 每一行都是等比缩放，没有水平拉伸。
    #[allow(clippy::too_many_arguments)]
    fn corners_perspective(
        &self,
        start_angle: f64,
        current_angle: f64,
        viewing_distance_ratio: f64,
        recession: f64,
        screen_width: f64,
        screen_height: f64,
    ) -> [[f64; 2]; 4] {
        let travel = (start_angle - current_angle).max(0.0);
        let sep_deg = (recession * travel).min(self.max_separation_degrees);
        let theta = sep_deg.to_radians();

        let sin_t = theta.sin();
        let cos_t = theta.cos();

        let w = screen_width;
        let h = screen_height;
        let cx = w * 0.5;

        // 观察距离，以屏幕高度为单位换算成像素。下限保护。
        let d = (viewing_distance_ratio * h).max(h * 0.1);

        // 顶边 (y=H) 的透视系数 t = D / (D + H·sinθ)。
        let denom = (d + h * sin_t).max(d * 0.5);
        let t = d / denom;

        let top_y = h * 0.5 + t * (h * cos_t - h * 0.5);
        let top_half = cx * t;

        [
            [0.0, 0.0],
            [w, 0.0],
            [cx + top_half, top_y],
            [cx - top_half, top_y],
        ]
    }
}

/// Heckbert 单位正方形 → 四边形。列主序。
pub fn homography(width: f64, height: f64, c: &[[f64; 2]; 4]) -> [[f64; 3]; 3] {
    let (x0, y0) = (c[0][0], c[0][1]);
    let (x1, y1) = (c[1][0], c[1][1]);
    let (x2, y2) = (c[2][0], c[2][1]);
    let (x3, y3) = (c[3][0], c[3][1]);

    let dx1 = x1 - x2;
    let dx2 = x3 - x2;
    let dx3 = x0 - x1 + x2 - x3;
    let dy1 = y1 - y2;
    let dy2 = y3 - y2;
    let dy3 = y0 - y1 + y2 - y3;

    let mut g = 0.0;
    let mut h = 0.0;
    if dx3.abs() > 1e-9 || dy3.abs() > 1e-9 {
        let det = dx1 * dy2 - dx2 * dy1;
        if det.abs() > 1e-12 {
            g = (dx3 * dy2 - dx2 * dy3) / det;
            h = (dx1 * dy3 - dx3 * dy1) / det;
        }
    }
    let a = x1 - x0 + g * x1;
    let b = x3 - x0 + h * x3;
    let cc = x0;
    let d = y1 - y0 + g * y1;
    let e = y3 - y0 + h * y3;
    let f = y0;

    [
        [a / width, d / width, g / width],
        [b / height, e / height, h / height],
        [cc, f, 1.0],
    ]
}

pub fn invert(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let a = m[0][0]; let b = m[1][0]; let cc = m[2][0];
    let d = m[0][1]; let e = m[1][1]; let f = m[2][1];
    let g = m[0][2]; let h = m[1][2]; let i = m[2][2];

    let det = a * (e * i - f * h) - b * (d * i - f * g) + cc * (d * h - e * g);
    let inv = if det.abs() < 1e-12 { 0.0 } else { 1.0 / det };

    let ia = (e * i - f * h) * inv;
    let ib = (cc * h - b * i) * inv;
    let ic = (b * f - cc * e) * inv;
    let id = (f * g - d * i) * inv;
    let ie = (a * i - cc * g) * inv;
    let if_ = (cc * d - a * f) * inv;
    let ig = (d * h - e * g) * inv;
    let ih = (b * g - a * h) * inv;
    let ii = (a * e - b * d) * inv;

    [
        [ia, id, ig],
        [ib, ie, ih],
        [ic, if_, ii],
    ]
}