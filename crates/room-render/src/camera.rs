//! View / camera parameters for scene building.

use glam::{Mat4, Vec3};

/// Render camera (interpolated player or free-fly).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Eye position (map units, Z up).
    pub eye: Vec3,
    /// Look direction (normalized).
    pub forward: Vec3,
    /// Up vector.
    pub up: Vec3,
    /// Horizontal FOV in degrees.
    pub fov_y_deg: f64,
    /// Aspect ratio width/height.
    pub aspect: f64,
    /// Near plane.
    pub z_near: f32,
    /// Far plane.
    pub z_far: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            eye: Vec3::new(0.0, 0.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.0),
            up: Vec3::Z,
            // ~90° horizontal at 16:9 (90° vertical FOV is fish-eye wide).
            fov_y_deg: 58.715,
            aspect: 16.0 / 9.0,
            z_near: 1.0,
            z_far: 65536.0,
        }
    }
}

impl Camera {
    /// View-projection matrix (Vulkan/Metal clip space via glam).
    #[must_use]
    pub fn view_proj(&self) -> Mat4 {
        let target = self.eye + self.forward;
        let view = Mat4::look_at_rh(self.eye, target, self.up);
        #[allow(clippy::cast_possible_truncation)]
        let fov = self.fov_y_deg as f32;
        #[allow(clippy::cast_possible_truncation)]
        let aspect = self.aspect as f32;
        let proj = Mat4::perspective_rh(fov.to_radians(), aspect, self.z_near, self.z_far);
        proj * view
    }

    /// Camera up after a view roll. Positive degrees are a right-hand turn around `forward`.
    /// A quarter turn looking along +X points up toward -Y.
    #[must_use]
    pub fn rolled_up(forward: Vec3, roll_rad: f32) -> Vec3 {
        let f = if forward.length_squared() < 1e-8 {
            Vec3::X
        } else {
            forward.normalize()
        };
        let mut side = f.cross(Vec3::Z);
        if side.length_squared() < 1e-8 {
            side = f.cross(Vec3::Y);
        }
        let side = side.normalize();
        let up = side.cross(f);
        let (s, c) = roll_rad.sin_cos();
        (up * c + f.cross(up) * s).normalize_or_zero()
    }

    /// XY map position used by the BSP clipper.
    #[must_use]
    pub fn map_xy(&self) -> (f64, f64) {
        (f64::from(self.eye.x), f64::from(self.eye.y))
    }

    /// Viewpoint for an ACS camera texture (`SetCameraToTexture`).
    #[must_use]
    pub fn from_actor_view(
        x: f64,
        y: f64,
        z: f64,
        yaw_rad: f64,
        fov_deg: f64,
        aspect: f64,
    ) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let forward = Vec3::new(yaw_rad.cos() as f32, yaw_rad.sin() as f32, 0.0);
        let forward = if forward.length_squared() < 1e-6 {
            Vec3::X
        } else {
            forward.normalize()
        };
        #[allow(clippy::cast_possible_truncation)]
        let eye = Vec3::new(x as f32, y as f32, z as f32);
        Self {
            eye,
            forward,
            up: Vec3::Z,
            fov_y_deg: fov_deg.clamp(1.0, 175.0),
            aspect: aspect.max(0.1),
            z_near: 1.0,
            z_far: 65536.0,
        }
    }

    /// Translate the eye in map XY (linked-portal displacement bootstrap).
    #[must_use]
    pub fn shifted_by(&self, dx: f64, dy: f64) -> Self {
        #[allow(clippy::cast_possible_truncation)]
        let eye = Vec3::new(
            (f64::from(self.eye.x) + dx) as f32,
            (f64::from(self.eye.y) + dy) as f32,
            self.eye.z,
        );
        Self { eye, ..*self }
    }

    /// Reflect this camera across a horizontal plane at `z` (floor/ceiling reflection).
    #[must_use]
    pub fn reflect_across_z(&self, z: f64) -> Self {
        let ez = f64::from(self.eye.z);
        #[allow(clippy::cast_possible_truncation)]
        let eye = Vec3::new(self.eye.x, self.eye.y, (2.0 * z - ez) as f32);
        let forward = Vec3::new(self.forward.x, self.forward.y, -self.forward.z);
        let forward = if forward.length_squared() < 1e-8 {
            -self.forward
        } else {
            forward.normalize()
        };
        Self {
            eye,
            forward,
            up: self.up,
            fov_y_deg: self.fov_y_deg,
            aspect: self.aspect,
            z_near: self.z_near,
            z_far: self.z_far,
        }
    }

    /// Reflect this camera across a 2D linedef (`Line_Mirror` bootstrap).
    #[must_use]
    pub fn reflect_across_line(&self, ax: f64, ay: f64, bx: f64, by: f64) -> Self {
        let (ex, ey) = self.map_xy();
        let (rx, ry) = reflect_point(ex, ey, ax, ay, bx, by);
        let fx = f64::from(self.forward.x);
        let fy = f64::from(self.forward.y);
        let tip_x = ex + fx;
        let tip_y = ey + fy;
        let (tx, ty) = reflect_point(tip_x, tip_y, ax, ay, bx, by);
        #[allow(clippy::cast_possible_truncation)]
        let mut forward = Vec3::new((tx - rx) as f32, (ty - ry) as f32, self.forward.z);
        if forward.length_squared() < 1e-8 {
            forward = -self.forward;
        } else {
            forward = forward.normalize();
        }
        #[allow(clippy::cast_possible_truncation)]
        let eye = Vec3::new(rx as f32, ry as f32, self.eye.z);
        Self {
            eye,
            forward,
            up: self.up,
            fov_y_deg: self.fov_y_deg,
            aspect: self.aspect,
            z_near: self.z_near,
            z_far: self.z_far,
        }
    }
}

