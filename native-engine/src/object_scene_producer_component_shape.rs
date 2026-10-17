//! Shape data and the affine application of the component evaluated-shape kernel
//! (L07-H; I16a, D32): `Affine` (Paper `Matrix`), the stroke dict split into path /
//! raster / opaque, and `applyMatrixToStrokeData` over it. See
//! `object_scene_producer_component.rs` for the placement, duplicator and
//! evaluation entry points and the full baseline description.

use serde_json::{Map, Number, Value};

/// A Paper `Matrix` as `[a, b, c, d, tx, ty]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Affine(pub(crate) [f64; 6]);

impl Affine {
    /// `Matrix.transform(new Point(x, y))`.
    pub(crate) fn transform(&self, x: f64, y: f64) -> [f64; 2] {
        let [a, b, c, d, tx, ty] = self.0;
        [x * a + y * c + tx, x * b + y * d + ty]
    }

    /// `Math.sqrt(m.a*m.a + m.b*m.b)`.
    fn column_scale(&self) -> f64 {
        let [a, b, ..] = self.0;
        (a * a + b * b).sqrt()
    }

    /// `Math.sqrt(m.c*m.c + m.d*m.d)`.
    fn row_scale(&self) -> f64 {
        let [_, _, c, d, ..] = self.0;
        (c * c + d * d).sqrt()
    }
}

/// Why a shape could not be evaluated. Never partial: see `EvaluateError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeError {
    /// The stroke dict is not a JSON object.
    NotAnObject,
    /// `segments` is truthy but not an array (`.map` throws in the baseline).
    SegmentsNotArray,
    /// A segment lacks an object shape, a numeric `point`, `handleIn` or `handleOut`.
    MalformedSegment { index: usize },
    /// A raster lacks a numeric `x`, `y`, `width` or `height`.
    MalformedRaster { field: &'static str },
    /// `opacity` is present but is not a number.
    NonNumericOpacity,
    /// A result coordinate, size or opacity is NaN or infinite.
    NonFiniteResult,
}

/// One path segment, handles relative to the point (`serP` convention).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PathSegment {
    pub(crate) point: [f64; 2],
    pub(crate) handle_in: [f64; 2],
    pub(crate) handle_out: [f64; 2],
}

/// A raster stroke dict.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RasterRect {
    /// The original `isRaster` value (any truthy JSON value).
    pub(crate) flag: Value,
    pub(crate) src: Option<Value>,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) opacity: Option<Value>,
    /// Every other field; a transform drops them.
    pub(crate) rest: Map<String, Value>,
}

/// A stroke dict split by what `applyMatrixToStrokeData` does with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ShapeData {
    Path {
        segments: Vec<PathSegment>,
        /// Every field except `segments`.
        rest: Map<String, Value>,
    },
    Raster(RasterRect),
    /// No usable `segments` and not a raster: returned unchanged.
    Opaque(Map<String, Value>),
}

/// JavaScript truthiness of a JSON value.
pub(crate) fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map(|x| x != 0.0 && !x.is_nan()).unwrap_or(true),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn pair(v: Option<&Value>) -> Option<[f64; 2]> {
    let items = v?.as_array()?;
    Some([items.first()?.as_f64()?, items.get(1)?.as_f64()?])
}

pub(crate) fn number(x: f64) -> Value {
    Number::from_f64(x).map_or(Value::Null, Value::Number)
}

fn point_value(p: [f64; 2]) -> Value {
    Value::Array(vec![number(p[0]), number(p[1])])
}

