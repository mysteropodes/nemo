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

  function stateIncreaseStep(cfg) {
    var minInc = cfg.minIncrease || 1;
    return {
      type: 'state', target: cfg.target, title: cfg.title, body: cfg.body, hint: cfg.hint || 'À toi de jouer…',
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
      if (rect) {
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

  function renderLauncherList() {
    var list = $('#tut-mod-list'); if (!list) return;
    var done = loadDone();
    list.innerHTML = '';
    MODULES.forEach(function (m) {
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
      list.appendChild(btn);
    });
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
