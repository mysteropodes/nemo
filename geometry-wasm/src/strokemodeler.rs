// Stroke modeler — Rust port of Google's ink-stroke-modeler core
// (github.com/google/ink-stroke-modeler, Apache-2.0), the spring-mass-
// damper input smoothing Chrome/Android inking and rnote use.
//
// This is the byte-for-byte port of src/js/stroke-modeler.js's JsModeler —
// that file is the REFERENCE implementation and documents the model, the
// three sub-pieces (wobble smoother / position modeler / end-of-stroke
// catch-up) and the level constants. Per CLAUDE.md §3 (duplicated JS/Rust
// pairs), any change to either side must be mirrored in the other in the
// same commit; draw-bridge.js consumes both through one adapter
// (window.SMStrokeModeler.create) with silent JS fallback, so a Rust-only
// bug would be masked as a fallback, never visible — test both paths.
//
// Stateful per-stroke (one instance per gesture), same precedent as
// engine.rs's VelloEngine: the ONLY state is the in-flight gesture's
// physics, nothing document-related ever lives here.
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Clone, Copy)]
struct Params {
    spring_mass: f64,
    drag: f64,
    rate: f64,
    wobble_timeout: f64,
    wobble_floor: f64,
    wobble_ceil: f64,
    stop_dist_px: f64,
    eos_iterations: u32,
}

fn params_for_level(level: u32) -> Params {
    // Level 1 = Google's real defaults (params.h: spring_mass 11/32400,
    // drag_constant = 72.0 — NOT 72/3240, that misreading left the spring
    // essentially undamped and the modeled point orbiting the pen). Levels
    // 2/3 weaken the spring for more lag and lower the drag just enough to
    // stay at-or-above critical damping (ζ≈0.92 / 1.25) — smoother without
    // ringing. Same numbers as stroke-modeler.js LEVELS.
    let base_spring = 11.0 / 32400.0;
    match level {
        2 => Params {
            spring_mass: base_spring * 3.0,
            drag: 57.6,
            rate: 120.0,
            wobble_timeout: 0.07,
            wobble_floor: 90.0,
            wobble_ceil: 140.0,
            stop_dist_px: 0.1,
            eos_iterations: 30,
        },
        3 => Params {
            spring_mass: base_spring * 8.0,
            drag: 48.0,
            rate: 120.0,
            wobble_timeout: 0.12,
            wobble_floor: 140.0,
            wobble_ceil: 260.0,
            stop_dist_px: 0.1,
            eos_iterations: 60,
        },
        _ => Params {
            spring_mass: base_spring,
            drag: 72.0,
            rate: 120.0,
            wobble_timeout: 0.04,
            wobble_floor: 50.0,
            wobble_ceil: 54.0,
            stop_dist_px: 0.1,
            eos_iterations: 20,
        },
    }
}

#[derive(Serialize)]
struct OutPoint {
    x: f64,
    y: f64,
    p: f64,
}

fn pack_points(points: &[OutPoint]) -> Vec<f64> {
    let mut packed = Vec::with_capacity(points.len() * 3);
    for point in points {
        packed.extend_from_slice(&[point.x, point.y, point.p]);
    }
    packed
}

#[wasm_bindgen]
pub struct StrokeModeler {
    params: Params,
    scale: f64,
    pos: [f64; 2],
    vel: [f64; 2],
    started: bool,
    last_input: [f64; 2],
    last_t: f64,
    last_p: f64,
    wobble_win: Vec<(f64, f64, f64, f64)>, // (t, x, y, speed)
}

