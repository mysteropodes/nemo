/* tslint:disable */
/* eslint-disable */

export class StrokeModeler {
    free(): void;
    [Symbol.dispose](): void;
    down(x: number, y: number, t: number, p: number): string;
    move(x: number, y: number, t: number, p: number): string;
    constructor(level: number, unit_scale: number);
    up(x: number, y: number, t: number, p: number): string;
}

export class VelloEngine {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Project-load hygiene (importJSON): every stroke dict is new, so every
     * stored path is garbage at once — cheaper than waiting for the GC.
     */
    clear_paths(): void;
    clear_selection(): void;
    get_selection(): string;
    /**
     * Named handle positions (world space) for the free-transform gizmo —
     * 8 box handles (corners + edge midpoints) plus a rotate handle offset
     * above the top edge. The rotate-handle offset is divided by the
     * current zoom so it looks like a constant screen-space distance
     * regardless of how zoomed in/out the view is (mirrors how
     * Paper.js-based tools.js already does this: handle sizes/offsets
     * divided by view.zoom). Returns `null` if nothing is selected.
     */
    gizmo_handles(scene_json: string): string;
    /**
     * Lets JS skip a redundant `register_image` upload for an image it's
     * already registered this session (images are cached for the engine's
     * whole lifetime, not per-scene, so this is a simple presence check).
     */
    has_image(id: string): boolean;
    path_store_size(): number;
    /**
     * Registers (or re-registers, if `key` already exists — e.g. the
     * author just edited the source) a user-authored custom WGSL effect
     * (2026-07, feedback: "la possibilité d'ajouter ses propres effets
     * wgsl et leur paramètre correspondant") — `key` is a JS-chosen stable
     * id (e.g. "custom:<uuid>") later used as an EffectIn.effect_type,
     * `fs_body` is ONLY the body of the fragment shader (a sequence of
     * WGSL statements ending in `return vec4<f32>(...)`), wrapped here
     * into a full document that already declares the standard fullscreen-
     * triangle vertex shader, the texture/sampler/Params bindings, and
     * three convenience locals every author can use without re-deriving
     * them: `uv` (0..1), `src` (the pixel already sampled at `uv`), and
     * `texel` (1 texel in UV units, for neighbor-sampling effects). Same
     * `Params{effect_id,p1,p2,p3,tex_w,tex_h,time,p4}` layout as
     * simple_fx.wgsl, so an author's `params.p1`..`params.p4` map 1:1 onto
     * the SAME p1..p4 fields the stack UI's generic param editor already
     * writes for every other effect type — no separate wiring needed on
     * the JS side for a custom effect's parameters.
     *
     * Compiling arbitrary author-supplied WGSL at runtime is safe here:
     * this crate only ever targets the web/WebGPU wgpu backend (built via
     * `wasm-pack build --target web`), where shader compilation is the
     * BROWSER's own WebGPU implementation doing the work — invalid WGSL
     * produces a normal asynchronous validation error via the browser's
     * uncaptured-error mechanism (the SAME "wgpu uncaptured error" console
     * messages every other shader bug in this file already produces),
     * never a Rust panic or a corrupted wasm instance.
     */
    register_custom_effect(key: string, fs_body: string): void;
    /**
     * Uploads (or re-uploads, if already cached under this id) an image's
     * raw RGBA8 pixels, keyed by a caller-chosen stable `id` — JS calls this
     * ONCE per distinct image (e.g. keyed by the Raster's own data URL) and
     * then just references `id` in every subsequent `render()` scene JSON,
     * rather than re-sending pixel bytes on every frame (see the `images`
     * field's own comment for why re-using the same `ImageData` instance
     * matters for vello's internal upload caching).
     */
    register_image(id: string, rgba: Uint8Array, width: number, height: number): void;
    /**
     * Retained path store (see the `paths` field's doc comment). `coords` is
     * a flat [px,py, hInX,hInY, hOutX,hOutY] × n array — the same
     * RELATIVE-handle convention as SegIn/serP, 6 slots per segment with
     * explicit zeros where the JSON form omits a zero handle. Built through
     * build_bezpath_from_segments so a registered path and an inline one
     * produce byte-identical curves (single source of truth, §3).
     */
    register_path(id: string, coords: Float64Array, closed: boolean): void;
    render(scene_json: string): void;
    /**
     * Phase C6 (export pipeline): renders the scene offscreen and reads the
     * pixels back to the CPU as straight-alpha RGBA8 bytes (width*height*4),
     * ready for `new ImageData(...)` -> canvas -> PNG, or for feeding the
     * existing ffmpeg sidecar frame-by-frame. Doesn't touch the visible
     * surface at all — exporting doesn't flash frames on screen.
     * Async because GPU->CPU buffer mapping is callback-based in WebGPU;
     * bridged to a Rust future with a oneshot channel.
     */
    render_to_pixels(scene_json: string): Promise<Uint8Array>;
    /**
     * Re-configures the surface AND the offscreen render target to a new
     * device-pixel size — needed because the canvas backing this engine is
     * created once at a fixed size (`create_engine`'s width/height); if the
     * app window is resized afterwards without this, the surface keeps
     * presenting at the stale size while the canvas's CSS box (and
     * `screen_to_world`'s caller-supplied scale) grows/shrinks, producing a
     * stretched/blurry canvas AND wrong world-space coordinates for any
     * interception relying on the canvas's current on-screen size (this is
     * what made an intercepted tool's live preview land off-screen after a
     * resize). No-ops if the size hasn't actually changed, since JS calls
     * this on every resize-observer tick regardless.
     */
    resize(width: number, height: number): void;
    /**
     * Retirement is JS-driven (FinalizationRegistry on the stroke dicts) —
     * this side never guesses at lifetimes. `ids_json`: JSON array of keys.
     */
    retire_paths(ids_json: string): void;
    /**
     * Rotates every selected item in-place around `(pivot_x, pivot_y)` by
     * `angle` radians — mirrors selPropsApplyRotate in tools.js.
     */
    rotate_selection(scene_json: string, pivot_x: number, pivot_y: number, angle: number): string;
    /**
     * Scales every selected item in-place around `(anchor_x, anchor_y)` —
     * the anchor is the OPPOSITE corner/edge from whichever handle is being
     * dragged (mirrors selPropsApplyScale in tools.js: dragging the SE
     * handle anchors on NW, etc — the caller picks the anchor, this just
     * applies the transform). Returns the updated scene JSON; the caller
     * (JS) is expected to keep using this returned scene for subsequent
     * render()/hit-test calls, replacing its own copy.
     */
    scale_selection(scene_json: string, anchor_x: number, anchor_y: number, sx: number, sy: number): string;
    /**
     * Screen (canvas pixel) coordinates -> world coordinates, accounting
     * for the current pan/zoom/rotation — the wasm-side equivalent of
     * Paper.js's `view.viewToProject`, needed since raw pointer events now
     * arrive in plain screen space with nothing translating them for free.
     */
    screen_to_world(sx: number, sy: number): Float64Array;
    /**
     * Hit-tests at (x,y) (world space — caller passes the result of
     * `screen_to_world`) and updates the persisted selection: `additive`
     * (shift-click) toggles the hit item in/out of the existing selection;
     * otherwise the selection is replaced by just the hit item, or cleared
     * entirely on a miss. Returns the updated selection as JSON (see
     * `get_selection`).
     */
    select_at(scene_json: string, x: number, y: number, tolerance: number, additive: boolean): string;
    /**
     * Union bounding box (world space) of every currently-selected item,
     * or `null` if nothing is selected — the box the transform gizmo's
     * handles are built from.
     */
    selection_bounds(scene_json: string): string;
    /**
     * `rotation` in radians, pivoting around `(pivot_x, pivot_y)` — pass
     * the artboard center (e.g. canvasW/2, canvasH/2) to match Animate's
     * Rotate Stage tool; pass (0,0) for a plain top-left-anchored zoom/pan.
     */
    set_viewport(pan_x: number, pan_y: number, zoom: number, rotation: number, pivot_x: number, pivot_y: number, effect_zoom: number): void;
}

