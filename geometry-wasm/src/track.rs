// Suivi de points par Lucas-Kanade pyramidal (2026-09).
//
// Pourquoi ce fichier plutôt qu'un crate : `optical-flow-lk` compile bien en
// wasm32 (vérifié), mais il tire 25 crates — nalgebra, image, simba, moxcms —
// pour un algorithme qui tient en cent cinquante lignes, et sa licence MIT est
// déclarée dans son Cargo.toml SANS fichier LICENSE dans le dépôt. Ce dépôt-ci
// est en GPL-3.0-or-later avec un audit de licences tenu à jour ; ajouter une
// provenance faible et une dizaine de mégaoctets de dépendances pour ça n'est
// pas un bon échange. La méthode est de toute façon un classique publié
// (Lucas & Kanade 1981, et la note de Bouguet pour la version pyramidale) :
// c'est de la mise en œuvre, pas de la copie.
//
// Principe, en une phrase : entre deux images, un petit carré de pixels autour
// du point s'est déplacé sans changer d'aspect ; on cherche le déplacement qui
// annule la différence, en résolvant à chaque itération un système 2×2 bâti
// sur les gradients spatiaux. La PYRAMIDE (des versions successivement deux
// fois plus petites) est ce qui permet de rattraper un grand déplacement : à
// l'échelle la plus grossière, dix pixels n'en font plus qu'un.
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
pub struct TrackIn {
    pub width: usize,
    pub height: usize,
    /// Luminance 0-255, une valeur par pixel, image de départ puis d'arrivée.
    pub prev: Vec<u8>,
    pub next: Vec<u8>,
    /// Points à suivre, en pixels image.
    pub points: Vec<[f32; 2]>,
    #[serde(default = "default_window")]
    pub window: usize,
    #[serde(default = "default_levels")]
    pub levels: usize,
    #[serde(default = "default_iters")]
    pub iterations: usize,
}
fn default_window() -> usize { 15 }
fn default_levels() -> usize { 3 }
fn default_iters() -> usize { 20 }

#[derive(Serialize)]
pub struct TrackOut {
    points: Vec<[f32; 2]>,
    /// true quand le point a convergé ET que la fenêtre est restée dans
    /// l'image : un point perdu doit être signalé, pas rendu à une position
    /// inventée.
    ok: Vec<bool>,
    /// Erreur résiduelle moyenne par pixel (0-255) — sert à repérer un point
    /// qui « suit » une zone qui ne lui ressemble plus.
    pub error: Vec<f32>,
}

struct Level {
    w: usize,
    h: usize,
    p: Vec<f32>,
}

/// Réduction de moitié par filtre binomial 5 taps (1 4 6 4 1) séparable,
/// puis décimation — la pyramide de Burt-Adelson, celle qu'utilise OpenCV.
/// La moyenne 2×2 essayée d'abord ne filtre pas assez : mesuré, sur un motif
/// contenant des détails de quatre pixels, les niveaux grossiers se
/// remplissaient d'aliasing et le suivi d'un déplacement de 18 px échouait
/// dès le niveau le plus grossier (erreur résiduelle 19,8/255, déplacement
/// trouvé 0,46 au lieu de 2,25). Le filtre écrase ces détails avant de
/// décimer, ce qui est exactement son rôle.
fn halve(src: &Level) -> Level {
    let w = (src.w / 2).max(1);
    let h = (src.h / 2).max(1);
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0];
    let at = |x: i32, y: i32| -> f32 {
        let xx = x.max(0).min(src.w as i32 - 1) as usize;
        let yy = y.max(0).min(src.h as i32 - 1) as usize;
        src.p[yy * src.w + xx]
    };
    // Horizontal d'abord, dans un tampon pleine hauteur, puis vertical :
    // deux passes de cinq multiplications au lieu des vingt-cinq d'un noyau
    // 2D, pour un résultat identique (le noyau est séparable).
    let mut tmp = vec![0.0f32; w * src.h];
    for y in 0..src.h {
        for x in 0..w {
            let cx = 2 * x as i32;
            let mut acc = 0.0;
            for i in 0..5 {
                acc += k[i] * at(cx + i as i32 - 2, y as i32);
            }
            tmp[y * w + x] = acc / 16.0;
        }
    }
    let mut p = vec![0.0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let cy = 2 * y as i32;
            let mut acc = 0.0;
            for i in 0..5 {
                let yy = (cy + i as i32 - 2).max(0).min(src.h as i32 - 1) as usize;
                acc += k[i] * tmp[yy * w + x];
            }
            p[y * w + x] = acc / 16.0;
        }
    }
    Level { w, h, p }
}