impl StrokeModeler {
    fn wobble(&mut self, x: f64, y: f64, t: f64) -> [f64; 2] {
        let p = self.params;
        let speed = match self.wobble_win.last() {
            Some(&(lt, lx, ly, _)) => {
                let dt = (t - lt).max(1e-4);
                ((x - lx).hypot(y - ly)) * self.scale / dt
            }
            None => 0.0,
        };
        self.wobble_win.push((t, x, y, speed));
        self.wobble_win.retain(|&(wt, ..)| wt >= t - p.wobble_timeout);
        let n = self.wobble_win.len() as f64;
        let (mut ax, mut ay, mut aspd) = (0.0, 0.0, 0.0);
        for &(_, wx, wy, ws) in &self.wobble_win {
            ax += wx;
            ay += wy;
            aspd += ws;
        }
        ax /= n;
        ay /= n;
        aspd /= n;
        let ratio = ((aspd - p.wobble_floor) / (p.wobble_ceil - p.wobble_floor).max(1e-6)).clamp(0.0, 1.0);
        [ax + (x - ax) * ratio, ay + (y - ay) * ratio]
    }

    fn step(&mut self, target: [f64; 2], ddt: f64) {
        let p = self.params;
        for a in 0..2 {
            let accel = (target[a] - self.pos[a]) / p.spring_mass - p.drag * self.vel[a];
            self.vel[a] += ddt * accel;
            self.pos[a] += ddt * self.vel[a];
        }
    }