export function align_pair(a_json: string, b_json: string): string;

export function auto_match(strokes_a_json: string, strokes_b_json: string): string;

/**
 * `op` is one of "unite" | "subtract" | "intersect" | "exclude" (matching
 * the Paper.js method names already used by the JS fallback, so callers
 * don't need to translate between two different vocabularies).
 */
export function boolean_op(op: string, a_json: string, b_json: string): string;

/**
 * Async because WebGPU adapter/device negotiation is inherently async —
 * JS must `await` this once, then reuse the returned handle every frame.
 * Sets up wgpu manually (rather than via vello::util::RenderContext) so we
 * control the backend flag: RenderContext's own default is
 * `wgpu::Backends::PRIMARY`, which on wasm32 means the *native* backends
 * bundled in the wgpu crate — NOT the browser's own WebGPU implementation
 * — and produced a `NoCompatibleDevice` error until this was pinned to
 * `Backends::BROWSER_WEBGPU` explicitly.
 */
export function create_engine(canvas: HTMLCanvasElement, width: number, height: number): Promise<VelloEngine>;

/**
 * Which frame index actually SUPPLIES the strokes shown at `frame_idx` —
 * the frame itself if it's a keyframe or an interpolated (tween) frame,
 * otherwise the nearest earlier keyframe (a "held" frame), or -1 when
 * nothing earlier exists (empty). Mirrors the scan in getEffectiveStrokes;
 * returning the index rather than the strokes keeps this a cheap pure
 * function — JS (or later, the Rust document model) owns the actual
 * stroke arrays and just indexes with the answer.
 */
export function effective_frame_index(frames_json: string, frame_idx: number): number;

/**
 * Bezier oval fitting the (x0,y0)-(x1,y1) bounding box — same kappa-
 * constant 4-curve construction Paper.js's `Path.Ellipse` uses, so an
 * ellipse drawn here looks identical to one drawn today.
 */
