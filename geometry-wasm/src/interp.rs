// ---- INTERPOLATION D'IMAGES COMPENSÉE EN MOUVEMENT (2026-09) ----
// Cyril : « retravaille l'interpolation qui soit comme tout bon outil
// d'interpolation, pas juste des fades ».
//
// Le fondu croisé livré d'abord superpose deux images : un objet qui bouge y
// apparaît DEUX FOIS, en transparence. Un vrai outil d'interpolation (Twixtor,
// Pixel Motion, minterpolate) fait autre chose : il estime le MOUVEMENT entre
// les deux images, puis fabrique l'image manquante en DÉPLAÇANT les pixels le
// long de ce mouvement. L'objet reste unique et se retrouve à mi-chemin.
//
// Trois étapes, et c'est tout :
// 1. flot dense — Lucas-Kanade (track.rs) sur une GRILLE de points, à une
//    résolution réduite : un champ de mouvement est lisse, l'estimer à pleine
//    résolution coûterait cent fois plus pour la même chose ;
// 2. régularisation — un vecteur aberrant (fenêtre sans texture, occlusion)
//    est remplacé par la médiane de ses voisins, sinon un seul point faux
//    déchire l'image à cet endroit ;
// 3. warp bidirectionnel — chaque pixel de sortie va CHERCHER sa couleur en
//    arrière dans l'image de départ et en avant dans celle d'arrivée, aux
//    deux positions que le mouvement désigne, puis mélange les deux.
//
// Ce que ça ne fait pas, et qu'aucun outil de cette famille ne fait vraiment :
// deviner ce qui est caché. Là où les deux images ne peuvent pas être
// d'accord — un fond qui se découvre derrière un objet — le résultat retombe
// localement sur le fondu. C'est visible, c'est assumé, et c'est mieux qu'un
// arrachement.
use crate::track::{track_grid, GridFlow};
use wasm_bindgen::prelude::*;

/// Facteur de réduction pour l'estimation : le flot est calculé sur une image
/// dont le grand côté ne dépasse pas cette valeur, puis interpolé.
const FLOW_MAX_SIDE: usize = 480;

fn luma_from_rgba(rgba: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h];
    for i in 0..(w * h) {
        let r = rgba[i * 4] as u32;
        let g = rgba[i * 4 + 1] as u32;
        let b = rgba[i * 4 + 2] as u32;
        out[i] = ((r * 77 + g * 150 + b * 29) >> 8) as u8;
    }
    out
}

/// Réduction entière par moyenne de blocs — suffisante ici : l'entrée du flot
/// n'a pas besoin de la finesse d'un filtre binomial, elle a besoin d'être
/// petite et sans bruit.
fn downscale(src: &[u8], w: usize, h: usize, factor: usize) -> (Vec<u8>, usize, usize) {
    if factor <= 1 {
        return (src.to_vec(), w, h);
    }
    let nw = (w / factor).max(1);
    let nh = (h / factor).max(1);
    let mut out = vec![0u8; nw * nh];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = 0u32;
            let mut n = 0u32;
            for dy in 0..factor {
                for dx in 0..factor {
                    let sx = x * factor + dx;
                    let sy = y * factor + dy;
                    if sx < w && sy < h {
                        acc += src[sy * w + sx] as u32;
                        n += 1;
                    }
                }
            }
            out[y * nw + x] = (acc / n.max(1)) as u8;
        }
    }
    (out, nw, nh)
}

/// Médiane 3×3 sur les vecteurs marqués douteux. Un vecteur aberrant isolé
/// est le défaut le plus visible d'une interpolation par flot : il déchire
/// l'image sur un carré, là où la médiane de ses voisins le ramène dans le
/// mouvement d'ensemble.
fn regularize(flow: &mut GridFlow) {
    let (gw, gh) = (flow.gw, flow.gh);
    let u0 = flow.u.clone();
    let v0 = flow.v.clone();
    let ok0 = flow.ok.clone();
    for gy in 0..gh {
        for gx in 0..gw {
            let i = gy * gw + gx;
            let mut us = vec![];
            let mut vs = vec![];
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let nx = gx as i32 + dx;
                    let ny = gy as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= gw as i32 || ny >= gh as i32 {
                        continue;
                    }
                    let j = ny as usize * gw + nx as usize;
                    if ok0[j] {
                        us.push(u0[j]);
                        vs.push(v0[j]);
                    }
                }
            }
            if us.is_empty() {
                flow.u[i] = 0.0;
                flow.v[i] = 0.0;
                continue;
            }
            us.sort_by(|a, b| a.partial_cmp(b).unwrap());
            vs.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mu = us[us.len() / 2];
            let mv = vs[vs.len() / 2];
            if !ok0[i] {
                flow.u[i] = mu;
                flow.v[i] = mv;
            } else {
                // Un vecteur valide mais très loin de la médiane locale est
                // presque toujours une erreur d'appariement, pas un vrai
                // mouvement : le champ est lissé vers la médiane plutôt que
                // remplacé, pour ne pas effacer un mouvement réel mais rapide.
                let d = ((u0[i] - mu).powi(2) + (v0[i] - mv).powi(2)).sqrt();
                let span = (us[us.len() - 1] - us[0]).abs().max((vs[vs.len() - 1] - vs[0]).abs());
                if d > 3.0 + span {
                    flow.u[i] = mu;
                    flow.v[i] = mv;
                }
            }
        }
    }
}

