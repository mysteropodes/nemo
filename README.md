# Nemo

**Open-source motion design and 2D animation, in one app.** A full keyframe motion editor —
editable easing curves, parenting, 3D layers, mograph, rigging, GPU effects — sharing the
same document as a frame-by-frame vector drawing tool with automatic inbetweening. On the
desktop or in your browser.

[![License: GPL v3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)
![Status: alpha](https://img.shields.io/badge/status-alpha-orange)
![Version](https://img.shields.io/badge/version-0.7.0--alpha.1-lightgrey)

<!-- TODO: screenshot or short demo GIF goes here -->

---

## Why Nemo exists

Frame-by-frame animation is well served by open source — Krita, OpenToonz and Blender's
Grease Pencil are all genuinely good tools. **Motion design is not.** If you need to
keyframe, ease, parent, rig and composite the way a motion designer actually works, the
free and open options are unmaintained, narrow single-purpose exporters, or built for a
different job entirely. So the professional standard stays proprietary and
subscription-only, and everyone who can't justify a subscription — students, small
studios, anyone in a place where it simply isn't an option — is shut out of the craft.

Nemo exists to close that gap: motion tooling that doesn't apologise for being free,
living in the same document as a real drawing tool, under a license that guarantees it
stays that way.

## Status: early alpha

Nemo is usable and under daily development, but it is **not stable yet**. Expect bugs,
rough edges, and occasional breaking changes to the file format. Please don't put an
important production on it without keeping backups.

If you try it, [bug reports and ideas](https://github.com/mysteropodes/nemo/issues) are
genuinely the most useful thing you can send — there's also a Comment button inside the
app that files an issue for you, screenshot attached.

## Try it

**In the browser** — no install, requires a WebGPU-capable browser (recent Chrome, Edge,
or Safari 26+):

> https://nemomotion.org

**On the desktop** — build from source (see [Building](#building) below). Packaged
releases are not published yet.

## What's in it today

Nemo is built around two connected modules. They're not separate editors — they're two
views of the same document.

### Animation 2D — drawing and inbetweening

- Vector brush with pressure support, stabilizer, and customizable tips
- Eraser, fill (with linked-fill regeneration), shape and boolean tools
- **Automatic inbetweening** — set two keyframes, Nemo generates the frames between,
  with feature-aware stroke matching rather than naive point interpolation
- Onion skinning, light table, X-sheet, flip/roll playback
- Bitmap brush layer with seeded dab stamping (texture stays put across generated frames)
- Layers with blend modes, groups, and components
- Track mattes — alpha or luma, stackable, with a keyframable on/off period

### Motion — keyframe animation

- Position / Anchor / Rotation / Scale / Opacity keyframes on any layer
- Editable easing curves per key pair, motion paths, and a graph editor
- Deform an imported image with an editable mesh, animatable per vertex
- On-canvas joystick and slider widgets to drive a rig (never rendered)
- Parenting, including weighted blending across several parents at once
- Expressions on any property, with a code editor that splits the canvas
- 3D layers with per-vertex projection, camera, and a transform gizmo
- Mograph duplicator (grid / radial / along-path, with stagger and per-property effectors)
- Rig tool — bones drawn with the pen, tangent-driven rotation, IK

### Effects, import/export, and more

- GPU effects library (WGSL shaders) plus built-in blur, color, distort, and stylize effects
- Export to **MP4, MOV, WebM, GIF**, and JSON; PNG sequences; OCA export
- Import PSD files, ABR brushes, images, video, and audio
- Camera export as a `.jsx` script, Lottie preview
- **Scripting API** (`nemo.*`) with HTML panels and plugins
- **Labs** — ~35 experimental tools behind a panel (lipsync assistant, speed lines,
  screentone, French curve, predictive stroke, pose library, and more)
- Kitsu integration for production tracking *(early, not yet verified against a
  real production Kitsu instance — treat as experimental for now)*
- Interactive in-app tutorial, 25 modules, in **English, French, Japanese, and Spanish**

## The bigger goal

Nemo aims to eventually cover the whole broadcast production chain in one application:
pre-production, storyboard, editing, sound design, 2D animation, motion, and compositing.

Today only **Animation 2D** and **Motion** really exist, and that's on purpose — a tool
that's useful and solid on a narrow scope beats an ambitious suite that's fragile
everywhere. StoryBoard is started but marked in-dev in the UI. Everything else is future
work, and which piece comes next will be driven by what beta feedback actually asks for.

## Building

```bash
npm install
npm run dev      # desktop build (Tauri) with hot reload
npm run serve    # browser-only preview, no Tauri
```

Changes to the Rust/wasm engine under `geometry-wasm/` need a rebuild before they show up
in either mode:

```bash
cd geometry-wasm && wasm-pack build --target web --out-dir ../src/wasm
```

Same for the image vectorizer (`vectorize-wasm/`, sharing its actual tracing logic with
`vectorize-core/` — see that crate's own doc comment) — loaded lazily, only the first time
the "Vectorize Image" dialog opens, not on every app boot:

```bash
cd vectorize-wasm && wasm-pack build --target web --out-dir ../src/wasm-vectorize
```

Run the tests with `npm test` (Node) and `cargo test` (inside `geometry-wasm/`,
`vectorize-core/`, or `src-tauri/`).

## How it's built

Nemo is a hybrid: **Paper.js** holds the document model (the source of truth), and a
**stateless Rust/WebGPU renderer** (`geometry-wasm/`, built on vello) does the drawing,
bridged through `src/js/engine-bridge.js`. The desktop app is Tauri v2. There is no
bundler — `src/` is the site root as-is, plain `<script>` tags.

The split is deliberate: **Rust for the parts that must not wobble** (geometry, rendering),
**JavaScript for everything else** (UI, tools, bridges) so that iteration stays fast.

### On AI-assisted development

Most of this codebase was written with the help of an AI assistant, and that's not hidden
or downplayed — it's the bet. It lowers the barrier to contributing, including for people
who code *with* AI help rather than as seasoned Rust/JS engineers.

The guardrail is [`CLAUDE.md`](CLAUDE.md): a long, blunt engineering guide documenting the
project's invariants and post-mortems of bugs that have already bitten this codebase more
than once (the "new item type handled in one consumer but not the others" family, in
particular). It was written to be read by an AI assistant, but every rule in it applies
just as much to a human — **read it before touching the scene/save pipeline, the render
engine, or Motion.**

## Contributing

Every kind of contribution counts equally here:

- **Feedback** — file an [issue](https://github.com/mysteropodes/nemo/issues), or use the
  Comment button inside the app
- **Code** — with or without AI assistance, both welcome. See [CONTRIBUTING.md](CONTRIBUTING.md)
- **Plugins** — the scripting API exists precisely as an extension surface
- **Non-code help** — issue triage, documentation, tutorials, translation (the app already
  ships in fr/en/ja/es)
- **Money** — entirely optional, see below

There's no CLA. By opening a PR you agree your contribution ships under the project's
license.

## Privacy

Nemo doesn't collect data. No analytics, no telemetry, no tracking — none is
built in, none is planned. Your projects stay on your machine (or in your
browser's local storage, for the web build); nothing about what you draw or
how you use the app is ever sent anywhere.

The app does make network requests, but only for things you explicitly
trigger: checking for app updates and submitting feedback (both talk to
GitHub), and Kitsu production sync if you configure it. Nothing runs in the
background, and nothing you didn't ask for leaves your machine.

## Support

Nemo is free, and it stays free. There is no paid tier, no pro edition, and no feature that
distinguishes a donor from anyone else — that would sit badly with the GPL spirit the
project chose deliberately.

If you want to help fund the work anyway: [ko-fi.com/mysteropodes](https://ko-fi.com/mysteropodes)

## License

[GPL-3.0-or-later](LICENSE) — the same license family as Blender, GIMP, Inkscape, and
Krita. Forks and commercial use are allowed; any distributed modification stays open.

Third-party components and the reasoning behind how they're bundled (notably ffmpeg,
rebuilt without GPL or patent-encumbered codecs) are documented in
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
