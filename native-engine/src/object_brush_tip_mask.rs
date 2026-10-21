//! Seeded procedural brush masks and sampling used by the existing charged baker.
use crate::evaluation::object_scene_style_kernels::{
    bitmap_brush_resources_stamp::{Lcg, StampRules},
    object_scene_producer_brush_slot::BakeRefused,
};

pub(super) const TIP: usize = 64;

#[derive(Clone, Copy)]
pub(super) struct Radial { x: f64, y: f64, radius: f64, alpha: f64 }
#[derive(Clone, Copy)]
pub(super) struct Bristle { x: f64, y: f64, radius: f64, alpha: f64 }
pub(super) enum TipShape {
    Soft, Hard, Polygon(Vec<[f64;2]>), Radials(Vec<Radial>),
    Circles(Vec<Radial>), Bristles(Vec<Bristle>), Marker,
}
fn rng(seed: f64) -> Lcg { StampRules::BASELINE.rng(seed) }
fn polar(rng: &mut Lcg, radius: f64, uniform_area: bool) -> [f64;2] {
    let angle = rng.next()*std::f64::consts::TAU;
    let distance = if uniform_area { rng.next().sqrt() } else { rng.next() } * radius;
    [32.+angle.cos()*distance,32.+angle.sin()*distance]
}
pub(super) fn tip_shape(name: &str, seed: f64) -> Result<TipShape, BakeRefused> {
    let mut random = rng(seed);
    Ok(match name {
        // Canvas source-atop black grain changes RGB, not alpha. Only alpha
        // survives the original final destination-in tint, so grain=soft.
        "soft" | "grain" => TipShape::Soft,
        "hard" => TipShape::Hard,
        "marker-flat" => TipShape::Marker,
        "charcoal-rough" => {
            let mut points = Vec::new();
            for i in 0..=14 {
                let angle = i as f64/14.*std::f64::consts::TAU;
                let radius = 32.*(0.7+random.next()*0.3);
                points.push([32.+angle.cos()*radius,32.+angle.sin()*radius]);
            }
            TipShape::Polygon(points)
        }
        "splatter" => {
            let count = 5+(random.next()*4.).floor() as usize;
            let mut blobs = Vec::new();
            for _ in 0..count {
                let p=polar(&mut random,32.*0.55,false);
                blobs.push(Radial{x:p[0],y:p[1],radius:32.*(0.35+random.next()*0.5),alpha:0.9});
            }
            TipShape::Radials(blobs)
        }
        "watercolor-edge" => {
            let mut blobs = Vec::new();
            for _ in 0..5 {
                let x=32.+(random.next()-0.5)*32.*0.4;
                let y=32.+(random.next()-0.5)*32.*0.4;
                let radius=32.*(0.6+random.next()*0.4);
                blobs.push(Radial{x,y,radius,alpha:0.35+random.next()*0.25});
            }
            TipShape::Radials(blobs)
        }
        "ink-splatter-fine" | "stipple" => {
            let fine=name=="ink-splatter-fine";
            let mut circles=Vec::new();
            if fine { circles.push(Radial{x:32.,y:32.,radius:32.*0.45,alpha:1.}); }
            for _ in 0..if fine {30} else {90} {
                let p=if fine {
                    let angle=random.next()*std::f64::consts::TAU;
                    let d=32.*(0.4+random.next()*0.55);
                    [32.+angle.cos()*d,32.+angle.sin()*d]
                } else { polar(&mut random,32.,true) };
                // Original stipple draws opacity before its radius draw.
                let alpha=if fine {1.} else {0.5+random.next()*0.5};
                let radius=if fine {0.5+random.next()*2.2} else {1.2+random.next()*1.5};
                circles.push(Radial{x:p[0],y:p[1],radius,alpha});
            }
            TipShape::Circles(circles)
        }
        "dry-brush" => {
            let mut strokes=Vec::new();
            for _ in 0..18 {
                let angle=random.next()*std::f64::consts::TAU;
                let length=32.*(0.5+random.next()*0.5);
                let width=0.8+random.next()*1.8;
                strokes.push(Bristle{x:32.+angle.cos()*length,y:32.+angle.sin()*length,
                    radius:width/2.,alpha:0.4+random.next()*0.5});
            }
            TipShape::Bristles(strokes)
        }
        _ => return Err(BakeRefused("native imported brush tip mask is unavailable")),
    })
}
pub(super) fn over(destination: f64, source: f64) -> f64 { source+destination*(1.-source) }
impl TipShape {
    pub(super) fn alpha(&self,x:f64,y:f64)->f64 {
        match self {
            Self::Soft => {
                let r=((x-32.).powi(2)+(y-32.).powi(2)).sqrt()/32.;
                if r<=0.7 {1.-r/0.7*0.45} else {((1.-r)/0.3*0.55).max(0.)}
            }
            Self::Hard => if ((x-32.).powi(2)+(y-32.).powi(2))<=31.*31. {1.} else {0.},
            Self::Marker => {
                let (dx,dy)=(x-32.,y-32.);
                let (s,c)=(-0.35_f64).sin_cos();
                if (c*dx+s*dy).abs()<=64.*0.46&&(-s*dx+c*dy).abs()<=64.*0.2 {1.} else {0.}
            }
            Self::Polygon(points) => {
                let mut inside=false;let mut j=points.len()-1;
                for i in 0..points.len() {
                    let (a,b)=(points[i],points[j]);
                    if (a[1]>y)!=(b[1]>y)&&x<(b[0]-a[0])*(y-a[1])/(b[1]-a[1])+a[0] {inside=!inside;}
                    j=i;
                }
                if inside {1.} else {0.}
            }
            Self::Radials(blobs) => blobs.iter().fold(0.,|a,b| {
                over(a,(1.-((x-b.x).powi(2)+(y-b.y).powi(2)).sqrt()/b.radius).max(0.)*b.alpha)
            }),
            Self::Circles(blobs) => blobs.iter().fold(0.,|a,b| {
                over(a,if (x-b.x).powi(2)+(y-b.y).powi(2)<=b.radius*b.radius {b.alpha} else {0.})
            }),
            Self::Bristles(strokes) => strokes.iter().fold(0.,|a,b| {
                let (dx,dy)=(b.x-32.,b.y-32.);
                let t=(((x-32.)*dx+(y-32.)*dy)/(dx*dx+dy*dy)).clamp(0.,1.);
                over(a,if (x-32.-t*dx).powi(2)+(y-32.-t*dy).powi(2)<=b.radius*b.radius {b.alpha} else {0.})
            }),
        }
    }
}
// Native coverage integration for the original 64px procedural mask. Its
// backend pixel parity is an explicit real-GPU validation obligation, not a
// claim that the Canvas2D implementation was executed by this source packet.
pub(super) fn tip_pixels(shape:&TipShape)->[f64;TIP*TIP] {
    let mut pixels=[0.;TIP*TIP];
    for y in 0..TIP { for x in 0..TIP {
        let mut sum=0.;
        for sy in 0..16 { for sx in 0..16 {
            sum+=shape.alpha(x as f64+(sx as f64+0.5)/16.,y as f64+(sy as f64+0.5)/16.);
        }}
        pixels[y*TIP+x]=(sum/256.*255.).round()/255.;
    }}
    pixels
}
pub(super) fn sample_tip(pixels:&[f64;TIP*TIP],x:f64,y:f64)->f64 {
    let x=x-0.5;let y=y-0.5;let (ix,iy)=(x.floor() as i32,y.floor() as i32);
    let at=|px:i32,py:i32| if px>=0&&py>=0&&px<TIP as i32&&py<TIP as i32 {
        pixels[py as usize*TIP+px as usize]
    } else {0.};
    let (u,v)=(x-x.floor(),y-y.floor());
    (at(ix,iy)*(1.-u)+at(ix+1,iy)*u)*(1.-v)+(at(ix,iy+1)*(1.-u)+at(ix+1,iy+1)*u)*v
}