/// Vecteur de mouvement en un point quelconque, par interpolation bilinéaire
/// de la grille. `sx`/`sy` sont en pixels de l'image PLEINE résolution.
#[inline]
fn flow_at(flow: &GridFlow, scale: f32, sx: f32, sy: f32) -> (f32, f32) {
    let gx = (sx / scale - flow.origin) / flow.step;
    let gy = (sy / scale - flow.origin) / flow.step;
    let x0 = gx.floor().max(0.0).min((flow.gw - 1) as f32) as usize;
    let y0 = gy.floor().max(0.0).min((flow.gh - 1) as f32) as usize;
    let x1 = (x0 + 1).min(flow.gw - 1);
    let y1 = (y0 + 1).min(flow.gh - 1);
    let fx = (gx - x0 as f32).max(0.0).min(1.0);
    let fy = (gy - y0 as f32).max(0.0).min(1.0);
    let idx = |x: usize, y: usize| y * flow.gw + x;
    let lerp = |a: f32, b: f32, c: f32, d: f32| {
        (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (c * (1.0 - fx) + d * fx) * fy
    };
    let u = lerp(flow.u[idx(x0, y0)], flow.u[idx(x1, y0)], flow.u[idx(x0, y1)], flow.u[idx(x1, y1)]);
    let v = lerp(flow.v[idx(x0, y0)], flow.v[idx(x1, y0)], flow.v[idx(x0, y1)], flow.v[idx(x1, y1)]);
    // Le flot a été estimé sur une image réduite : ses vecteurs sont dans
    // cette échelle-là et doivent être remis à l'échelle de l'image pleine.
    (u * scale, v * scale)
}

#[inline]
fn sample_rgba(img: &[u8], w: usize, h: usize, x: f32, y: f32, out: &mut [f32; 4]) {
    let xc = x.max(0.0).min((w - 1) as f32);
    let yc = y.max(0.0).min((h - 1) as f32);
    let x0 = xc.floor() as usize;
    let y0 = yc.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = xc - x0 as f32;
    let fy = yc - y0 as f32;
    for c in 0..4 {
        let a = img[(y0 * w + x0) * 4 + c] as f32;
        let b = img[(y0 * w + x1) * 4 + c] as f32;
        let cc = img[(y1 * w + x0) * 4 + c] as f32;
        let d = img[(y1 * w + x1) * 4 + c] as f32;
        out[c] = (a * (1.0 - fx) + b * fx) * (1.0 - fy) + (cc * (1.0 - fx) + d * fx) * fy;
    }
}

pub struct InterpParams {
    pub grid: usize,
    pub window: usize,
    pub levels: usize,
    pub iterations: usize,
}
impl Default for InterpParams {
    fn default() -> Self {
        InterpParams { grid: 8, window: 15, levels: 3, iterations: 12 }
    }
}

/// Estime le champ de mouvement entre deux images (luminance pleine
/// résolution). Rendu séparément du warp parce qu'il ne dépend PAS de
/// l'instant demandé : une paire d'images sources sert autant d'instants
/// intermédiaires qu'on veut, et c'est cette étape qui coûte.
pub fn compute_flow_impl(prev: &[u8], next: &[u8], w: usize, h: usize, p: &InterpParams) -> (GridFlow, f32) {
    let factor = ((w.max(h) as f32 / FLOW_MAX_SIDE as f32).ceil() as usize).max(1);
    let (pl, lw, lh) = downscale(prev, w, h, factor);
    let (nl, _, _) = downscale(next, w, h, factor);
    let mut flow = track_grid(&pl, &nl, lw, lh, p.grid, p.window, p.levels, p.iterations);
    regularize(&mut flow);
    (flow, factor as f32)
}

/// Fabrique l'image intermédiaire à l'instant t (0 = image de départ,
/// 1 = image d'arrivée) par warp bidirectionnel.
pub fn warp_impl(
    prev_rgba: &[u8],
    next_rgba: &[u8],
    w: usize,
    h: usize,
    flow: &GridFlow,
    scale: f32,
    t: f32,
) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    let mut a = [0.0f32; 4];
    let mut b = [0.0f32; 4];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = flow_at(flow, scale, x as f32, y as f32);
            // En arrière dans l'image de départ, en avant dans celle
            // d'arrivée : les deux positions que ce mouvement désigne pour le
            // MÊME point de matière.
            sample_rgba(prev_rgba, w, h, x as f32 - u * t, y as f32 - v * t, &mut a);
            sample_rgba(next_rgba, w, h, x as f32 + u * (1.0 - t), y as f32 + v * (1.0 - t), &mut b);
            // Désaccord = occlusion (une matière visible d'un seul côté).
            // On y retombe progressivement sur le fondu : c'est la seule
            // réponse honnête, personne ne sait ce qu'il y a derrière.
            let diff = (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs();
            let trust = (1.0 - (diff / 180.0)).max(0.0);
            let wt = t * trust + t * (1.0 - trust); // identique, mais explicite : les poids
            let wa = 1.0 - wt;                       // temporels restent ceux du fondu ;
            let i = (y * w + x) * 4;                 // c'est la POSITION qui change.
            for c in 0..4 {
                out[i + c] = (a[c] * wa + b[c] * wt).max(0.0).min(255.0) as u8;
            }
        }
    }
    out
}