export function ellipse_segments(x0: number, y0: number, x1: number, y1: number): string;

/**
 * Erases a round "brush" (or, mid-drag, a capsule swept from the previous
 * sample point) at `(eraserX, eraserY)` out of `item`. Returns the same
 * `[{exterior,holes}, ...]` JSON shape as `boolean_op` (zero, one, or
 * several polygons — erasing the middle of an open stroke splits it into
 * two separate pieces, same as biting a notch out of a filled shape).
 */
export function erase_at_point(json: string): string;

export function fill_find(input_json: string): string;

/**
 * Returns a JSON `{layerIndex,itemIndex,kind}` for the topmost hit item
 * (last layer, last item within it, scanned first — matches draw order:
 * last-drawn is topmost), or the literal string `"null"` for no hit.
 *
 * NOT a drop-in for select-bridge.js's click/marquee hit-testing — that
 * caller does far more than "which item is under the point": locked-layer
 * skip (except a symbol/component layer, which must still hit as one
 * rigid whole), cross-layer active-layer switching when the hit lands on
 * a non-active layer, a component-layer fallback scan
 * (hitTestComponentLayers) with its own double-click-to-enter-symbol
 * timing, and node/handle-level hits (hitTestHandles, for the node-edit
 * tool) that this function has no concept of at all — it only knows
 * fill/stroke containment over a static scene snapshot. Wiring this in
 * would mean re-implementing all of that business logic here too, not
 * just swapping the geometry test. Verified unused as of the 2026-07-13
 * optimization pass (grep across src/js/*.js found zero callers) — no
 * measured slowness in select-bridge.js's current Paper.js hitTest() to
 * justify the port either, per the same pass.
 */
export function hit_test(scene_json: string, x: number, y: number, tolerance: number): string;

export function interp_stroke(json: string): string;

export function line_segments(x0: number, y0: number, x1: number, y1: number): string;

export function rect_segments(x0: number, y0: number, x1: number, y1: number): string;

export function resample_stroke(stroke_json: string, n: number): string;

/**
 * Which internal frame of a component/symbol shows at `main_frame_idx` —
 * mirrors resolveSymbolFrameIdx (including the ping-pong mode added this
 * session) exactly.
 */
export function resolve_symbol_frame(json: string, main_frame_idx: number): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_velloengine_free: (a: number, b: number) => void;
    readonly create_engine: (a: any, b: number, c: number) => any;
    readonly velloengine_clear_paths: (a: number) => void;
    readonly velloengine_clear_selection: (a: number) => void;
    readonly velloengine_get_selection: (a: number) => [number, number, number, number];
    readonly velloengine_gizmo_handles: (a: number, b: number, c: number) => [number, number, number, number];
    readonly velloengine_has_image: (a: number, b: number, c: number) => number;
    readonly velloengine_path_store_size: (a: number) => number;
    readonly velloengine_register_custom_effect: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly velloengine_register_image: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number];
    readonly velloengine_register_path: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly velloengine_render: (a: number, b: number, c: number) => [number, number];
    readonly velloengine_render_to_pixels: (a: number, b: number, c: number) => any;
    readonly velloengine_resize: (a: number, b: number, c: number) => void;
    readonly velloengine_retire_paths: (a: number, b: number, c: number) => [number, number];
    readonly velloengine_rotate_selection: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly velloengine_scale_selection: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
    readonly velloengine_screen_to_world: (a: number, b: number, c: number) => [number, number];
    readonly velloengine_select_at: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
    readonly velloengine_selection_bounds: (a: number, b: number, c: number) => [number, number, number, number];
    readonly velloengine_set_viewport: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly align_pair: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly auto_match: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly resample_stroke: (a: number, b: number, c: number) => [number, number, number, number];
    readonly fill_find: (a: number, b: number) => [number, number, number, number];
    readonly __wbg_strokemodeler_free: (a: number, b: number) => void;
    readonly effective_frame_index: (a: number, b: number, c: number) => [number, number, number];
    readonly ellipse_segments: (a: number, b: number, c: number, d: number) => [number, number];
    readonly line_segments: (a: number, b: number, c: number, d: number) => [number, number];
    readonly rect_segments: (a: number, b: number, c: number, d: number) => [number, number];
    readonly resolve_symbol_frame: (a: number, b: number, c: number) => [number, number, number];
    readonly strokemodeler_down: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly strokemodeler_move: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly strokemodeler_new: (a: number, b: number) => number;
    readonly strokemodeler_up: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly boolean_op: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly erase_at_point: (a: number, b: number) => [number, number, number, number];
    readonly hit_test: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly interp_stroke: (a: number, b: number) => [number, number, number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h4177160f1dac6248: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen__convert__closures_____invoke__h49909fab4bc066b4: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h29bfc5eda1199406: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__h29bfc5eda1199406_2: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