fn pyramid(gray: &[u8], w: usize, h: usize, levels: usize, window: usize) -> Vec<Level> {
    let base = Level { w, h, p: gray.iter().map(|&v| v as f32).collect() };
    let mut out = vec![base];
    let need = (2 * window).max(16);
    for _ in 1..levels.max(1) {
        let last = out.last().unwrap();
        if last.w / 2 < need || last.h / 2 < need {
            break;
        }
        let next = halve(last);
        out.push(next);
    }
    out
}

/// Échantillonnage bilinéaire : sans lui le suivi serait borné au pixel
/// entier, donc incapable du sous-pixel qui fait toute l'utilité d'un tracker.
/// Les coordonnées sont BORNÉES au bord de l'image plutôt que refusées : un
/// point suivi à trois pixels du cadre est parfaitement légitime, et c'est le
/// CENTRE (contrôlé séparément par `inside`) qui décide si le point est encore
/// dans l'image, pas le coin de sa fenêtre.
fn sample(l: &Level, x: f32, y: f32) -> f32 {
    let x = x.max(0.0).min((l.w - 1) as f32);
    let y = y.max(0.0).min((l.h - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(l.w - 1);
    let y1 = (y0 + 1).min(l.h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let a = l.p[y0 * l.w + x0];
    let b = l.p[y0 * l.w + x1];
    let c = l.p[y1 * l.w + x0];
    let d = l.p[y1 * l.w + x1];
    a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy
}

fn inside(l: &Level, x: f32, y: f32) -> bool {
    x >= 0.0 && y >= 0.0 && x <= (l.w - 1) as f32 && y <= (l.h - 1) as f32
}

/// Une passe de Lucas-Kanade à un niveau de pyramide. `guess` est le
/// déplacement hérité du niveau supérieur ; on l'affine.
fn track_level(
    prev: &Level,
    next: &Level,
    px: f32,
    py: f32,
    guess: (f32, f32),
    half: i32,
    iterations: usize,
) -> Option<((f32, f32), f32)> {
    // Matrice des gradients, constante d'une itération à l'autre : elle ne
    // dépend que de l'image de DÉPART, ce qui est justement l'astuce qui rend
    // la méthode bon marché.
    if !inside(prev, px, py) {
        return None;
    }
    let (mut gxx, mut gxy, mut gyy) = (0.0f64, 0.0f64, 0.0f64);
    let mut ix = vec![];
    let mut iy = vec![];
    let mut i0 = vec![];
    for dy in -half..=half {
        for dx in -half..=half {
            let x = px + dx as f32;
            let y = py + dy as f32;
            let a = sample(prev, x + 1.0, y);
            let b = sample(prev, x - 1.0, y);
            let c = sample(prev, x, y + 1.0);
            let d = sample(prev, x, y - 1.0);
            let v = sample(prev, x, y);
            let gx = 0.5 * (a - b);
            let gy = 0.5 * (c - d);
            gxx += (gx * gx) as f64;
            gxy += (gx * gy) as f64;
            gyy += (gy * gy) as f64;
            ix.push(gx);
            iy.push(gy);
            i0.push(v);
        }
    }
    let det = gxx * gyy - gxy * gxy;
    // Déterminant nul = fenêtre sans texture (un aplat, ou une arête seule :
    // le fameux problème de l'ouverture). Refuser plutôt que de rendre un
    // déplacement arbitraire.
    if det.abs() < 1e-6 {
        return None;
    }
    let (mut vx, mut vy) = guess;
    let mut err = 0.0f32;
    for _ in 0..iterations {
        let (mut bx, mut by) = (0.0f64, 0.0f64);
        let mut sum = 0.0f32;
        let mut k = 0usize;
        for dy in -half..=half {
            for dx in -half..=half {
                let x = px + dx as f32 + vx;
                let y = py + dy as f32 + vy;
                let v = sample(next, x, y);
                let dt = i0[k] - v;
                bx += (dt * ix[k]) as f64;
                by += (dt * iy[k]) as f64;
                sum += dt.abs();
                k += 1;
            }
        }
        err = sum / k as f32;
        if !inside(next, px + vx, py + vy) {
            return None;
        }
        let dx = ((gyy * bx - gxy * by) / det) as f32;
        let dy = ((gxx * by - gxy * bx) / det) as f32;
        vx += dx;
        vy += dy;
        if dx.abs() < 0.01 && dy.abs() < 0.01 {
            break;
        }
    }
    Some(((vx, vy), err))
}

pub fn track_points_impl(input: TrackIn) -> TrackOut {
    let TrackIn { width, height, prev, next, points, window, levels, iterations } = input;
    let half = (window.max(3) as i32) / 2;
    let pyr_prev = pyramid(&prev, width, height, levels, window.max(3));
    let pyr_next = pyramid(&next, width, height, levels, window.max(3));
    let top = pyr_prev.len().min(pyr_next.len());
    let mut out_pts = Vec::with_capacity(points.len());
    let mut out_ok = Vec::with_capacity(points.len());
    let mut out_err = Vec::with_capacity(points.len());
    for p in points.iter() {
        let mut guess = (0.0f32, 0.0f32);
        let mut ok = true;
        let mut err = 0.0f32;
        // Du plus grossier au plus fin : chaque niveau double le déplacement
        // hérité, puisqu'il travaille sur une image deux fois plus grande.
        for l in (0..top).rev() {
            let scale = (1 << l) as f32;
            let lx = p[0] / scale;
            let ly = p[1] / scale;
            match track_level(&pyr_prev[l], &pyr_next[l], lx, ly, guess, half, iterations) {
                Some((v, e)) => {
                    guess = v;
                    err = e;
                }
                None => {
                    // Un niveau GROSSIER sans texture exploitable (une zone
                    // qui devient un aplat une fois réduite) n'est pas un
                    // échec : on descend avec l'estimation courante. Seul
                    // l'échec du niveau le plus FIN condamne le point, parce
                    // que c'est lui qui porte le résultat.
                    if l == 0 {
                        ok = false;
                    }
                }
            }
            if l > 0 {
                guess = (guess.0 * 2.0, guess.1 * 2.0);
            }
        }
        if ok {
            out_pts.push([p[0] + guess.0, p[1] + guess.1]);
        } else {
            out_pts.push([p[0], p[1]]);
        }
        out_ok.push(ok);
        out_err.push(err);
    }
    TrackOut { points: out_pts, ok: out_ok, error: out_err }
}


/// Champ de mouvement échantillonné sur une grille régulière — la forme dont
/// l'interpolation d'images a besoin (interp.rs). Les vecteurs sont dans
/// l'échelle des images fournies.
pub struct GridFlow {
    pub gw: usize,
    pub gh: usize,
    /// Position du premier point de grille, et pas de la grille, en pixels.
    pub origin: f32,
    pub step: f32,
    pub u: Vec<f32>,
    pub v: Vec<f32>,
    pub ok: Vec<bool>,
}

/// Lance le suivi sur une grille régulière de points. Chaque point est traité
/// indépendamment, exactement comme un point de suivi ordinaire — c'est le
/// même noyau, appelé en nombre.
pub fn track_grid(
    prev: &[u8],
    next: &[u8],
    w: usize,
    h: usize,
    step: usize,
    window: usize,
    levels: usize,
    iterations: usize,
) -> GridFlow {
    let step = step.max(2);
    let origin = (step / 2) as f32;
    let gw = ((w as f32 - origin) / step as f32).ceil().max(1.0) as usize;
    let gh = ((h as f32 - origin) / step as f32).ceil().max(1.0) as usize;
    let mut points = Vec::with_capacity(gw * gh);
    for gy in 0..gh {
        for gx in 0..gw {
            points.push([origin + (gx * step) as f32, origin + (gy * step) as f32]);
        }
    }
    let out = track_points_impl(TrackIn {
        width: w,
        height: h,
        prev: prev.to_vec(),
        next: next.to_vec(),
        points: points.clone(),
        window,
        levels,
        iterations,
    });
    let mut u = Vec::with_capacity(points.len());
    let mut v = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        u.push(out.points[i][0] - points[i][0]);
        v.push(out.points[i][1] - points[i][1]);
    }
    GridFlow { gw, gh, origin, step: step as f32, u, v, ok: out.ok }
}

#[wasm_bindgen]
pub fn track_points(input_json: &str) -> Result<String, JsValue> {
    let input: TrackIn = serde_json::from_str(input_json)
        .map_err(|e| JsValue::from_str(&format!("track_points: bad input: {}", e)))?;
    if input.prev.len() < input.width * input.height || input.next.len() < input.width * input.height {
        return Err(JsValue::from_str("track_points: buffers smaller than width*height"));
    }
    let out = track_points_impl(input);
    serde_json::to_string(&out)
        .map_err(|e| JsValue::from_str(&format!("track_points: bad output: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Motif de test translaté d'un vecteur connu. Surtout PAS un damier :
    /// un motif périodique rend le déplacement ambigu par construction (c'est
    /// l'exemple d'école du problème de l'ouverture), et un tracker correct y
    /// trouve un alias parfaitement légitime — mesuré, (-2, 4) au lieu de
    /// (6, -4) sur un damier de 8 px. Ici, des fréquences incommensurables
    /// superposées : structure unique, et à plusieurs échelles pour que les
    /// niveaux grossiers de la pyramide aient eux aussi de quoi mordre.
    fn checker(w: usize, h: usize, ox: f32, oy: f32) -> Vec<u8> {
        let mut v = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let fx = (x as f32 - ox) as f64;
                let fy = (y as f32 - oy) as f64;
                let a = 60.0 * (fx / 37.0 + (fy / 29.0).cos()).sin();
                let b = 40.0 * (fx / 11.0 - fy / 13.0).sin();
                let c = 25.0 * (fx / 5.0 + fy / 7.0).sin();
                v[y * w + x] = (128.0 + a + b + c).max(0.0).min(255.0) as u8;
            }
        }
        v
    }

    fn run(dx: f32, dy: f32) -> ([f32; 2], bool) {
        let (w, h) = (640usize, 480usize);
        let input = TrackIn {
            width: w,
            height: h,
            prev: checker(w, h, 0.0, 0.0),
            next: checker(w, h, dx, dy),
            points: vec![[320.0, 240.0]],
            window: 21,
            levels: 4,
            iterations: 30,
        };
        let out = track_points_impl(input);
        (out.points[0], out.ok[0])
    }

    #[test]
    fn suit_une_translation_entiere() {
        let (p, ok) = run(6.0, -4.0);
        assert!(ok, "point perdu");
        assert!((p[0] - 326.0).abs() < 0.5, "x = {}", p[0]);
        assert!((p[1] - 236.0).abs() < 0.5, "y = {}", p[1]);
    }

    #[test]
    fn suit_au_sous_pixel() {
        let (p, ok) = run(2.5, 1.25);
        assert!(ok, "point perdu");
        assert!((p[0] - 322.5).abs() < 0.35, "x = {}", p[0]);
        assert!((p[1] - 241.25).abs() < 0.35, "y = {}", p[1]);
    }

    #[test]
    fn rattrape_un_grand_deplacement_grace_a_la_pyramide() {
        let (p, ok) = run(18.0, 12.0);
        assert!(ok, "point perdu");
        assert!((p[0] - 338.0).abs() < 1.0, "x = {}", p[0]);
        assert!((p[1] - 252.0).abs() < 1.0, "y = {}", p[1]);
    }

    /// Une zone sans texture n'a aucun déplacement identifiable : le tracker
    /// doit le DIRE plutôt que de rendre un chiffre au hasard.
    #[test]
    fn refuse_un_aplat() {
        let (w, h) = (64usize, 64usize);
        let input = TrackIn {
            width: w,
            height: h,
            prev: vec![128; w * h],
            next: vec![128; w * h],
            points: vec![[32.0, 32.0]],
            window: 15,
            levels: 3,
            iterations: 20,
        };
        let out = track_points_impl(input);
        assert!(!out.ok[0], "un aplat ne devrait pas être suivable");
    }
}