impl ShapeData {
    /// Parse one stroke dict (`sd`) as the baseline reads it.
    pub(crate) fn from_value(value: &Value) -> Result<ShapeData, ShapeError> {
        let map = value.as_object().ok_or(ShapeError::NotAnObject)?;
        if map.get("isRaster").is_some_and(truthy) {
            return RasterRect::from_map(map).map(ShapeData::Raster);
        }
        let items = match map.get("segments") {
            None => return Ok(ShapeData::Opaque(map.clone())),
            Some(s) if !truthy(s) => return Ok(ShapeData::Opaque(map.clone())),
            Some(Value::Array(items)) => items,
            Some(_) => return Err(ShapeError::SegmentsNotArray),
        };
        let mut segments = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let bad = ShapeError::MalformedSegment { index };
            let o = item.as_object().ok_or(bad)?;
            segments.push(PathSegment {
                point: pair(o.get("point")).ok_or(bad)?,
                handle_in: pair(o.get("handleIn")).ok_or(bad)?,
                handle_out: pair(o.get("handleOut")).ok_or(bad)?,
            });
        }
        let mut rest = map.clone();
        rest.remove("segments");
        Ok(ShapeData::Path { segments, rest })
    }

    /// The dict back as JSON (numbers are `f64`).
    pub(crate) fn to_value(&self) -> Value {
        match self {
            ShapeData::Opaque(map) => Value::Object(map.clone()),
            ShapeData::Path { segments, rest } => {
                let mut map = rest.clone();
                let items = segments.iter().map(|s| {
                    let mut o = Map::new();
                    o.insert("point".to_string(), point_value(s.point));
                    o.insert("handleIn".to_string(), point_value(s.handle_in));
                    o.insert("handleOut".to_string(), point_value(s.handle_out));
                    Value::Object(o)
                });
                map.insert("segments".to_string(), Value::Array(items.collect()));
                Value::Object(map)
            }
            ShapeData::Raster(r) => {
                let mut map = r.rest.clone();
                map.insert("isRaster".to_string(), r.flag.clone());
                map.insert("x".to_string(), number(r.x));
                map.insert("y".to_string(), number(r.y));
                map.insert("width".to_string(), number(r.width));
                map.insert("height".to_string(), number(r.height));
                if let Some(src) = &r.src {
                    map.insert("src".to_string(), src.clone());
                }
                if let Some(opacity) = &r.opacity {
                    map.insert("opacity".to_string(), opacity.clone());
                }
                Value::Object(map)
            }
        }
    }

    /// `applyMatrixToStrokeData(cloneStrokeForTransform(sd), m)`: a new value,
    /// `self` is untouched.
    pub(crate) fn apply_affine(&self, m: &Affine) -> Result<ShapeData, ShapeError> {
        match self {
            ShapeData::Opaque(map) => Ok(ShapeData::Opaque(map.clone())),
            ShapeData::Path { segments, rest } => {
                let origin = m.transform(0.0, 0.0);
                let mut moved = Vec::with_capacity(segments.len());
                for s in segments {
                    let hi = m.transform(s.handle_in[0], s.handle_in[1]);
                    let ho = m.transform(s.handle_out[0], s.handle_out[1]);
                    moved.push(PathSegment {
                        point: m.transform(s.point[0], s.point[1]),
                        handle_in: [hi[0] - origin[0], hi[1] - origin[1]],
                        handle_out: [ho[0] - origin[0], ho[1] - origin[1]],
                    });
                }
                let finite = moved.iter().all(|s| {
                    [s.point, s.handle_in, s.handle_out]
                        .iter()
                        .all(|p| p[0].is_finite() && p[1].is_finite())
                });
                if !finite {
                    return Err(ShapeError::NonFiniteResult);
                }
                Ok(ShapeData::Path {
                    segments: moved,
                    rest: rest.clone(),
                })
            }
            ShapeData::Raster(r) => {
                let c = m.transform(r.x, r.y);
                let moved = RasterRect {
                    flag: Value::Bool(true),
                    src: r.src.clone(),
                    x: c[0],
                    y: c[1],
                    width: r.width * m.column_scale(),
                    height: r.height * m.row_scale(),
                    opacity: r.opacity.clone(),
                    rest: Map::new(),
                };
                if ![moved.x, moved.y, moved.width, moved.height]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    return Err(ShapeError::NonFiniteResult);
                }
                Ok(ShapeData::Raster(moved))
            }
        }
    }

    /// The non-geometry fields (what a `data.*` tag lives in).
    pub(crate) fn extras(&self) -> &Map<String, Value> {
        match self {
            ShapeData::Path { rest, .. } => rest,
            ShapeData::Raster(r) => &r.rest,
            ShapeData::Opaque(map) => map,
        }
    }

    /// `(sd2.opacity !== undefined ? sd2.opacity : 1) * factor`.
    pub(crate) fn scale_opacity(&mut self, factor: f64) -> Result<(), ShapeError> {
        let current = match self {
            ShapeData::Raster(r) => r.opacity.as_ref(),
            ShapeData::Path { rest, .. } => rest.get("opacity"),
            ShapeData::Opaque(map) => map.get("opacity"),
        };
        let base = match current {
            None => 1.0,
            Some(Value::Number(n)) => n.as_f64().ok_or(ShapeError::NonNumericOpacity)?,
            Some(_) => return Err(ShapeError::NonNumericOpacity),
        };
        let product = base * factor;
        if !product.is_finite() {
            return Err(ShapeError::NonFiniteResult);
        }
        let value = number(product);
        match self {
            ShapeData::Raster(r) => r.opacity = Some(value),
            ShapeData::Path { rest, .. } => {
                rest.insert("opacity".to_string(), value);
            }
            ShapeData::Opaque(map) => {
                map.insert("opacity".to_string(), value);
            }
        }
        Ok(())
    }
}

impl RasterRect {
    fn from_map(map: &Map<String, Value>) -> Result<RasterRect, ShapeError> {
        let num = |field: &'static str| {
            map.get(field)
                .and_then(Value::as_f64)
                .ok_or(ShapeError::MalformedRaster { field })
        };
        let mut rest = map.clone();
        for key in ["isRaster", "src", "x", "y", "width", "height", "opacity"] {
            rest.remove(key);
        }
        Ok(RasterRect {
            flag: map.get("isRaster").cloned().unwrap_or(Value::Null),
            src: map.get("src").cloned(),
            x: num("x")?,
            y: num("y")?,
            width: num("width")?,
            height: num("height")?,
            opacity: map.get("opacity").cloned(),
            rest,
        })
    }
}
