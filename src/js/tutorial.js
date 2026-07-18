// ---- Interactive tutorial ("Découvrir Nemo") ----
// Flash 4/5-style in-app lessons: a spotlight highlights a REAL UI element,
// a tooltip explains what to do, and the step only advances once the user
// actually did it — either a real click on the target (event delegation)
// or a real state change polled straight off the live app state (global
// `state`/`userLayers`, same objects app.js/tools.js/timeline.js mutate
// directly — no separate event bus exists in this codebase, see CLAUDE.md).
// Polling (not hooking into every internal function) keeps this module
// completely decoupled: it never patches or wraps existing app code, so it
// can't introduce the "consumer forgot about a new item type" bug family
// documented in CLAUDE.md §1 — it only ever READS state, never writes it.
(function () {
  var POLL_MS = 220;

  // ---- Measurement helpers + a step factory that's hard to get wrong ---
  // Bug found live in testing (2026-07-17): a couple of 'state' steps
  // checked an ABSOLUTE condition ("isKeyframe is true", "children.length
  // > 0") instead of a CHANGE from a baseline captured when the step
  // started. Whenever that absolute condition already happened to be true
  // for an unrelated reason (a fresh layer's frame 0 is already a keyframe
  // by definition; the active layer already had content left over from a
  // previous step/module), the step silently passed without the user
  // doing anything. `stateIncreaseStep` is the fix made structural: every
  // step built with it snapshots `measure(win)` in `before()` and only
  // fires once `measure(win)` has genuinely moved past that snapshot by at
  // least `minIncrease` — so a step can never complete on state that
  // predates it. Use this factory for every new "did a count go up" step
  // instead of writing an ad-hoc check by hand.
  function activeLayer(win) {
    var ul = win.userLayers, st = win.state;
    return (ul && st) ? ul[st.activeLayerIdx] : null;
  }
  function activeLayerData(win) {
    var st = win.state;
    return (st && st.layers) ? st.layers[st.activeLayerIdx] : null;
  }
  function measureStrokeCount(win) {
    var l = activeLayer(win);
    return (l && l.children) ? l.children.length : 0;
  }
  function measureLayerCount(win) {
    return (win.state && win.state.layers) ? win.state.layers.length : 0;
  }
  function measureCurrentFrame(win) {
    return (win.state && win.state.currentFrame) || 0;
  }
  function measureKeyframeCount(win) {
    var ld = activeLayerData(win); if (!ld || !ld.frames) return 0;
    var n = 0; for (var i = 0; i < ld.frames.length; i++) if (ld.frames[i] && ld.frames[i].isKeyframe) n++;
    return n;
  }
  function measureInterpolatedCount(win) {
    var ld = activeLayerData(win); if (!ld || !ld.frames) return 0;
    var n = 0; for (var i = 0; i < ld.frames.length; i++) if (ld.frames[i] && ld.frames[i].isInterpolated) n++;
    return n;
  }
  // Generic "did the geometry change at all" fingerprint — covers edits
  // that DON'T add/remove a child (moving a shape, recoloring an existing
  // fill, partially erasing a path leaves the same item with different
  // segments). A raw JSON length is a cheap, good-enough proxy: virtually
  // any real edit (position, color, segment count) changes the serialized
  // length. Never used as the ONLY signal for something that's supposed to
  // create a brand-new item — use measureStrokeCount (stateIncreaseStep)
  // for that instead.
  function measureLayerFingerprint(win) {
    var l = activeLayer(win);
    try { return (l && l.exportJSON) ? String(l.exportJSON({ asString: true })).length : 0; }
    catch (e) { return 0; }
  }
  function measureLayersLength(win) { return measureLayerCount(win); }
  function measureSymbolCount(win) {
    var st = win.state;
    return (st && st.symbols) ? Object.keys(st.symbols).length : 0;
  }
  function measureCameraKeyCount(win) {
    return (win.state && win.state.cameraKeys) ? win.state.cameraKeys.length : 0;
  }
  function measureMotionPositionKeyCount(win) {
    var ld = activeLayerData(win);
    return (ld && ld.motion && ld.motion.position && ld.motion.position.keys) ? ld.motion.position.keys.length : 0;
  }
  function measureTextInputLength(win) {
    var el = win.document.getElementById('text-input');
    return el ? el.value.length : 0;
  }
  function measureFillColor(win) { return (win.state && win.state.fillColor) || ''; }
  function measureStrokeColor(win) { return (win.state && win.state.strokeColor) || ''; }

  function stateIncreaseStep(cfg) {
    var minInc = cfg.minIncrease || 1;
    return {
      type: 'state', target: cfg.target, title: cfg.title, body: cfg.body, hint: cfg.hint || 'À toi de jouer…', pinCorner: cfg.pinCorner,
      before: function (win) { win.__tutBaseline = cfg.measure(win); },
      check: function (win) { return cfg.measure(win) >= win.__tutBaseline + minInc; }
    };
  }

  // For a toggle/boolean rather than a monotonic counter — a fresh project
  // can start with the flag already true (e.g. onionSkin defaults to true,
  // see app.js's initial `state`), so "wait for it to become true" is the
  // exact same premature-pass trap as the counter steps above. Require an
  // actual CHANGE from whatever it was when the step started instead.
  function stateChangedStep(cfg) {
    return {
      type: 'state', target: cfg.target, title: cfg.title, body: cfg.body, hint: cfg.hint || 'À toi de jouer…',
      before: function (win) { win.__tutBaseline = cfg.measure(win); },
      check: function (win) { return cfg.measure(win) !== win.__tutBaseline; }
    };
  }

  // ---- Module content ------------------------------------------------
  // Each step is one of:
  //   {type:'info', title, body}                         — just a "Suivant" button
  //   {type:'click', target, title, body}                 — real click on `target` (CSS selector)
  //   {type:'state', target, title, body, hint, check}    — check(win) polled until true;
  //                                                          `target` (optional) is only used
  //                                                          to aim the spotlight. Prefer
  //                                                          stateIncreaseStep() over writing
  //                                                          this by hand — see comment above.
  var MODULES = [
    {
      id: 'draw',
      category: 'Dessiner',
      icon: '1',
      title: 'Premier trait',
      desc: 'Le pinceau, les couleurs, dessiner une forme',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Bienvenue dans Nemo', body: 'On va dessiner un premier trait ensemble. À chaque étape, fais vraiment le geste demandé — le tutoriel avance tout seul dès que c\'est fait.' },
        { type: 'click', target: '.tool-btn[data-tool="draw"]', title: 'Choisis le Pinceau', body: 'Clique sur l\'outil Pinceau dans la barre de gauche (raccourci B).' },
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Dessine un trait', body: 'Clique-glisse sur le canevas blanc pour tracer un trait, comme au crayon.', hint: 'En attente de ton trait…',
          measure: measureStrokeCount
        }),
        { type: 'click', target: '#stroke-well', title: 'Change la couleur du trait', body: 'Clique le carré de couleur du Trait pour ouvrir le sélecteur, choisis une teinte.' },
        { type: 'info', title: 'Bien joué !', body: 'Tu sais dessiner un trait et changer sa couleur. La suite du chapitre "Dessiner" du guide utilisateur détaille tous les autres outils (plume, formes, gomme…).' }
      ]
    },
    {
      id: 'layers',
      category: 'Calques et animation',
      icon: '2',
      title: 'Calques et images-clés',
      desc: 'Ajouter un calque, poser une keyframe',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Calques et timeline', body: 'Un calque contient une série de frames. Une "keyframe" est une frame où tu as vraiment dessiné quelque chose de nouveau — c\'est ce sur quoi l\'interpolation automatique s\'appuie.' },
        { type: 'click', target: '#btn-al', title: 'Ajoute un calque', body: 'Clique le bouton "+" en bas du panneau Calques, à gauche de la timeline.' },
        stateIncreaseStep({ title: 'Vérification…', body: 'Le nouveau calque doit apparaître dans la liste.', hint: 'Un instant…', measure: measureLayerCount }),
        stateIncreaseStep({
          title: 'Avance de quelques frames', body: 'Clique "Frame suivante" 2 ou 3 fois pour te placer plus loin dans la timeline.', hint: 'En attente…',
          measure: measureCurrentFrame, minIncrease: 2
        }),
        stateIncreaseStep({
          title: 'Insère une image-clé', body: 'Appuie sur la touche F6 de ton clavier pour transformer la frame actuelle en keyframe.', hint: 'En attente de F6…',
          measure: measureKeyframeCount
        }),
        { type: 'info', title: 'Bien joué !', body: 'Tu as un nouveau calque et une keyframe posée. Le chapitre "Calques et timeline" du guide couvre F5/F7, le drag & drop de frames, et le clic-droit sur la timeline.' }
      ]
    },
    {
      id: 'tween',
      category: 'Calques et animation',
      icon: '3',
      title: 'Interpolation automatique',
      desc: 'Deux keyframes, un tween généré tout seul',
      time: '3 min',
      steps: [
        { type: 'info', title: 'Le tween automatique', body: 'Pose deux keyframes avec un dessin différent, puis laisse Nemo générer les frames intermédiaires tout seul. C\'est le cœur de Nemo.' },
        // generateTweens() (tweens.js) only counts a frame as a valid tween
        // anchor when `isKeyframe && strokes.length>0` — a keyframe with
        // NOTHING drawn on it (frame 0 of a fresh layer, by default) does
        // not count, so with only the second keyframe drawn there's still
        // only 1 real anchor and T silently no-ops ("Il faut au moins 2
        // keyframes dessinées" toast). Found live in testing — draw the
        // FIRST keyframe for real before moving on, don't just assume the
        // untouched frame 0 counts as one.
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Dessine une première forme', body: 'Clique-glisse sur le canevas pour dessiner quelque chose sur cette première keyframe.',
          hint: 'En attente de ton trait…', measure: measureStrokeCount
        }),
        stateIncreaseStep({
          title: 'Avance de quelques frames', body: 'Clique "Frame suivante" plusieurs fois (ou glisse le curseur) pour te placer 5 à 10 frames plus loin.', hint: 'En attente…',
          measure: measureCurrentFrame, minIncrease: 5
        }),
        // F6 BEFORE drawing, not after: Nemo only ever saves ce qui est
        // dessiné sur une frame qui EST DÉJÀ une keyframe (ou interpolée) —
        // saveActiveLayerFrame() (app.js) bail out silently sinon
        // (`if(!f.isKeyframe&&!f.isInterpolated)return;`). Dessiner d'abord
        // puis appuyer F6 ensuite semblait marcher à l'écran (Paper.js
        // affiche le trait pendant qu'on dessine) mais se faisait
        // silencieusement effacer au prochain rechargement de frame —
        // trouvé en testant en direct, pas en lisant le code. Poser la
        // keyframe D'ABORD reproduit le vrai flux de travail de Nemo.
        stateIncreaseStep({ title: 'Pose une nouvelle keyframe', body: 'Appuie sur F6 pour créer une nouvelle keyframe ici — c\'est elle que tu vas dessiner.', hint: 'En attente de F6…', measure: measureKeyframeCount }),
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Dessine un second trait, différent du premier', body: 'Même geste qu\'au premier module — mais dessine autre chose, pour que le tween ait quelque chose à interpoler.',
          hint: 'En attente de ton trait…', measure: measureStrokeCount
        }),
        stateIncreaseStep({ title: 'Lance le tween', body: 'Appuie sur la touche T pour interpoler automatiquement entre les deux keyframes.', hint: 'En attente de T…', measure: measureInterpolatedCount }),
        { type: 'info', title: 'Bravo, premier tween !', body: 'Regarde la timeline : les frames entre tes deux keyframes sont maintenant interpolées. Le chapitre "Interpolation automatique" du guide couvre l\'éditeur de courbes et l\'onion skin.' }
      ]
    },
    {
      id: 'onion',
      category: 'Calques et animation',
      icon: '4',
      title: 'Onion skin',
      desc: 'Voir les frames voisines en transparence',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Voir à travers les frames', body: 'L\'onion skin affiche les frames voisines en transparence par-dessus la frame actuelle — pratique pour juger un mouvement sans jouer l\'animation. Il est activé par défaut sur un nouveau projet ; on va vérifier que tu sais où le couper/rallumer.' },
        // onionSkin defaults to true (app.js) on a fresh project — "wait
        // for it to become true" would pass instantly with no click at
        // all. stateChangedStep requires an actual toggle, whichever
        // direction it goes.
        stateChangedStep({
          target: '#btn-os', title: 'Bascule l\'onion skin', body: 'Clique le bouton onion skin dans la timeline (ou la touche O) pour le couper, puis reclique pour le rallumer.', hint: 'En attente…',
          measure: function (win) { return !!(win.state && win.state.onionSkin); }
        }),
        { type: 'click', target: '#btn-nf', title: 'Change de frame', body: 'Clique "Frame suivante" pour voir les frames voisines apparaître en fantôme.' },
        { type: 'info', title: 'Bien joué !', body: 'Le chapitre "Interpolation automatique" du guide détaille les marqueurs de plage et le mode contours seuls.' }
      ]
    },
    {
      id: 'shapes',
      category: 'Dessiner',
      icon: '5',
      title: 'Formes, remplissage et gomme',
      desc: 'Rectangle, pot de peinture, gomme',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Au-delà du pinceau', body: 'Rectangle, Ellipse, Pot de peinture, Gomme — les outils de base pour construire des formes propres plutôt qu\'à main levée.' },
        { type: 'click', target: '.tool-btn[data-tool="rect"]', title: 'Choisis le Rectangle', body: 'Clique sur l\'outil Rectangle dans la barre de gauche (raccourci R).' },
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Dessine un rectangle', body: 'Clique-glisse en diagonale sur le canevas pour tracer un rectangle.',
          hint: 'En attente de ta forme…', measure: measureStrokeCount
        }),
        { type: 'click', target: '.tool-btn[data-tool="fill"]', title: 'Choisis le Pot de peinture', body: 'Clique sur l\'outil Pot de peinture (raccourci G).' },
        { type: 'click', target: '#fill-well', title: 'Change la couleur de fond', body: 'Clique le carré de couleur du Fond pour ouvrir le sélecteur, choisis une autre teinte.' },
        // Rect ships with fillEnabled:true by default (app.js), so the
        // rectangle is ALREADY filled the moment it's drawn — clicking
        // inside it with the bucket hits the "recolor in place" branch
        // (tools.js), not "insert a brand-new filled path". A raw
        // children.length check would never move. The fingerprint catches
        // the recolor either way, whichever branch actually ran.
        stateChangedStep({
          target: '#drawing-canvas', title: 'Remplis le rectangle', body: 'Clique à l\'intérieur du rectangle pour appliquer la nouvelle couleur.',
          hint: 'En attente de ton clic…', measure: measureLayerFingerprint
        }),
        { type: 'click', target: '.tool-btn[data-tool="eraser"]', title: 'Choisis la Gomme', body: 'Clique sur l\'outil Gomme (raccourci E).' },
        stateChangedStep({
          target: '#drawing-canvas', title: 'Efface un morceau du rectangle', body: 'Clique-glisse sur un bord du rectangle pour en effacer une partie.',
          hint: 'En attente de ton geste…', measure: measureLayerFingerprint
        }),
        { type: 'info', title: 'Bien joué !', body: 'La Pipette (I) prélève une couleur existante sur le canevas — pratique pour rester cohérent d\'une frame à l\'autre. Le chapitre "Dessiner" du guide couvre aussi les opérations booléennes (union/soustraction).' }
      ]
    },
    {
      id: 'select',
      category: 'Dessiner',
      icon: '6',
      title: 'Sélection et transformation',
      desc: 'Déplacer et redimensionner une forme',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Revenir sur ce qui existe déjà', body: 'L\'outil Sélection sert à reprendre une forme après coup — la déplacer, la redimensionner, la faire pivoter — sans devoir la redessiner.' },
        { type: 'click', target: '.tool-btn[data-tool="select"]', title: 'Choisis la Sélection', body: 'Clique sur l\'outil Sélection dans la barre de gauche (raccourci V).' },
        stateChangedStep({
          target: '#drawing-canvas', title: 'Déplace une forme', body: 'Clique sur un trait ou une forme dessinée puis fais-la glisser ailleurs sur le canevas.',
          hint: 'En attente de ton geste…', measure: measureLayerFingerprint
        }),
        { type: 'info', title: 'Bien joué !', body: 'Les poignées aux coins redimensionnent, celle au-dessus fait pivoter. Le panneau de droite (Position/Size/Rotate) permet aussi de saisir des valeurs exactes au clavier.' }
      ]
    },
    {
      id: 'motion',
      category: 'Calques et animation',
      icon: '7',
      title: 'Motion — animer une propriété',
      desc: 'Position, rotation, échelle par keyframes',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Animer sans redessiner', body: 'Le mode Motion anime des PROPRIÉTÉS (position, rotation, échelle, opacité) par keyframes, façon After Effects — complémentaire du dessin frame par frame.' },
        { type: 'click', target: '.app-mode-btn[data-mode="motion"]', title: 'Passe en mode Motion', body: 'Clique l\'onglet "Motion" en haut de l\'écran.' },
        // The very first stopwatch icon in the Motion panel is Position —
        // PROPS' own declared order (motion.js) — since the transform-group
        // row for the active layer renders before any per-element rows.
        stateIncreaseStep({
          target: '.motion-stopwatch', title: 'Active l\'animation de Position', body: 'Clique le petit losange à côté de "Position" dans le panneau Motion pour poser une première clé.',
          hint: 'En attente…', measure: measureMotionPositionKeyCount
        }),
        stateIncreaseStep({
          title: 'Avance de quelques frames', body: 'Clique "Frame suivante" plusieurs fois pour te placer plus loin.', hint: 'En attente…',
          measure: measureCurrentFrame, minIncrease: 5
        }),
        stateChangedStep({
          target: '#drawing-canvas', title: 'Déplace le calque', body: 'Fais glisser le calque sur le canevas — une nouvelle clé de Position se crée automatiquement ici, à cette frame.',
          hint: 'En attente de ton geste…', measure: measureMotionPositionKeyCount
        }),
        { type: 'info', title: 'Bien joué !', body: 'Rejoue (Entrée) pour voir le calque bouger entre les deux clés. Un calque avec 2 éléments ou plus devient automatiquement un Component dès qu\'une propriété de calque est keyée — voir le module suivant.' }
      ]
    },
    {
      id: 'component',
      category: 'Organisation',
      icon: '8',
      title: 'Components et StoryBoard',
      desc: 'Réutiliser un calque comme un symbole',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Un calque réutilisable', body: 'Un Component est un calque transformé en symbole réutilisable — comme un symbole Flash/Animate. StoryBoard, le montage nodal de Nemo, ne manipule QUE des Components.' },
        { type: 'click', target: '.app-mode-btn[data-mode="anim2d"]', title: 'Reviens en Animation 2D', body: 'Clique l\'onglet "Animation 2D" en haut de l\'écran.' },
        // Convert-to-component (app.js) early-returns with just a toast on a
        // layer that's ALREADY a component (`if(!ld||ld.symbolId)return`) —
        // e.g. right after the Motion module, whose own exercise can
        // auto-convert the layer. Adding a guaranteed-fresh, never-yet-a-
        // component layer here makes this step work regardless of what a
        // previous module left the project in, instead of silently getting
        // stuck waiting for a click that will never do anything.
        stateIncreaseStep({ target: '#btn-al', title: 'Ajoute un calque neuf', body: 'Clique le bouton "+" en bas du panneau Calques — on part d\'un calque tout neuf pour cet exercice.', hint: 'En attente…', measure: measureLayerCount }),
        stateIncreaseStep({
          target: '#btn-comp', title: 'Convertis le calque en Component', body: 'Clique le bouton losange ◈ en bas du panneau Calques ("Convert layer to component").',
          hint: 'En attente…', measure: measureSymbolCount
        }),
        { type: 'click', target: '.app-mode-btn[data-mode="storyboard"]', title: 'Ouvre le StoryBoard', body: 'Clique l\'onglet "StoryBoard" en haut de l\'écran — c\'est là que les Components se montent en séquence.' },
        { type: 'info', title: 'Bien joué !', body: 'Le chapitre "Components et StoryBoard" du guide couvre les instances (vitesse/offset propres à chacune) et le montage nodal complet.' }
      ]
    },
    {
      id: 'camera',
      category: 'Organisation',
      icon: '9',
      title: 'Caméra',
      desc: 'Cadrage animé (zoom/pan)',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Une caméra virtuelle', body: 'Un calque caméra anime le cadrage (zoom/pan/rotation) par-dessus toute la scène, avec des courbes de Bézier — comme dans TVPaint ou Callipeg.' },
        stateChangedStep({
          target: '#btn-camera', title: 'Ajoute un calque caméra', body: 'Clique le bouton caméra en bas du panneau Calques.', hint: 'En attente…',
          measure: function (win) { return !!(win.state && win.state.cameraLayerOn); }
        }),
        { type: 'info', title: 'Bien joué !', body: 'Le calque caméra sélectionné, glisse un cadre sur le canevas pour poser une clé de cadrage — chaque frame où tu ajustes le cadre en pose une nouvelle. Le chapitre "Caméra, audio et médias" du guide détaille l\'éditeur de courbe dédié.' }
      ]
    },
    {
      id: 'media',
      category: 'Médias',
      icon: '10',
      title: 'Audio et médias',
      desc: 'Importer un son, une image, une vidéo',
      time: '1 min',
      // File-driven features (a real file picker/drag-drop) aren't
      // something this sandboxed tutorial can hand a real file to — these
      // steps stay click-only (real clicks on the real buttons, no state
      // polling) rather than pretending to validate an import that never
      // happens. Still real navigation, just not gated on a file result.
      steps: [
        { type: 'info', title: 'Faire entrer du contenu externe', body: 'Nemo importe de l\'audio, des images (dont des séquences numérotées) et de la vidéo — chacune devient une piste ou un calque animé.' },
        { type: 'click', target: '#btn-audio', title: 'Ouvre l\'import audio', body: 'Clique le bouton note de musique en bas du panneau Calques pour voir le sélecteur de fichier s\'ouvrir.' },
        { type: 'info', title: 'Bien joué !', body: 'Import Image(s)…/Import Video… vivent dans le menu principal. Le chapitre "Caméra, audio et médias" du guide couvre la bibliothèque de médias et la référence vidéo pour la rotoscopie.' }
      ]
    },
    {
      id: 'text',
      category: 'Dessiner',
      icon: '11',
      title: 'Texte et pipette',
      desc: 'Poser du texte, prélever une couleur',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Texte et pipette', body: 'Le texte se pose comme une image (rendu, pas éditable ensuite) — pour du texte permanent façon titrage. La pipette prélève une couleur déjà présente sur le canevas.' },
        // Draw a real colored Path FIRST — the eyedropper only ever reads
        // from `item instanceof Path` (tools.js), never a Raster. Text is
        // rendered as a Raster, so trying to eyedropper the text itself
        // would never do anything, no matter how precisely it's clicked —
        // found live in testing, the step just sat there forever.
        { type: 'click', target: '.tool-btn[data-tool="rect"]', title: 'Choisis le Rectangle', body: 'Clique sur l\'outil Rectangle (raccourci R) — on va se donner une couleur à prélever tout à l\'heure.' },
        // Change the stroke color BEFORE drawing — a fresh project's
        // rectangle would otherwise carry the exact default stroke/fill
        // (#000000/#ff0000), identical to state.strokeColor/fillColor
        // already. The eyedropper step further down measures whether
        // strokeColor CHANGES after picking — picking a color that's
        // already active is indistinguishable from picking nothing at
        // all, so that step would wait forever. Found live in testing.
        { type: 'click', target: '#stroke-well', title: 'Choisis une couleur de trait inhabituelle', body: 'Clique le carré de couleur du Trait et choisis une teinte qui n\'est pas déjà utilisée.' },
        stateIncreaseStep({ target: '#drawing-canvas', title: 'Dessine un rectangle', body: 'Clique-glisse sur le canevas pour tracer un rectangle.', hint: 'En attente de ta forme…', measure: measureStrokeCount }),
        { type: 'click', target: '.tool-btn[data-tool="text"]', title: 'Choisis le Texte', body: 'Clique sur l\'outil Texte dans la barre de gauche.' },
        { type: 'click', target: '#drawing-canvas', title: 'Clique sur le canevas', body: 'Clique où poser le texte — une petite fenêtre de saisie apparaît.' },
        stateIncreaseStep({
          target: '#text-input', title: 'Écris quelque chose', body: 'Tape un mot ou deux dans le champ de texte qui vient de s\'ouvrir.',
          hint: 'En attente de ta saisie…', measure: measureTextInputLength, pinCorner: 'top-right'
        }),
        stateIncreaseStep({ target: '#text-apply', title: 'Valide le texte', body: 'Clique "Apply" (ou Ctrl/Cmd+Entrée) pour poser le texte sur le canevas.', hint: 'En attente…', measure: measureStrokeCount, pinCorner: 'top-right' }),
        { type: 'click', target: '.tool-btn[data-tool="eyedropper"]', title: 'Choisis la Pipette', body: 'Clique sur l\'outil Pipette dans la barre de gauche (raccourci I).' },
        stateChangedStep({
          target: '#drawing-canvas', title: 'Prélève une couleur', body: 'Clique sur le rectangle (pas le texte — la pipette ne lit pas les images) pour reprendre sa couleur comme couleur de trait active.',
          hint: 'En attente de ton clic…', measure: measureStrokeColor
        }),
        { type: 'info', title: 'Bien joué !', body: 'Le chapitre "Dessiner" du guide utilisateur détaille les options de police et de taille du texte.' }
      ]
    },
    {
      id: 'palette',
      category: 'Dessiner',
      icon: '12',
      title: 'Palette de couleurs',
      desc: 'Réutiliser des couleurs déjà choisies',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Une bibliothèque de couleurs', body: 'Le panneau Palette garde des couleurs prêtes à réutiliser — plusieurs palettes nommées, glisser-déposer pour réordonner, clic pour appliquer.' },
        stateChangedStep({
          target: '#palette-grid', title: 'Applique une couleur de palette', body: 'Clique une des pastilles de couleur — elle devient la couleur de Fond active (Shift+clic pour le Trait).',
          hint: 'En attente de ton clic…', measure: measureFillColor
        }),
        { type: 'click', target: '#btn-palette-swap', title: 'Échange Fond et Trait', body: 'Clique le bouton ⇄ pour inverser les couleurs de Fond et de Trait.' },
        { type: 'info', title: 'Bien joué !', body: 'Clic-droit sur une pastille propose "Remplacer dans le calque…" — pratique pour recolorer tous les traits d\'une teinte en une fois. Le "+" au-dessus des palettes en crée une nouvelle.' }
      ]
    },
    {
      id: 'boolean',
      category: 'Dessiner',
      icon: '13',
      title: 'Opérations booléennes',
      desc: 'Fusionner deux formes en une seule',
      time: '2 min',
      steps: [
        { type: 'info', title: 'Combiner des formes', body: 'Union, soustraction, intersection, exclusion — combine plusieurs formes sélectionnées en une seule, plutôt que de redessiner à la main.' },
        { type: 'click', target: '.tool-btn[data-tool="rect"]', title: 'Choisis le Rectangle', body: 'Clique sur l\'outil Rectangle (raccourci R).' },
        stateIncreaseStep({ target: '#drawing-canvas', title: 'Dessine un premier rectangle', body: 'Clique-glisse pour tracer un premier rectangle.', hint: 'En attente…', measure: measureStrokeCount }),
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Dessine un second rectangle, qui chevauche le premier', body: 'Trace un second rectangle qui recouvre partiellement le premier.',
          hint: 'En attente…', measure: measureStrokeCount
        }),
        { type: 'click', target: '.tool-btn[data-tool="select"]', title: 'Choisis la Sélection', body: 'Clique sur l\'outil Sélection (raccourci V).' },
        {
          // A marquee drag has to start on EMPTY canvas — starting on top
          // of a shape moves it instead (see the Sélection module). The
          // two rectangles were drawn in the canvas's upper-left area, so
          // a drag from further down/right stays empty long enough to
          // start a real rubber-band selection.
          type: 'click', target: '#drawing-canvas', title: 'Entoure les deux formes', body: 'Clique-glisse depuis une zone vide du canevas pour entourer les deux rectangles et les sélectionner ensemble.'
        },
        // Union REDUCES the child count (2 shapes -> 1 merged path) —
        // stateIncreaseStep only ever fires on an increase, so negate the
        // count instead of writing a third, near-duplicate factory just
        // for the one decreasing case in this whole file.
        stateIncreaseStep({
          target: '#btn-bool-unite', title: 'Fusionne (Union)', body: 'Clique le bouton Union dans le panneau de droite pour fusionner les deux formes en une seule.',
          hint: 'En attente…', measure: function (win) { return -measureStrokeCount(win); }, minIncrease: 1
        }),
        { type: 'info', title: 'Bien joué !', body: 'Soustraction, Intersection et Exclusion suivent le même principe, juste à côté du bouton Union.' }
      ]
    },
    {
      id: 'settings',
      category: 'Réglages',
      icon: '14',
      title: 'Réglages et raccourcis',
      desc: 'Langue, raccourcis, mises à jour',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Personnaliser l\'app', body: 'Réglages regroupe la langue, ton profil, la collaboration, les raccourcis clavier et les prototypes expérimentaux (Labs).' },
        { type: 'click', target: '#project-tabs-settings', title: 'Ouvre les Réglages', body: 'Clique l\'icône en forme de roue crantée en haut de l\'écran.' },
        stateChangedStep({
          target: '#settings-language', title: 'Change la langue', body: 'Choisis une autre langue dans le menu déroulant — l\'interface change instantanément.',
          hint: 'En attente…', measure: function (win) { var el = win.document.getElementById('settings-language'); return el ? el.value : ''; }
        }),
        { type: 'click', target: '.settings-tab[data-tab="shortcuts"]', title: 'Ouvre l\'onglet Raccourcis', body: 'Clique l\'onglet "Raccourcis" en haut de la fenêtre de Réglages.' },
        { type: 'click', target: '#settings-close', title: 'Ferme les Réglages', body: 'Clique la croix pour refermer la fenêtre.' },
        { type: 'info', title: 'Bien joué !', body: 'Le chapitre "Paramètres de l\'application" du guide couvre aussi Collaboration, Feedback et Labs.' }
      ]
    },
    {
      id: 'history',
      category: 'Réglages',
      icon: '15',
      title: 'Historique de versions',
      desc: 'Revenir à un état antérieur',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Remonter dans le temps', body: 'Nemo prend un instantané automatique toutes les 30 secondes — récupérable même après un crash, pas seulement la dernière session.' },
        // The "Project" section (right panel) starts COLLAPSED by default
        // (.pbdy has the .hid class on load) — #btn-history is 0x0 and
        // unclickable until its header is opened. Found live: the spotlight
        // highlighted nothing, because getBoundingClientRect() on a
        // display:none descendant is legitimately zero.
        { type: 'click', target: '#phdr-project', title: 'Ouvre la section "Project"', body: 'Clique l\'en-tête "Project" dans le panneau de droite pour la déplier.' },
        { type: 'click', target: '#btn-history', title: 'Ouvre l\'historique', body: 'Clique "Historique…" dans le panneau de droite (section Projet).' },
        { type: 'info', title: 'Bien joué !', body: 'Restaurer un ancien instantané prend d\'abord un instantané de l\'état actuel — l\'opération reste donc elle-même annulable.' }
      ]
    },
    {
      id: 'perspective',
      category: 'Dessiner',
      icon: '16',
      title: 'Guide de perspective',
      desc: 'Placer des points de fuite',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Dessiner en perspective', body: 'Le guide de perspective affiche une grille de points de fuite et aimante l\'outil Ligne dessus, depuis n\'importe quel outil.' },
        { type: 'click', target: '.tool-btn[data-tool="perspective"]', title: 'Choisis l\'outil Perspective', body: 'Clique sur l\'outil Perspective dans la barre de gauche.' },
        {
          type: 'state', target: '#phdr-perspective', title: 'Ouvre la section "Perspective Guide"', body: 'Clique l\'en-tête "Perspective Guide" dans le panneau de droite pour la déplier, si besoin.',
          hint: 'En attente…',
          check: function (win) { var el = win.document.getElementById('p-persp-on'); return !!(el && el.getBoundingClientRect().height > 0); }
        },
        stateChangedStep({
          target: '#p-persp-on', title: 'Active le guide', body: 'Coche "Enabled" pour afficher la grille de points de fuite sur le canevas.', hint: 'En attente…',
          measure: function (win) { var el = win.document.getElementById('p-persp-on'); return el ? el.checked : false; }
        }),
        { type: 'click', target: '.tool-btn[data-tool="line"]', title: 'Choisis la Ligne', body: 'Clique sur l\'outil Ligne (raccourci U) — il va s\'aimanter aux points de fuite.' },
        stateIncreaseStep({
          target: '#drawing-canvas', title: 'Trace une ligne', body: 'Clique-glisse sur le canevas — la ligne s\'oriente vers un point de fuite.',
          hint: 'En attente de ton trait…', measure: measureStrokeCount
        }),
        { type: 'info', title: 'Bien joué !', body: '1/2/3 points de fuite selon le mode choisi. "Lock vanishing points" évite de les déplacer par erreur en dessinant près d\'eux.' }
      ]
    },
    {
      id: 'collab',
      category: 'Réglages',
      icon: '17',
      title: 'Profil et travail d\'équipe',
      desc: 'Se présenter, dossier partagé',
      time: '1 min',
      steps: [
        { type: 'info', title: 'Travailler à plusieurs', body: 'Ton profil (nom + couleur) distingue tes traits de ceux d\'un autre profil qui corrige ton travail. La Sync équipe publie/récupère les modifs via un dossier partagé (Drive, kDrive…), sans temps réel.' },
        { type: 'click', target: '#project-tabs-settings', title: 'Ouvre les Réglages', body: 'Clique l\'icône en forme de roue crantée en haut de l\'écran.' },
        stateIncreaseStep({
          target: '#profile-name', title: 'Indique ton nom', body: 'Tape ton nom dans le champ "Nom" de la section Profil.', hint: 'En attente de ta saisie…',
          measure: function (win) { var el = win.document.getElementById('profile-name'); return el ? el.value.length : 0; }
        }),
        { type: 'click', target: '.settings-tab[data-tab="collab"]', title: 'Ouvre l\'onglet Collaboration', body: 'Clique l\'onglet "Collaboration" en haut de la fenêtre de Réglages.' },
        { type: 'click', target: '#sync-choose-folder', title: 'Regarde le bouton "Choisir…"', body: 'Clique "Choisir…" pour voir comment on désigne un dossier partagé (nécessite l\'app desktop — un simple message s\'affiche ici en preview navigateur).' },
        { type: 'click', target: '#settings-close', title: 'Ferme les Réglages', body: 'Clique la croix pour refermer la fenêtre.' },
        { type: 'info', title: 'Bien joué !', body: 'Le chapitre "Travailler à plusieurs" du guide couvre aussi les corrections à Accepter/Rejeter et le feedback d\'équipe.' }
      ]
    }
  ];

  // ---- Progress persistence ------------------------------------------
  function loadDone() { try { return JSON.parse(localStorage.getItem('nemo-tutorial-done') || '[]'); } catch (e) { return []; } }
  function markDone(id) {
    try {
      var d = loadDone();
      if (d.indexOf(id) === -1) { d.push(id); localStorage.setItem('nemo-tutorial-done', JSON.stringify(d)); }
    } catch (e) {}
  }

  // ---- Runtime state ----------------------------------------------------
  var active = null; // {module, stepIdx, pollTimer, clickHandler}

  function $(sel) { return document.querySelector(sel); }

  function ensureDom() {
    if ($('#tut-spotlight')) return;
    var sp = document.createElement('div'); sp.id = 'tut-spotlight'; document.body.appendChild(sp);
    var tt = document.createElement('div'); tt.id = 'tut-tooltip'; document.body.appendChild(tt);
  }

  function stopStepListeners() {
    if (active && active.pollTimer) { clearInterval(active.pollTimer); active.pollTimer = null; }
    if (active && active.clickHandler) { document.removeEventListener('click', active.clickHandler, true); active.clickHandler = null; }
  }

  function positionFor(target) {
    var el = target ? $(target) : null;
    if (!el) return null;
    var r = el.getBoundingClientRect();
    return r;
  }

  function renderStep() {
    stopStepListeners();
    var mod = active.module, step = mod.steps[active.stepIdx];
    if (step.before) { try { step.before(window); } catch (e) {} }

    var sp = $('#tut-spotlight'), tt = $('#tut-tooltip');
    var rect = positionFor(step.target);

    if (rect) {
      sp.classList.remove('no-target');
      sp.style.top = (rect.top - 6) + 'px';
      sp.style.left = (rect.left - 6) + 'px';
      sp.style.width = (rect.width + 12) + 'px';
      sp.style.height = (rect.height + 12) + 'px';
    } else {
      sp.classList.add('no-target');
      sp.style.top = '40%'; sp.style.left = '50%'; sp.style.width = '0px'; sp.style.height = '0px';
    }
    sp.classList.add('on');

    var isLast = active.stepIdx === mod.steps.length - 1;
    tt.innerHTML =
      '<button class="tut-close" title="Quitter le tutoriel">&times;</button>' +
      '<div class="tut-progress">' + mod.title + ' — étape ' + (active.stepIdx + 1) + '/' + mod.steps.length + '</div>' +
      '<div class="tut-title">' + step.title + '</div>' +
      '<div class="tut-body">' + step.body + '</div>' +
      (step.type !== 'info' ? '<div class="tut-hint"><span class="tut-dot"></span>' + (step.hint || 'À toi de jouer…') + '</div>' : '') +
      '<div class="tut-actions">' +
      '<button class="tut-skip">Passer ce module</button>' +
      (step.type === 'info' ? '<button class="tut-next">' + (isLast ? 'Terminer' : 'Suivant') + '</button>' : '') +
      '</div>';

    // Position the tooltip near the spotlight (or centered if none)
    tt.classList.add('on');
    requestAnimationFrame(function () {
      var tw = tt.offsetWidth, th = tt.offsetHeight;
      var top, left;
      // Some app popovers (the text tool's input box) open AT the click
      // point, which can land anywhere on the canvas — the normal
      // "position near the spotlighted rect" logic then has no reliable
      // free side and can end up overlapping the very field the step
      // asks the user to type into (found live: tooltip fully covering
      // #text-input, blocking it since the tooltip sits on top and has
      // pointer-events:auto). pinCorner sidesteps the guesswork for a
      // step like that — always a fixed, known-clear corner.
      if (step.pinCorner === 'top-right') {
        top = 16; left = window.innerWidth - tw - 16;
      } else if (rect) {
        var spaceBelow = window.innerHeight - rect.bottom;
        if (spaceBelow > th + 24) { top = rect.bottom + 16; } else { top = Math.max(12, rect.top - th - 16); }
        left = Math.min(window.innerWidth - tw - 16, Math.max(16, rect.left));
      } else {
        top = window.innerHeight / 2 - th / 2;
        left = window.innerWidth / 2 - tw / 2;
      }
      tt.style.top = top + 'px'; tt.style.left = left + 'px';
    });

    tt.querySelector('.tut-close').addEventListener('click', stopTutorial);
    tt.querySelector('.tut-skip').addEventListener('click', function () { finishModule(false); });
    var nextBtn = tt.querySelector('.tut-next');
    if (nextBtn) nextBtn.addEventListener('click', function () { advance(); });

    if (step.type === 'click') {
      active.clickHandler = function (e) {
        var t = e.target.closest && e.target.closest(step.target);
        if (t) advance();
      };
      document.addEventListener('click', active.clickHandler, true);
    } else if (step.type === 'state') {
      active.pollTimer = setInterval(function () {
        try { if (step.check(window)) advance(); } catch (e) {}
      }, POLL_MS);
    }
  }

  function advance() {
    if (!active) return;
    var mod = active.module;
    if (active.stepIdx >= mod.steps.length - 1) { finishModule(true); return; }
    active.stepIdx++;
    renderStep();
  }

  function finishModule(completed) {
    stopStepListeners();
    var mod = active && active.module;
    active = null;
    var sp = $('#tut-spotlight'), tt = $('#tut-tooltip');
    if (sp) sp.classList.remove('on');
    if (tt) tt.classList.remove('on');
    if (mod && completed) {
      markDone(mod.id);
      if (window.showToast) showToast('Module "' + mod.title + '" terminé ✓');
    }
  }

  function stopTutorial() { finishModule(false); }

  function startModule(id) {
    var mod = MODULES.filter(function (m) { return m.id === id; })[0];
    if (!mod) return;
    // The lessons target real toolbar/canvas elements — those are only
    // reachable once a project is open (the start screen sits on top and
    // intercepts clicks, even though the editor DOM already exists
    // underneath). Launching a lesson straight from the start screen's own
    // row must not dead-end there, so spin up a default blank project first.
    var startScreen = document.getElementById('start-screen');
    if (startScreen && !startScreen.classList.contains('hid') && window.SMProject && window.SMProject.newProject) {
      window.SMProject.newProject({ w: 1920, h: 1080, fps: 24, name: 'Tutoriel' });
    }
    ensureDom();
    closeLauncher();
    active = { module: mod, stepIdx: 0 };
    renderStep();
  }

  // ---- Module launcher modal -------------------------------------------
  function ensureLauncher() {
    if ($('#tut-launcher')) return $('#tut-launcher');
    var modal = document.createElement('div');
    modal.id = 'tut-launcher';
    modal.className = 'modal-overlay';
    modal.style.display = 'none';
    modal.innerHTML =
      '<div class="modal-box">' +
      '<div class="modal-hdr"><span>Découvrir Nemo</span> <button class="modal-x" id="tut-launcher-close">&times;</button></div>' +
      '<div class="modal-bdy">' +
      '<div style="font-size:11px;color:var(--text-dim);margin-bottom:12px">Des mini-leçons pas à pas, directement dans l\'app — comme les tutoriels intégrés de Flash. Choisis un module ; tu peux quitter à tout moment.</div>' +
      '<div class="tut-mod-list" id="tut-mod-list"></div>' +
      '</div></div>';
    document.body.appendChild(modal);
    modal.querySelector('#tut-launcher-close').addEventListener('click', closeLauncher);
    modal.addEventListener('mousedown', function (e) { if (e.target === modal) closeLauncher(); });
    return modal;
  }

  // Categories render in this fixed order (not alphabetical, not
  // MODULES-array order) — roughly the order someone actually learning
  // Nemo would want: draw first, then animate, then organize/output.
  var CATEGORY_ORDER = ['Dessiner', 'Calques et animation', 'Organisation', 'Médias', 'Réglages'];

  // Accordion open/closed state, keyed by category name — lives for the
  // whole session (module-level, not per openLauncher() call) so
  // re-opening the launcher remembers what you had open, same as any
  // normal accordion UI. Nothing open on first run: with 15+ modules
  // across 5 categories, showing everything at once defeats the point of
  // grouping them in the first place.
  var openCats = {};

  function renderLauncherList() {
    var list = $('#tut-mod-list'); if (!list) return;
    var done = loadDone();
    list.innerHTML = '';
    var byCat = {};
    MODULES.forEach(function (m) {
      var cat = m.category || 'Autres';
      (byCat[cat] || (byCat[cat] = [])).push(m);
    });
    var cats = CATEGORY_ORDER.filter(function (c) { return byCat[c]; })
      .concat(Object.keys(byCat).filter(function (c) { return CATEGORY_ORDER.indexOf(c) === -1; }));
    cats.forEach(function (cat) {
      var mods = byCat[cat];
      var doneCount = mods.filter(function (m) { return done.indexOf(m.id) !== -1; }).length;
      var isOpen = !!openCats[cat];

      var hdr = document.createElement('button');
      hdr.className = 'tut-cat-hdr' + (isOpen ? ' open' : '');
      hdr.type = 'button';
      hdr.innerHTML =
        '<span class="tut-cat-chevron">' + chevronSvg() + '</span>' +
        '<span class="tut-cat-name">' + cat + '</span>' +
        '<span class="tut-cat-count">' + doneCount + '/' + mods.length + '</span>';
      var body = document.createElement('div');
      body.className = 'tut-cat-body' + (isOpen ? '' : ' collapsed');
      var inner = document.createElement('div');
      inner.className = 'tut-cat-inner';
      body.appendChild(inner);
      mods.forEach(function (m) {
        var isDone = done.indexOf(m.id) !== -1;
        var btn = document.createElement('button');
        btn.className = 'tut-mod' + (isDone ? ' done' : '');
        btn.innerHTML =
          '<span class="tut-mod-ico">' + (isDone ? '✓' : m.icon) + '</span>' +
          '<span class="tut-mod-body">' +
          '<span class="tut-mod-title">' + m.title + '</span>' +
          '<span class="tut-mod-desc">' + m.desc + '</span>' +
          '</span>' +
          '<span class="tut-mod-time">' + m.time + '</span>';
        btn.addEventListener('click', function () { startModule(m.id); });
        inner.appendChild(btn);
      });
      hdr.addEventListener('click', function () {
        openCats[cat] = !openCats[cat];
        hdr.classList.toggle('open', openCats[cat]);
        body.classList.toggle('collapsed', !openCats[cat]);
      });
      list.appendChild(hdr);
      list.appendChild(body);
    });
  }

  function chevronSvg() {
    return '<svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6"/></svg>';
  }

  function openLauncher() {
    ensureLauncher();
    renderLauncherList();
    $('#tut-launcher').style.display = 'flex';
  }
  function closeLauncher() { var m = $('#tut-launcher'); if (m) m.style.display = 'none'; }

  // ---- Entry point wiring ----------------------------------------------
  document.addEventListener('DOMContentLoaded', function () {
    ['btn-open-tutorial', 'btn-open-tutorial-topbar'].forEach(function (id) {
      var btn = document.getElementById(id);
      if (btn) btn.addEventListener('click', openLauncher);
    });
    document.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && active) stopTutorial();
    });
  });

  window.SMTutorial = { open: openLauncher, start: startModule, stop: stopTutorial, MODULES: MODULES };
})();
