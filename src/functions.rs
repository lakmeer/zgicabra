


pub fn hyp (v: &[f32;3]) -> f32 {
    let dx = v[0] * v[0];
    let dy = v[1] * v[1];
    let dz = v[2] * v[2];
    (dx + dy + dz).sqrt()
}

pub fn rad_to_cycles (radians: f32) -> f32 {
    1.0 - (radians + PI/2.0 + PI) / (2.0 * PI) % 1.0
}

pub fn button_mask (buttons: u32, mask: u32) -> bool {
    (buttons & mask) != 0
}

pub fn smoothstep (a: f32, b: f32, t: f32) -> f32 {
    let t = (t - a) / (b - a);
    t * t * (3.0 - 2.0 * t)
}