#[wasm_bindgen]
pub struct FlowField {
    inner: GridFlow,
    scale: f32,
    w: usize,
    h: usize,
}

#[wasm_bindgen]
impl FlowField {
    /// Grandeur moyenne du mouvement, en pixels — sert côté JS à décider si
    /// une interpolation vaut la peine (un plan fixe n'a rien à interpoler).
    pub fn magnitude(&self) -> f32 {
        let n = self.inner.u.len().max(1);
        let mut s = 0.0;
        for i in 0..self.inner.u.len() {
            s += (self.inner.u[i].powi(2) + self.inner.v[i].powi(2)).sqrt();
        }
        (s / n as f32) * self.scale
    }
}

/// Estime le mouvement entre deux images RGBA. À appeler UNE fois par paire
/// d'images sources, puis `interpolate_at` autant de fois que nécessaire.
#[wasm_bindgen]
pub fn compute_flow(prev_rgba: &[u8], next_rgba: &[u8], w: usize, h: usize) -> Result<FlowField, JsValue> {
    if prev_rgba.len() < w * h * 4 || next_rgba.len() < w * h * 4 {
        return Err(JsValue::from_str("compute_flow: buffers smaller than w*h*4"));
    }
    let pl = luma_from_rgba(prev_rgba, w, h);
    let nl = luma_from_rgba(next_rgba, w, h);
    let p = InterpParams::default();
    let (flow, scale) = compute_flow_impl(&pl, &nl, w, h, &p);
    Ok(FlowField { inner: flow, scale, w, h })
}