fn reflect_point(px: f64, py: f64, ax: f64, ay: f64, bx: f64, by: f64) -> (f64, f64) {
    let dx = bx - ax;
    let dy = by - ay;
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-12 {
        return (px, py);
    }
    let t = ((px - ax) * dx + (py - ay) * dy) / len2;
    let qx = ax + t * dx;
    let qy = ay + t * dy;
    (2.0 * qx - px, 2.0 * qy - py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflect_across_x_axis_line() {
        let cam = Camera {
            eye: Vec3::new(0.0, 10.0, 41.0),
            forward: Vec3::new(0.0, 1.0, 0.0),
            ..Camera::default()
        };
        // Line along X at y=0.
        let r = cam.reflect_across_line(-100.0, 0.0, 100.0, 0.0);
        assert!((f64::from(r.eye.y) + 10.0).abs() < 1e-3);
        assert!(r.forward.y < 0.0);
    }

    #[test]
    fn from_actor_view_yaw() {
        let c = Camera::from_actor_view(0.0, 0.0, 41.0, 0.0, 90.0, 1.0);
        assert!(c.forward.x > 0.9);
    }

    #[test]
    fn quarter_roll_looking_along_x_points_up_toward_negative_y() {
        let up = Camera::rolled_up(Vec3::X, std::f32::consts::FRAC_PI_2);
        assert!(up.y < -0.9, "up={up:?}");
        assert!(up.z.abs() < 0.1, "up={up:?}");
        let level = Camera::rolled_up(Vec3::X, 0.0);
        assert!(level.z > 0.9, "level={level:?}");
    }

    #[test]
    fn reflect_across_z_flips_height_and_pitch() {
        let cam = Camera {
            eye: Vec3::new(10.0, 20.0, 41.0),
            forward: Vec3::new(1.0, 0.0, 0.5).normalize(),
            ..Camera::default()
        };
        let r = cam.reflect_across_z(0.0);
        assert!((f64::from(r.eye.z) + 41.0).abs() < 1e-3);
        assert!((r.forward.z + cam.forward.z).abs() < 1e-4);
        assert!((r.eye.x - cam.eye.x).abs() < 1e-4);
    }

    #[test]
    fn shifted_by_moves_eye_xy() {
        let c = Camera {
            eye: Vec3::new(10.0, 20.0, 41.0),
            ..Camera::default()
        };
        let s = c.shifted_by(5.0, -3.0);
        assert!((f64::from(s.eye.x) - 15.0).abs() < 1e-4);
        assert!((f64::from(s.eye.y) - 17.0).abs() < 1e-4);
        assert!((f64::from(s.eye.z) - 41.0).abs() < 1e-4);
    }
}