    fn move_impl(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<OutPoint> {
        if !self.started {
            return self.down_impl(x, y, t, p);
        }
        let sm = self.wobble(x, y, t);
        let dt = (t - self.last_t).max(1e-4);
        // 2026-09 fix — mirrors stroke-modeler.js's JsModeler::move (same
        // date, same comment there for the full story): capping n at a flat
        // 50 let ddt grow unbounded for any dt past ≈0.42s at rate=120,
        // which diverges this explicit-Euler spring-damper. Substeps are
        // O(1) each, so only ddt needs a ceiling, not the work.
        let n = ((dt * self.params.rate).ceil() as i32).clamp(1, 20000);
        let ddt = dt / n as f64;
        let mut out = Vec::with_capacity(n as usize);
        for i in 1..=n {
            let f = i as f64 / n as f64;
            let target = [
                self.last_input[0] + (sm[0] - self.last_input[0]) * f,
                self.last_input[1] + (sm[1] - self.last_input[1]) * f,
            ];
            self.step(target, ddt);
            out.push(OutPoint {
                x: self.pos[0],
                y: self.pos[1],
                p: self.last_p + (p - self.last_p) * f,
            });
        }
        self.last_input = sm;
        self.last_t = t;
        self.last_p = p;
        out
    }

    fn down_impl(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<OutPoint> {
        self.pos = [x, y];
        self.vel = [0.0, 0.0];
        self.started = true;
        self.last_input = [x, y];
        self.last_t = t;
        self.last_p = p;
        self.wobble_win.clear();
        self.wobble_win.push((t, x, y, 0.0));
        vec![OutPoint { x, y, p }]
    }
}

#[wasm_bindgen]
impl StrokeModeler {
    #[wasm_bindgen(constructor)]
    pub fn new(level: u32, unit_scale: f64) -> StrokeModeler {
        StrokeModeler {
            params: params_for_level(level),
            scale: if unit_scale > 0.0 { unit_scale } else { 1.0 },
            pos: [0.0, 0.0],
            vel: [0.0, 0.0],
            started: false,
            last_input: [0.0, 0.0],
            last_t: 0.0,
            last_p: 1.0,
            wobble_win: Vec::new(),
        }
    }

    pub fn down(&mut self, x: f64, y: f64, t: f64, p: f64) -> String {
        serde_json::to_string(&self.down_impl(x, y, t, p)).unwrap()
    }

    /// Allocation-light browser path. The legacy JSON methods stay exported
    /// for compatibility; new JS consumes these packed x/y/pressure triplets
    /// without stringify/parse on every pointer event.
    pub fn down_packed(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<f64> {
        pack_points(&self.down_impl(x, y, t, p))
    }

    #[wasm_bindgen(js_name = move)]
    pub fn move_(&mut self, x: f64, y: f64, t: f64, p: f64) -> String {
        serde_json::to_string(&self.move_impl(x, y, t, p)).unwrap()
    }

    pub fn move_packed(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<f64> {
        pack_points(&self.move_impl(x, y, t, p))
    }

    pub fn up(&mut self, x: f64, y: f64, t: f64, p: f64) -> String {
        serde_json::to_string(&self.up_impl(x, y, t, p)).unwrap()
    }

    pub fn up_packed(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<f64> {
        pack_points(&self.up_impl(x, y, t, p))
    }
}

impl StrokeModeler {
    fn up_impl(&mut self, x: f64, y: f64, t: f64, p: f64) -> Vec<OutPoint> {
        let mut out = self.move_impl(x, y, t, p);
        // End-of-stroke catch-up toward the FINAL raw input — Google's
        // ModelEndOfStroke (position_modeler.h) verbatim: overshooting
        // steps (anchor's projection onto the step segment lands before
        // its end) are discarded and retried with half the time step, so
        // the tail settles ONTO the lift-off point instead of orbiting it.
        let params = self.params;
        let mut ddt = 1.0 / params.rate;
        let target = [x, y];
        let stop_world = params.stop_dist_px / self.scale.max(1e-9);
        for _ in 0..params.eos_iterations {
            let prev_pos = self.pos;
            let prev_vel = self.vel;
            self.step(target, ddt);
            let step_dist = (self.pos[0] - prev_pos[0]).hypot(self.pos[1] - prev_pos[1]);
            if step_dist < stop_world {
                self.pos = prev_pos;
                self.vel = prev_vel;
                break;
            }
            let dx = self.pos[0] - prev_pos[0];
            let dy = self.pos[1] - prev_pos[1];
            let tt = ((target[0] - prev_pos[0]) * dx + (target[1] - prev_pos[1]) * dy) / (step_dist * step_dist);
            if tt < 1.0 {
                self.pos = prev_pos;
                self.vel = prev_vel;
                ddt *= 0.5;
                continue;
            }
            out.push(OutPoint {
                x: self.pos[0],
                y: self.pos[1],
                p,
            });
            if (target[0] - self.pos[0]).hypot(target[1] - self.pos[1]) < stop_world {
                break;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_same_points(json: &str, packed: &[f64]) {
        let decoded: Vec<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert_eq!(packed.len(), decoded.len() * 3);
        for (index, point) in decoded.iter().enumerate() {
            for (offset, key) in ["x", "y", "p"].iter().enumerate() {
                let expected = point[*key].as_f64().unwrap();
                let actual = packed[index * 3 + offset];
                assert!(
                    (actual - expected).abs() <= f64::EPSILON * actual.abs().max(1.0),
                    "point {index} {key}: packed={actual}, json={expected}"
                );
            }
        }
    }

    #[test]
    fn packed_api_matches_legacy_json_for_a_complete_stroke() {
        let mut json_modeler = StrokeModeler::new(2, 1.25);
        let mut packed_modeler = StrokeModeler::new(2, 1.25);

        let json = json_modeler.down(12.0, 18.0, 1.0, 0.4);
        let packed = packed_modeler.down_packed(12.0, 18.0, 1.0, 0.4);
        assert_same_points(&json, &packed);

        for &(x, y, t, p) in &[
            (14.0, 19.0, 1.008, 0.5),
            (22.0, 25.0, 1.021, 0.8),
            (31.0, 21.0, 1.045, 0.65),
        ] {
            let json = json_modeler.move_(x, y, t, p);
            let packed = packed_modeler.move_packed(x, y, t, p);
            assert_same_points(&json, &packed);
        }

        let json = json_modeler.up(36.0, 24.0, 1.06, 0.3);
        let packed = packed_modeler.up_packed(36.0, 24.0, 1.06, 0.3);
        assert_same_points(&json, &packed);
    }
}