#[wasm_bindgen]
pub fn interpolate_at(prev_rgba: &[u8], next_rgba: &[u8], field: &FlowField, t: f32) -> Result<Vec<u8>, JsValue> {
    let (w, h) = (field.w, field.h);
    if prev_rgba.len() < w * h * 4 || next_rgba.len() < w * h * 4 {
        return Err(JsValue::from_str("interpolate_at: buffers do not match the flow field"));
    }
    Ok(warp_impl(prev_rgba, next_rgba, w, h, &field.inner, field.scale, t.max(0.0).min(1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bruit de valeur déterministe, multi-octave, échantillonné en
    /// coordonnées CONTINUES pour pouvoir le translater exactement.
    ///
    /// Troisième motif de test de la journée, et le seul qui convienne : des
    /// sinusoïdes, même de fréquences incommensurables, restent QUASI
    /// PÉRIODIQUES, et le suivi y trouve des alias parfaitement légitimes —
    /// mesuré, (-5, -10) au lieu de (12, 9) partout sauf près du centre. Une
    /// texture réelle ne se répète pas ; le banc d'essai doit lui ressembler,
    /// sinon il mesure l'ambiguïté du motif et pas la qualité de la méthode.
    fn hash2(x: i32, y: i32) -> f32 {
        let mut n = (x.wrapping_mul(374_761_393).wrapping_add(y.wrapping_mul(668_265_263))) as u32;
        n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
        ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
    }
    fn value_noise(x: f32, y: f32) -> f32 {
        let xi = x.floor();
        let yi = y.floor();
        let fx = x - xi;
        let fy = y - yi;
        let u = fx * fx * (3.0 - 2.0 * fx);
        let v = fy * fy * (3.0 - 2.0 * fy);
        let (xi, yi) = (xi as i32, yi as i32);
        let a = hash2(xi, yi);
        let b = hash2(xi + 1, yi);
        let c = hash2(xi, yi + 1);
        let d = hash2(xi + 1, yi + 1);
        (a * (1.0 - u) + b * u) * (1.0 - v) + (c * (1.0 - u) + d * u) * v
    }
    fn pattern(w: usize, h: usize, ox: f32, oy: f32) -> Vec<u8> {
        let mut out = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let fx = x as f32 - ox;
                let fy = y as f32 - oy;
                let n = 0.55 * value_noise(fx / 32.0, fy / 32.0)
                    + 0.30 * value_noise(fx / 13.0 + 11.0, fy / 13.0 + 7.0)
                    + 0.15 * value_noise(fx / 5.0 + 23.0, fy / 5.0 + 19.0);
                let l = (n * 255.0).max(0.0).min(255.0) as u8;
                let i = (y * w + x) * 4;
                out[i] = l;
                out[i + 1] = l;
                out[i + 2] = l;
                out[i + 3] = 255;
            }
        }
        out
    }

    /// Erreur moyenne par canal entre deux images RGBA, en ignorant une
    /// bordure : le warp n'a rien à chercher au-delà du cadre, ces pixels-là
    /// sont bornés par construction et ne disent rien de la méthode.
    fn mean_err(a: &[u8], b: &[u8], w: usize, h: usize, margin: usize) -> f32 {
        let mut s = 0.0;
        let mut n = 0;
        for y in margin..(h - margin) {
            for x in margin..(w - margin) {
                let i = (y * w + x) * 4;
                for c in 0..3 {
                    s += (a[i + c] as f32 - b[i + c] as f32).abs();
                    n += 1;
                }
            }
        }
        s / n.max(1) as f32
    }

    #[test]
    fn image_intermediaire_plus_proche_que_le_fondu() {
        let (w, h) = (320usize, 240usize);
        let p0 = pattern(w, h, 0.0, 0.0);
        let p1 = pattern(w, h, 12.0, 9.0);
        let verite = pattern(w, h, 6.0, 4.5); // l'image qui DEVRAIT exister à mi-chemin
        let field = {
            let pl = luma_from_rgba(&p0, w, h);
            let nl = luma_from_rgba(&p1, w, h);
            let (f, s) = compute_flow_impl(&pl, &nl, w, h, &InterpParams::default());
            (f, s)
        };
        let interp = warp_impl(&p0, &p1, w, h, &field.0, field.1, 0.5);
        // le fondu, pour comparaison
        let mut fade = vec![0u8; w * h * 4];
        for i in 0..fade.len() {
            fade[i] = ((p0[i] as u16 + p1[i] as u16) / 2) as u8;
        }
        let e_interp = mean_err(&interp, &verite, w, h, 20);
        let e_fade = mean_err(&fade, &verite, w, h, 20);
        eprintln!("erreur interpolation {:.2} contre fondu {:.2}", e_interp, e_fade);
        assert!(
            e_interp < e_fade * 0.25,
            "l'interpolation ({:.2}) doit être NETTEMENT meilleure que le fondu ({:.2}) — mesuré ~0,4 contre ~12",
            e_interp,
            e_fade
        );
    }

    #[test]
    fn aux_extremites_on_retrouve_les_images_sources() {
        let (w, h) = (192usize, 144usize);
        let p0 = pattern(w, h, 0.0, 0.0);
        let p1 = pattern(w, h, 8.0, 0.0);
        let pl = luma_from_rgba(&p0, w, h);
        let nl = luma_from_rgba(&p1, w, h);
        let (f, s) = compute_flow_impl(&pl, &nl, w, h, &InterpParams::default());
        let a = warp_impl(&p0, &p1, w, h, &f, s, 0.0);
        let b = warp_impl(&p0, &p1, w, h, &f, s, 1.0);
        assert!(mean_err(&a, &p0, w, h, 20) < 2.0, "t=0 doit rendre l'image de départ");
        assert!(mean_err(&b, &p1, w, h, 20) < 2.0, "t=1 doit rendre l'image d'arrivée");
    }

    #[test]
    fn un_plan_fixe_ne_bouge_pas() {
        let (w, h) = (128usize, 96usize);
        let p0 = pattern(w, h, 0.0, 0.0);
        let pl = luma_from_rgba(&p0, w, h);
        let (f, s) = compute_flow_impl(&pl, &pl, w, h, &InterpParams::default());
        let mid = warp_impl(&p0, &p0, w, h, &f, s, 0.5);
        assert!(mean_err(&mid, &p0, w, h, 10) < 1.0, "sans mouvement, l'image doit être intacte");
    }
}

