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

  // ---- i18n: tutorial content translations ----------------------------
  // Keyed on the exact French source string (the MODULES array below stays
  // 100% French, already tested) rather than on generated keys — translation
  // is a pure lookup applied only at render time, so it never touches step
  // logic. tt() falls back to the French input unchanged if the current
  // language is fr, or if a string has no entry yet for that language.
  var TUT_TR = { en: {}, es: {}, ja: {} };
  (function (rows) {
    rows.forEach(function (r) {
      TUT_TR.en[r[0]] = r[1]; TUT_TR.es[r[0]] = r[2]; TUT_TR.ja[r[0]] = r[3];
    });
  })([
    ["Dessiner", "Draw", "Dibujar", "描く"],
    ["Calques et animation", "Layers & Animation", "Capas y animación", "レイヤーとアニメーション"],
    ["Organisation", "Organization", "Organización", "構成"],
    ["Médias", "Media", "Medios", "メディア"],
    ["Réglages", "Settings", "Ajustes", "設定"],
    ["1 min", "1 min", "1 min", "1分"],
    ["2 min", "2 min", "2 min", "2分"],
    ["3 min", "3 min", "3 min", "3分"],
    ["Le pinceau, les couleurs, dessiner une forme", "The brush, colors, drawing a shape", "El pincel, los colores, dibujar una forma", "ブラシ、色、形を描く"],
    ["Ajouter un calque, poser une keyframe", "Add a layer, set a keyframe", "Añadir una capa, colocar un fotograma clave", "レイヤーを追加し、キーフレームを打つ"],
    ["Deux keyframes, un tween généré tout seul", "Two keyframes, a tween generated automatically", "Dos fotogramas clave, un tween generado automáticamente", "2つのキーフレームから自動でトゥイーン生成"],
    ["Voir les frames voisines en transparence", "See neighboring frames in transparency", "Ver los fotogramas vecinos en transparencia", "前後のフレームを半透明で見る"],
    ["Rectangle, pot de peinture, gomme", "Rectangle, paint bucket, eraser", "Rectángulo, bote de pintura, borrador", "長方形、塗りつぶし、消しゴム"],
    ["Déplacer et redimensionner une forme", "Move and resize a shape", "Mover y redimensionar una forma", "形を移動・リサイズする"],
    ["Position, rotation, échelle par keyframes", "Position, rotation, scale by keyframes", "Posición, rotación, escala por fotogramas clave", "キーフレームで位置・回転・拡大縮小"],
    ["Réutiliser un calque comme un symbole", "Reuse a layer as a symbol", "Reutilizar una capa como símbolo", "レイヤーをシンボルとして再利用"],
    ["Cadrage animé (zoom/pan)", "Animated framing (zoom/pan)", "Encuadre animado (zoom/paneo)", "アニメーションするカメラワーク（ズーム/パン）"],
    ["Importer un son, une image, une vidéo", "Import a sound, an image, a video", "Importar un sonido, una imagen, un vídeo", "音声・画像・動画を読み込む"],
    ["Poser du texte, prélever une couleur", "Place text, pick up a color", "Colocar texto, tomar un color", "テキストを配置し、色を採取する"],
    ["Réutiliser des couleurs déjà choisies", "Reuse already-chosen colors", "Reutilizar colores ya elegidos", "選んだ色を再利用する"],
    ["Fusionner deux formes en une seule", "Merge two shapes into one", "Fusionar dos formas en una sola", "2つの形をひとつに合成する"],
    ["Langue, raccourcis, mises à jour", "Language, shortcuts, updates", "Idioma, atajos, actualizaciones", "言語、ショートカット、アップデート"],
    ["Revenir à un état antérieur", "Go back to an earlier state", "Volver a un estado anterior", "以前の状態に戻る"],
    ["Placer des points de fuite", "Place vanishing points", "Colocar puntos de fuga", "消失点を配置する"],
    ["Se présenter, dossier partagé", "Introduce yourself, shared folder", "Presentarte, carpeta compartida", "プロフィール設定、共有フォルダ"],
    ["Ouvrir la fenêtre d'export, choisir un format", "Open the export window, choose a format", "Abrir la ventana de exportación, elegir un formato", "エクスポートウィンドウを開き、形式を選ぶ"],
    ["Active l'animation de Position", "Enable Position animation", "Activa la animación de Posición", "「位置」のアニメーションを有効化"],
    ["Active le guide", "Enable the guide", "Activa la guía", "ガイドを有効化"],
    ["Ajoute un calque", "Add a layer", "Añade una capa", "レイヤーを追加"],
    ["Ajoute un calque caméra", "Add a camera layer", "Añade una capa de cámara", "カメラレイヤーを追加"],
    ["Ajoute un calque neuf", "Add a fresh layer", "Añade una capa nueva", "新しいレイヤーを追加"],
    ["Animer sans redessiner", "Animate without redrawing", "Animar sin volver a dibujar", "描き直さずにアニメーションする"],
    ["Applique une couleur de palette", "Apply a palette color", "Aplica un color de la paleta", "パレットの色を適用"],
    ["Au-delà du pinceau", "Beyond the brush", "Más allá del pincel", "ブラシだけじゃない"],
    ["Audio et médias", "Audio and media", "Audio y medios", "音声とメディア"],
    ["Avance de quelques frames", "Move forward a few frames", "Avanza unos fotogramas", "数フレーム先へ進む"],
    ["Bascule l'onion skin", "Toggle onion skin", "Alterna el onion skin", "オニオンスキンを切り替え"],
    ["Bien joué !", "Well done!", "¡Bien hecho!", "よくできました！"],
    ["Bienvenue dans Nemo", "Welcome to Nemo", "Bienvenido a Nemo", "Nemoへようこそ"],
    ["Bravo, premier tween !", "Great, first tween!", "¡Bravo, primer tween!", "お見事、最初のトゥイーン！"],
    ["Calques et images-clés", "Layers and keyframes", "Capas y fotogramas clave", "レイヤーとキーフレーム"],
    ["Calques et timeline", "Layers and timeline", "Capas y línea de tiempo", "レイヤーとタイムライン"],
    ["Caméra", "Camera", "Cámara", "カメラ"],
    ["Change de frame", "Change frame", "Cambia de fotograma", "フレームを切り替える"],
    ["Change la couleur de fond", "Change the fill color", "Cambia el color de relleno", "塗りの色を変更"],
    ["Change la couleur du trait", "Change the stroke color", "Cambia el color de trazo", "線の色を変更"],
    ["Change la langue", "Change the language", "Cambia el idioma", "言語を変更"],
    ["Choisis l'outil Perspective", "Choose the Perspective tool", "Elige la herramienta Perspectiva", "パースツールを選択"],
    ["Choisis la Gomme", "Choose the Eraser", "Elige el Borrador", "消しゴムを選択"],
    ["Choisis la Ligne", "Choose the Line", "Elige la Línea", "直線ツールを選択"],
    ["Choisis la Pipette", "Choose the Eyedropper", "Elige el Cuentagotas", "スポイトを選択"],
    ["Choisis la Sélection", "Choose Select", "Elige Selección", "選択ツールを選択"],
    ["Choisis la plage à exporter", "Choose the range to export", "Elige el rango a exportar", "書き出す範囲を選ぶ"],
    ["Choisis le Pinceau", "Choose the Brush", "Elige el Pincel", "ブラシを選択"],
    ["Choisis le Pot de peinture", "Choose the Paint Bucket", "Elige el Bote de pintura", "塗りつぶしツールを選択"],
    ["Choisis le Rectangle", "Choose the Rectangle", "Elige el Rectángulo", "長方形ツールを選択"],
    ["Choisis le Texte", "Choose Text", "Elige Texto", "テキストツールを選択"],
    ["Choisis un format", "Choose a format", "Elige un formato", "形式を選ぶ"],
    ["Choisis une couleur de trait inhabituelle", "Choose an unusual stroke color", "Elige un color de trazo poco habitual", "いつもと違う線の色を選ぶ"],
    ["Clique sur le canevas", "Click on the canvas", "Haz clic en el lienzo", "キャンバスをクリック"],
    ["Combiner des formes", "Combine shapes", "Combinar formas", "形を組み合わせる"],
    ["Components et StoryBoard", "Components and StoryBoard", "Componentes y StoryBoard", "コンポーネントとストーリーボード"],
    ["Convertis le calque en Component", "Convert the layer to a Component", "Convierte la capa en Componente", "レイヤーをコンポーネントに変換"],
    ["Dessine un premier rectangle", "Draw a first rectangle", "Dibuja un primer rectángulo", "最初の長方形を描く"],
    ["Dessine un rectangle", "Draw a rectangle", "Dibuja un rectángulo", "長方形を描く"],
    ["Dessine un second rectangle, qui chevauche le premier", "Draw a second rectangle, overlapping the first", "Dibuja un segundo rectángulo que se superponga al primero", "2つ目の長方形を、最初のものと重なるように描く"],
    ["Dessine un second trait, différent du premier", "Draw a second stroke, different from the first", "Dibuja un segundo trazo, distinto del primero", "最初と違う2本目の線を描く"],
    ["Dessine un trait", "Draw a stroke", "Dibuja un trazo", "線を描く"],
    ["Dessine une première forme", "Draw a first shape", "Dibuja una primera forma", "最初の形を描く"],
    ["Dessiner en perspective", "Drawing in perspective", "Dibujar en perspectiva", "パースをつけて描く"],
    ["Déplace le calque", "Move the layer", "Mueve la capa", "レイヤーを移動"],
    ["Déplace une forme", "Move a shape", "Mueve una forma", "形を移動"],
    ["Efface un morceau du rectangle", "Erase part of the rectangle", "Borra una parte del rectángulo", "長方形の一部を消す"],
    ["Entoure les deux formes", "Surround both shapes", "Rodea ambas formas", "2つの形を囲む"],
    ["Exporter", "Export", "Exportar", "エクスポート"],
    ["Faire entrer du contenu externe", "Bring in outside content", "Importar contenido externo", "外部コンテンツを取り込む"],
    ["Ferme la fenêtre", "Close the window", "Cierra la ventana", "ウィンドウを閉じる"],
    ["Ferme les Réglages", "Close Settings", "Cierra Ajustes", "設定を閉じる"],
    ["Formes, remplissage et gomme", "Shapes, fill and eraser", "Formas, relleno y borrador", "図形、塗り、消しゴム"],
    ["Fusionne (Union)", "Merge (Union)", "Fusiona (Unión)", "結合（ユニオン）"],
    ["Guide de perspective", "Perspective guide", "Guía de perspectiva", "パースガイド"],
    ["Historique de versions", "Version history", "Historial de versiones", "バージョン履歴"],
    ["Indique ton nom", "Enter your name", "Indica tu nombre", "名前を入力"],
    ["Insère une image-clé", "Insert a keyframe", "Inserta un fotograma clave", "キーフレームを挿入"],
    ["Interpolation automatique", "Automatic tweening", "Interpolación automática", "自動トゥイーン"],
    ["Lance le tween", "Run the tween", "Lanza el tween", "トゥイーンを実行"],
    ["Le tween automatique", "Automatic tweening", "El tween automático", "自動トゥイーン"],
    ["Motion — animer une propriété", "Motion — animate a property", "Motion — animar una propiedad", "Motion — プロパティをアニメーションする"],
    ["Onion skin", "Onion skin", "Onion skin", "オニオンスキン"],
    ["Opérations booléennes", "Boolean operations", "Operaciones booleanas", "ブール演算"],
    ["Ouvre l'historique", "Open history", "Abre el historial", "履歴を開く"],
    ["Ouvre l'import audio", "Open audio import", "Abre la importación de audio", "音声インポートを開く"],
    ["Ouvre l'onglet Collaboration", "Open the Collaboration tab", "Abre la pestaña Colaboración", "「コラボレーション」タブを開く"],
    ["Ouvre l'onglet Raccourcis", "Open the Shortcuts tab", "Abre la pestaña Atajos", "「ショートカット」タブを開く"],
    ["Ouvre la fenêtre d'export", "Open the export window", "Abre la ventana de exportación", "エクスポートウィンドウを開く"],
    ["Ouvre la section \"Perspective Guide\"", "Open the \"Perspective Guide\" section", "Abre la sección \"Perspective Guide\"", "「Perspective Guide」セクションを開く"],
    ["Ouvre la section \"Project\"", "Open the \"Project\" section", "Abre la sección \"Project\"", "「Project」セクションを開く"],
    ["Ouvre la section \"Projet\"", "Open the \"Project\" section", "Abre la sección \"Proyecto\"", "「Projet」セクションを開く"],
    ["Ouvre le StoryBoard", "Open StoryBoard", "Abre StoryBoard", "ストーリーボードを開く"],
    ["Ouvre les Réglages", "Open Settings", "Abre Ajustes", "設定を開く"],
    ["Palette de couleurs", "Color palette", "Paleta de colores", "カラーパレット"],
    ["Passe en mode Motion", "Switch to Motion mode", "Pasa al modo Motion", "Motionモードに切り替え"],
    ["Personnaliser l'app", "Customize the app", "Personalizar la app", "アプリをカスタマイズする"],
    ["Pose une nouvelle keyframe", "Set a new keyframe", "Coloca un nuevo fotograma clave", "新しいキーフレームを打つ"],
    ["Premier trait", "First stroke", "Primer trazo", "最初の一筆"],
    ["Profil et travail d'équipe", "Profile and teamwork", "Perfil y trabajo en equipo", "プロフィールとチーム作業"],
    ["Prélève une couleur", "Pick up a color", "Toma un color", "色を採取する"],
    ["Regarde le bouton \"Choisir…\"", "Look at the \"Choose…\" button", "Mira el botón \"Elegir…\"", "「選択…」ボタンを見る"],
    ["Remonter dans le temps", "Go back in time", "Retroceder en el tiempo", "時間を巻き戻す"],
    ["Remplis le rectangle", "Fill the rectangle", "Rellena el rectángulo", "長方形を塗りつぶす"],
    ["Revenir sur ce qui existe déjà", "Go back to what already exists", "Volver sobre lo que ya existe", "既存の形に戻って手を加える"],
    ["Reviens en Animation 2D", "Go back to 2D Animation", "Vuelve a Animación 2D", "2Dアニメーションに戻る"],
    ["Réglages et raccourcis", "Settings and shortcuts", "Ajustes y atajos", "設定とショートカット"],
    ["Sortir ton animation", "Get your animation out", "Exportar tu animación", "アニメーションを書き出す"],
    ["Sélection et transformation", "Select and transform", "Selección y transformación", "選択と変形"],
    ["Texte et pipette", "Text and eyedropper", "Texto y cuentagotas", "テキストとスポイト"],
    ["Trace une ligne", "Draw a line", "Traza una línea", "線を引く"],
    ["Travailler à plusieurs", "Working together", "Trabajar en equipo", "複数人での作業"],
    ["Un calque réutilisable", "A reusable layer", "Una capa reutilizable", "再利用できるレイヤー"],
    ["Une bibliothèque de couleurs", "A color library", "Una biblioteca de colores", "カラーライブラリ"],
    ["Une caméra virtuelle", "A virtual camera", "Una cámara virtual", "仮想カメラ"],
    ["Valide le texte", "Confirm the text", "Confirma el texto", "テキストを確定"],
    ["Voir à travers les frames", "See through the frames", "Ver a través de los fotogramas", "フレームを透かして見る"],
    ["Vérification…", "Checking…", "Verificando…", "確認中…"],
    ["Échange Fond et Trait", "Swap fill and stroke", "Intercambia relleno y trazo", "塗りと線を入れ替える"],
    ["Écris quelque chose", "Write something", "Escribe algo", "何か書いてみる"],
    ["1/2/3 points de fuite selon le mode choisi. \"Lock vanishing points\" évite de les déplacer par erreur en dessinant près d'eux.", "1/2/3 vanishing points depending on the chosen mode. \"Lock vanishing points\" prevents moving them by accident while drawing near them.", "1/2/3 puntos de fuga según el modo elegido. \"Lock vanishing points\" evita moverlos por error al dibujar cerca de ellos.", "選択したモードにより消失点は1〜3個。「Lock vanishing points」で、近くで描く際に誤って動かすのを防げます。"],
    ["Appuie sur F6 pour créer une nouvelle keyframe ici — c'est elle que tu vas dessiner.", "Press F6 to create a new keyframe here — this is the one you're about to draw on.", "Pulsa F6 para crear un nuevo fotograma clave aquí — es el que vas a dibujar.", "F6を押してここに新しいキーフレームを作成 — これから描くのはこのフレームです。"],
    ["Appuie sur la touche F6 de ton clavier pour transformer la frame actuelle en keyframe.", "Press the F6 key on your keyboard to turn the current frame into a keyframe.", "Pulsa la tecla F6 de tu teclado para convertir el fotograma actual en fotograma clave.", "キーボードのF6キーを押して、現在のフレームをキーフレームに変換します。"],
    ["Appuie sur la touche T pour interpoler automatiquement entre les deux keyframes.", "Press the T key to automatically interpolate between the two keyframes.", "Pulsa la tecla T para interpolar automáticamente entre los dos fotogramas clave.", "Tキーを押して、2つのキーフレーム間を自動的に補間します。"],
    ["Choisis une autre langue dans le menu déroulant — l'interface change instantanément.", "Choose another language from the dropdown — the interface changes instantly.", "Elige otro idioma en el menú desplegable — la interfaz cambia al instante.", "ドロップダウンから別の言語を選ぶと、インターフェースが即座に切り替わります。"],
    ["Clic-droit sur une pastille propose \"Remplacer dans le calque…\" — pratique pour recolorer tous les traits d'une teinte en une fois. Le \"+\" au-dessus des palettes en crée une nouvelle.", "Right-clicking a swatch offers \"Replace in layer…\" — handy for recoloring every stroke of one shade at once. The \"+\" above the palettes creates a new one.", "Clic derecho en una muestra ofrece \"Reemplazar en la capa…\" — práctico para recolorear todos los trazos de un tono a la vez. El \"+\" encima de las paletas crea una nueva.", "スウォッチを右クリックすると「レイヤー内で置換…」が表示されます — 同じ色のすべての線を一括で塗り替えるのに便利です。パレット上部の「+」で新しいパレットを作成できます。"],
    ["Clique \"Apply\" (ou Ctrl/Cmd+Entrée) pour poser le texte sur le canevas.", "Click \"Apply\" (or Ctrl/Cmd+Enter) to place the text on the canvas.", "Haz clic en \"Apply\" (o Ctrl/Cmd+Intro) para colocar el texto en el lienzo.", "「Apply」（またはCtrl/Cmd+Enter）をクリックしてテキストをキャンバスに配置します。"],
    ["Clique \"Choisir…\" pour voir comment on désigne un dossier partagé (nécessite l'app desktop — un simple message s'affiche ici en preview navigateur).", "Click \"Choose…\" to see how a shared folder is designated (requires the desktop app — just a message shows here in the browser preview).", "Haz clic en \"Elegir…\" para ver cómo se designa una carpeta compartida (requiere la app de escritorio — aquí en la vista previa del navegador solo se muestra un mensaje).", "「選択…」をクリックして共有フォルダの指定方法を確認します（デスクトップアプリが必要 — このブラウザプレビューでは単純なメッセージが表示されるだけです）。"],
    ["Clique \"Export…\" dans le panneau de droite (section Projet).", "Click \"Export…\" in the right panel (Project section).", "Haz clic en \"Export…\" en el panel derecho (sección Proyecto).", "右パネルの「Export…」をクリック（Projetセクション内）。"],
    ["Clique \"Frame suivante\" 2 ou 3 fois pour te placer plus loin dans la timeline.", "Click \"Next frame\" 2 or 3 times to move further along the timeline.", "Haz clic en \"Siguiente fotograma\" 2 o 3 veces para avanzar en la línea de tiempo.", "「次のフレーム」を2〜3回クリックして、タイムラインの先へ進みます。"],
    ["Clique \"Frame suivante\" plusieurs fois (ou glisse le curseur) pour te placer 5 à 10 frames plus loin.", "Click \"Next frame\" several times (or drag the playhead) to move 5 to 10 frames further.", "Haz clic en \"Siguiente fotograma\" varias veces (o arrastra el cursor) para avanzar de 5 a 10 fotogramas.", "「次のフレーム」を何度かクリック（またはカーソルをドラッグ）して5〜10フレーム先へ進みます。"],
    ["Clique \"Frame suivante\" plusieurs fois pour te placer plus loin.", "Click \"Next frame\" several times to move further along.", "Haz clic en \"Siguiente fotograma\" varias veces para avanzar.", "「次のフレーム」を何度かクリックして先へ進みます。"],
    ["Clique \"Frame suivante\" pour voir les frames voisines apparaître en fantôme.", "Click \"Next frame\" to see the neighboring frames appear as ghosts.", "Haz clic en \"Siguiente fotograma\" para ver los fotogramas vecinos aparecer como fantasmas.", "「次のフレーム」をクリックすると、前後のフレームが半透明の残像として表示されます。"],
    ["Clique \"Historique…\" dans le panneau de droite (section Projet).", "Click \"History…\" in the right panel (Project section).", "Haz clic en \"Historial…\" en el panel derecho (sección Proyecto).", "右パネルの「Historique…」をクリック（Projetセクション内）。"],
    ["Clique l'en-tête \"Perspective Guide\" dans le panneau de droite pour la déplier, si besoin.", "Click the \"Perspective Guide\" header in the right panel to expand it, if needed.", "Haz clic en el encabezado \"Perspective Guide\" del panel derecho para desplegarlo, si es necesario.", "必要であれば、右パネルの「Perspective Guide」見出しをクリックして開きます。"],
    ["Clique l'en-tête \"Project\" dans le panneau de droite pour la déplier.", "Click the \"Project\" header in the right panel to expand it.", "Haz clic en el encabezado \"Project\" del panel derecho para desplegarlo.", "右パネルの「Project」見出しをクリックして開きます。"],
    ["Clique l'en-tête \"Projet\" dans le panneau de droite pour la déplier, si besoin.", "Click the \"Project\" header in the right panel to expand it, if needed.", "Haz clic en el encabezado \"Proyecto\" del panel derecho para desplegarlo, si es necesario.", "必要であれば、右パネルの「Projet」見出しをクリックして開きます。"],
    ["Clique l'icône en forme de roue crantée en haut de l'écran.", "Click the gear icon at the top of the screen.", "Haz clic en el icono de engranaje en la parte superior de la pantalla.", "画面上部の歯車アイコンをクリックします。"],
    ["Clique l'onglet \"Animation 2D\" en haut de l'écran.", "Click the \"Animation 2D\" tab at the top of the screen.", "Haz clic en la pestaña \"Animation 2D\" en la parte superior de la pantalla.", "画面上部の「Animation 2D」タブをクリックします。"],
    ["Clique l'onglet \"Collaboration\" en haut de la fenêtre de Réglages.", "Click the \"Collaboration\" tab at the top of the Settings window.", "Haz clic en la pestaña \"Collaboration\" en la parte superior de la ventana de Ajustes.", "設定ウィンドウ上部の「Collaboration」タブをクリックします。"],
    ["Clique l'onglet \"Motion\" en haut de l'écran.", "Click the \"Motion\" tab at the top of the screen.", "Haz clic en la pestaña \"Motion\" en la parte superior de la pantalla.", "画面上部の「Motion」タブをクリックします。"],
    ["Clique l'onglet \"Raccourcis\" en haut de la fenêtre de Réglages.", "Click the \"Shortcuts\" tab at the top of the Settings window.", "Haz clic en la pestaña \"Atajos\" en la parte superior de la ventana de Ajustes.", "設定ウィンドウ上部の「Raccourcis」タブをクリックします。"],
    ["Clique l'onglet \"StoryBoard\" en haut de l'écran — c'est là que les Components se montent en séquence.", "Click the \"StoryBoard\" tab at the top of the screen — that's where Components get assembled into a sequence.", "Haz clic en la pestaña \"StoryBoard\" en la parte superior de la pantalla — ahí es donde los Componentes se montan en secuencia.", "画面上部の「StoryBoard」タブをクリック — ここでコンポーネントをシーケンスとして組み立てます。"],
    ["Clique la croix pour refermer la fenêtre.", "Click the × to close the window.", "Haz clic en la cruz para cerrar la ventana.", "×をクリックしてウィンドウを閉じます。"],
    ["Clique la croix — on ne lance pas un vrai export ici, juste la découverte de la fenêtre.", "Click the × — we're not running a real export here, just exploring the window.", "Haz clic en la cruz — aquí no se lanza una exportación real, solo estamos explorando la ventana.", "×をクリック — ここでは実際のエクスポートは行わず、ウィンドウを確認するだけです。"],
    ["Clique le bouton \"+\" en bas du panneau Calques — on part d'un calque tout neuf pour cet exercice.", "Click the \"+\" button at the bottom of the Layers panel — we're starting from a brand-new layer for this exercise.", "Haz clic en el botón \"+\" al final del panel Capas — para este ejercicio partimos de una capa totalmente nueva.", "レイヤーパネル下部の「+」ボタンをクリック — この演習は真新しいレイヤーから始めます。"],
    ["Clique le bouton \"+\" en bas du panneau Calques, à gauche de la timeline.", "Click the \"+\" button at the bottom of the Layers panel, to the left of the timeline.", "Haz clic en el botón \"+\" al final del panel Capas, a la izquierda de la línea de tiempo.", "レイヤーパネル下部、タイムライン左側の「+」ボタンをクリックします。"],
    ["Clique le bouton Union dans le panneau de droite pour fusionner les deux formes en une seule.", "Click the Union button in the right panel to merge the two shapes into one.", "Haz clic en el botón Unión del panel derecho para fusionar las dos formas en una sola.", "右パネルの「Union（結合）」ボタンをクリックして、2つの形をひとつに合成します。"],
    ["Clique le bouton caméra en bas du panneau Calques.", "Click the camera button at the bottom of the Layers panel.", "Haz clic en el botón de cámara al final del panel Capas.", "レイヤーパネル下部のカメラボタンをクリックします。"],
    ["Clique le bouton losange ◈ en bas du panneau Calques (\"Convert layer to component\").", "Click the diamond ◈ button at the bottom of the Layers panel (\"Convert layer to component\").", "Haz clic en el botón rombo ◈ al final del panel Capas (\"Convert layer to component\").", "レイヤーパネル下部のダイヤ型ボタン◈をクリック（「Convert layer to component」）。"],
    ["Clique le bouton note de musique en bas du panneau Calques pour voir le sélecteur de fichier s'ouvrir.", "Click the music-note button at the bottom of the Layers panel to see the file picker open.", "Haz clic en el botón de nota musical al final del panel Capas para ver cómo se abre el selector de archivos.", "レイヤーパネル下部の音符ボタンをクリックすると、ファイル選択ダイアログが開きます。"],
    ["Clique le bouton onion skin dans la timeline (ou la touche O) pour le couper, puis reclique pour le rallumer.", "Click the onion skin button in the timeline (or the O key) to turn it off, then click again to turn it back on.", "Haz clic en el botón de onion skin en la línea de tiempo (o la tecla O) para desactivarlo, y vuelve a hacer clic para reactivarlo.", "タイムラインのオニオンスキンボタン（またはOキー）をクリックしてオフにし、もう一度クリックしてオンに戻します。"],
    ["Clique le bouton ⇄ pour inverser les couleurs de Fond et de Trait.", "Click the ⇄ button to swap the Fill and Stroke colors.", "Haz clic en el botón ⇄ para intercambiar los colores de Relleno y Trazo.", "⇄ボタンをクリックして、塗りと線の色を入れ替えます。"],
    ["Clique le carré de couleur du Fond pour ouvrir le sélecteur, choisis une autre teinte.", "Click the Fill color swatch to open the picker, choose a different shade.", "Haz clic en la muestra de color de Relleno para abrir el selector, elige otro tono.", "塗りのカラースウォッチをクリックしてカラーピッカーを開き、別の色を選びます。"],
    ["Clique le carré de couleur du Trait et choisis une teinte qui n'est pas déjà utilisée.", "Click the Stroke color swatch and choose a shade that isn't already in use.", "Haz clic en la muestra de color de Trazo y elige un tono que no esté ya en uso.", "線のカラースウォッチをクリックし、まだ使われていない色を選びます。"],
    ["Clique le carré de couleur du Trait pour ouvrir le sélecteur, choisis une teinte.", "Click the Stroke color swatch to open the picker, choose a shade.", "Haz clic en la muestra de color de Trazo para abrir el selector, elige un tono.", "線のカラースウォッチをクリックしてカラーピッカーを開き、色を選びます。"],
    ["Clique le petit losange à côté de \"Position\" dans le panneau Motion pour poser une première clé.", "Click the small diamond next to \"Position\" in the Motion panel to set a first key.", "Haz clic en el pequeño rombo junto a \"Position\" en el panel Motion para colocar una primera clave.", "Motionパネルの「Position」横にある小さなダイヤをクリックして最初のキーを打ちます。"],
    ["Clique où poser le texte — une petite fenêtre de saisie apparaît.", "Click where you want to place the text — a small input box appears.", "Haz clic donde quieras colocar el texto — aparece una pequeña ventana de entrada.", "テキストを配置したい場所をクリック — 小さな入力ボックスが表示されます。"],
    ["Clique sur l'outil Gomme (raccourci E).", "Click the Eraser tool (shortcut E).", "Haz clic en la herramienta Borrador (atajo E).", "消しゴムツールをクリック（ショートカット: E）。"],
    ["Clique sur l'outil Ligne (raccourci U) — il va s'aimanter aux points de fuite.", "Click the Line tool (shortcut U) — it will snap to the vanishing points.", "Haz clic en la herramienta Línea (atajo U) — se ajustará a los puntos de fuga.", "直線ツールをクリック（ショートカット: U） — 消失点にスナップします。"],
    ["Clique sur l'outil Perspective dans la barre de gauche.", "Click the Perspective tool in the left toolbar.", "Haz clic en la herramienta Perspectiva en la barra izquierda.", "左側のツールバーでパースツールをクリックします。"],
    ["Clique sur l'outil Pinceau dans la barre de gauche (raccourci B).", "Click the Brush tool in the left toolbar (shortcut B).", "Haz clic en la herramienta Pincel en la barra izquierda (atajo B).", "左側のツールバーでブラシツールをクリック（ショートカット: B）。"],
    ["Clique sur l'outil Pipette dans la barre de gauche (raccourci I).", "Click the Eyedropper tool in the left toolbar (shortcut I).", "Haz clic en la herramienta Cuentagotas en la barra izquierda (atajo I).", "左側のツールバーでスポイトツールをクリック（ショートカット: I）。"],
    ["Clique sur l'outil Pot de peinture (raccourci G).", "Click the Paint Bucket tool (shortcut G).", "Haz clic en la herramienta Bote de pintura (atajo G).", "塗りつぶしツールをクリック（ショートカット: G）。"],
    ["Clique sur l'outil Rectangle (raccourci R) — on va se donner une couleur à prélever tout à l'heure.", "Click the Rectangle tool (shortcut R) — we'll give ourselves a color to pick up later.", "Haz clic en la herramienta Rectángulo (atajo R) — nos daremos un color para tomar más adelante.", "長方形ツールをクリック（ショートカット: R） — 後で採取する色を用意します。"],
    ["Clique sur l'outil Rectangle (raccourci R).", "Click the Rectangle tool (shortcut R).", "Haz clic en la herramienta Rectángulo (atajo R).", "長方形ツールをクリック（ショートカット: R）。"],
    ["Clique sur l'outil Rectangle dans la barre de gauche (raccourci R).", "Click the Rectangle tool in the left toolbar (shortcut R).", "Haz clic en la herramienta Rectángulo en la barra izquierda (atajo R).", "左側のツールバーで長方形ツールをクリック（ショートカット: R）。"],
    ["Clique sur l'outil Sélection (raccourci V).", "Click the Select tool (shortcut V).", "Haz clic en la herramienta Selección (atajo V).", "選択ツールをクリック（ショートカット: V）。"],
    ["Clique sur l'outil Sélection dans la barre de gauche (raccourci V).", "Click the Select tool in the left toolbar (shortcut V).", "Haz clic en la herramienta Selección en la barra izquierda (atajo V).", "左側のツールバーで選択ツールをクリック（ショートカット: V）。"],
    ["Clique sur l'outil Texte dans la barre de gauche.", "Click the Text tool in the left toolbar.", "Haz clic en la herramienta Texto en la barra izquierda.", "左側のツールバーでテキストツールをクリックします。"],
    ["Clique sur le rectangle (pas le texte — la pipette ne lit pas les images) pour reprendre sa couleur comme couleur de trait active.", "Click the rectangle (not the text — the eyedropper doesn't read images) to pick up its color as the active stroke color.", "Haz clic en el rectángulo (no en el texto — el cuentagotas no lee imágenes) para tomar su color como color de trazo activo.", "長方形をクリック（テキストではありません — スポイトは画像を読み取れません）して、その色をアクティブな線色として採取します。"],
    ["Clique sur un trait ou une forme dessinée puis fais-la glisser ailleurs sur le canevas.", "Click a drawn stroke or shape, then drag it elsewhere on the canvas.", "Haz clic en un trazo o forma dibujada y luego arrástrala a otra parte del lienzo.", "描いた線や形をクリックし、キャンバス上の別の場所へドラッグします。"],
    ["Clique une des pastilles de couleur — elle devient la couleur de Fond active (Shift+clic pour le Trait).", "Click one of the color swatches — it becomes the active Fill color (Shift+click for Stroke).", "Haz clic en una de las muestras de color — se convierte en el color de Relleno activo (Mayús+clic para el Trazo).", "カラースウォッチのひとつをクリック — アクティブな塗り色になります（Shift+クリックで線色）。"],
    ["Clique à l'intérieur du rectangle pour appliquer la nouvelle couleur.", "Click inside the rectangle to apply the new color.", "Haz clic dentro del rectángulo para aplicar el nuevo color.", "長方形の内側をクリックして新しい色を適用します。"],
    ["Clique-glisse depuis une zone vide du canevas pour entourer les deux rectangles et les sélectionner ensemble.", "Click-drag from an empty area of the canvas to surround both rectangles and select them together.", "Haz clic y arrastra desde una zona vacía del lienzo para rodear los dos rectángulos y seleccionarlos juntos.", "キャンバスの何もない場所からクリックドラッグして、2つの長方形を囲んで一緒に選択します。"],
    ["Clique-glisse en diagonale sur le canevas pour tracer un rectangle.", "Click-drag diagonally on the canvas to draw a rectangle.", "Haz clic y arrastra en diagonal sobre el lienzo para trazar un rectángulo.", "キャンバス上を斜めにクリックドラッグして長方形を描きます。"],
    ["Clique-glisse pour tracer un premier rectangle.", "Click-drag to draw a first rectangle.", "Haz clic y arrastra para trazar un primer rectángulo.", "クリックドラッグして最初の長方形を描きます。"],
    ["Clique-glisse sur le canevas blanc pour tracer un trait, comme au crayon.", "Click-drag on the white canvas to draw a stroke, like with a pencil.", "Haz clic y arrastra sobre el lienzo blanco para trazar un trazo, como con un lápiz.", "白いキャンバス上をクリックドラッグして、鉛筆で描くように線を引きます。"],
    ["Clique-glisse sur le canevas pour dessiner quelque chose sur cette première keyframe.", "Click-drag on the canvas to draw something on this first keyframe.", "Haz clic y arrastra sobre el lienzo para dibujar algo en este primer fotograma clave.", "キャンバス上をクリックドラッグして、この最初のキーフレームに何か描きます。"],
    ["Clique-glisse sur le canevas pour tracer un rectangle.", "Click-drag on the canvas to draw a rectangle.", "Haz clic y arrastra sobre el lienzo para trazar un rectángulo.", "キャンバス上をクリックドラッグして長方形を描きます。"],
    ["Clique-glisse sur le canevas — la ligne s'oriente vers un point de fuite.", "Click-drag on the canvas — the line orients itself toward a vanishing point.", "Haz clic y arrastra sobre el lienzo — la línea se orienta hacia un punto de fuga.", "キャンバス上をクリックドラッグ — 線が消失点に向かって配置されます。"],
    ["Clique-glisse sur un bord du rectangle pour en effacer une partie.", "Click-drag along an edge of the rectangle to erase part of it.", "Haz clic y arrastra sobre un borde del rectángulo para borrar una parte.", "長方形の端をクリックドラッグして一部を消します。"],
    ["Coche \"Enabled\" pour afficher la grille de points de fuite sur le canevas.", "Check \"Enabled\" to show the vanishing-point grid on the canvas.", "Marca \"Enabled\" para mostrar la cuadrícula de puntos de fuga en el lienzo.", "「Enabled」にチェックを入れて、キャンバスに消失点グリッドを表示します。"],
    ["Fais glisser le calque sur le canevas — une nouvelle clé de Position se crée automatiquement ici, à cette frame.", "Drag the layer on the canvas — a new Position key is created automatically here, at this frame.", "Arrastra la capa sobre el lienzo — se crea automáticamente una nueva clave de Posición aquí, en este fotograma.", "キャンバス上でレイヤーをドラッグ — このフレームに自動的に新しい「位置」キーが作成されます。"],
    ["Import Image(s)…/Import Video… vivent dans le menu principal. Le chapitre \"Caméra, audio et médias\" du guide couvre la bibliothèque de médias et la référence vidéo pour la rotoscopie.", "Import Image(s)…/Import Video… live in the main menu. The \"Camera, audio and media\" chapter of the guide covers the media library and video reference for rotoscoping.", "Import Image(s)…/Import Video… están en el menú principal. El capítulo \"Cámara, audio y medios\" de la guía cubre la biblioteca de medios y la referencia de vídeo para rotoscopia.", "Import Image(s)…/Import Video…はメインメニューにあります。ガイドの「カメラ、音声とメディア」の章で、メディアライブラリとロトスコープ用のビデオリファレンスを解説しています。"],
    ["L'onion skin affiche les frames voisines en transparence par-dessus la frame actuelle — pratique pour juger un mouvement sans jouer l'animation. Il est activé par défaut sur un nouveau projet ; on va vérifier que tu sais où le couper/rallumer.", "Onion skin shows the neighboring frames in transparency over the current frame — handy for judging a movement without playing the animation. It's on by default on a new project; let's check you know where to turn it off/on.", "El onion skin muestra los fotogramas vecinos en transparencia sobre el fotograma actual — útil para juzgar un movimiento sin reproducir la animación. Está activado por defecto en un proyecto nuevo; vamos a comprobar que sabes dónde desactivarlo/activarlo.", "オニオンスキンは、現在のフレームの上に前後のフレームを半透明で重ねて表示します — アニメーションを再生せずに動きを判断するのに便利です。新規プロジェクトではデフォルトで有効になっています。オン/オフの切り替え場所を確認しましょう。"],
    ["L'outil Sélection sert à reprendre une forme après coup — la déplacer, la redimensionner, la faire pivoter — sans devoir la redessiner.", "The Select tool lets you go back to a shape after the fact — move it, resize it, rotate it — without having to redraw it.", "La herramienta Selección sirve para retomar una forma después — moverla, redimensionarla, rotarla — sin tener que volver a dibujarla.", "選択ツールを使うと、後から形を移動・リサイズ・回転させることができ、描き直す必要がありません。"],
    ["La Pipette (I) prélève une couleur existante sur le canevas — pratique pour rester cohérent d'une frame à l'autre. Le chapitre \"Dessiner\" du guide couvre aussi les opérations booléennes (union/soustraction).", "The Eyedropper (I) picks up an existing color from the canvas — handy for staying consistent from one frame to the next. The \"Draw\" chapter of the guide also covers boolean operations (union/subtraction).", "El Cuentagotas (I) toma un color existente del lienzo — útil para mantener la coherencia de un fotograma a otro. El capítulo \"Dibujar\" de la guía también cubre las operaciones booleanas (unión/sustracción).", "スポイト（I）はキャンバス上の既存の色を採取します — フレームごとの色の一貫性を保つのに便利です。ガイドの「描く」の章では、ブール演算（結合/減算）についても解説しています。"],
    ["Le bouton \"Exporter\" (en bas de la fenêtre) lance le vrai rendu — image par image pour PNG/TIFF, ou en pilotant ffmpeg pour GIF/MP4/ProRes. Le chapitre \"Export\" du guide détaille chaque format.", "The \"Export\" button (at the bottom of the window) starts the actual render — frame by frame for PNG/TIFF, or by driving ffmpeg for GIF/MP4/ProRes. The \"Export\" chapter of the guide details each format.", "El botón \"Exportar\" (al final de la ventana) inicia el renderizado real — fotograma a fotograma para PNG/TIFF, o pilotando ffmpeg para GIF/MP4/ProRes. El capítulo \"Export\" de la guía detalla cada formato.", "「Exporter」ボタン（ウィンドウ下部）で実際のレンダリングが始まります — PNG/TIFFはフレームごと、GIF/MP4/ProResはffmpegを介して処理されます。ガイドの「Export」の章で各形式を詳しく解説しています。"],
    ["Le calque caméra sélectionné, glisse un cadre sur le canevas pour poser une clé de cadrage — chaque frame où tu ajustes le cadre en pose une nouvelle. Le chapitre \"Caméra, audio et médias\" du guide détaille l'éditeur de courbe dédié.", "With the camera layer selected, drag a frame on the canvas to set a framing key — every frame where you adjust the frame sets a new one. The \"Camera, audio and media\" chapter of the guide details the dedicated curve editor.", "Con la capa de cámara seleccionada, arrastra un encuadre sobre el lienzo para colocar una clave de encuadre — cada fotograma en el que ajustes el encuadre coloca una nueva. El capítulo \"Cámara, audio y medios\" de la guía detalla el editor de curvas dedicado.", "カメラレイヤーを選択した状態でキャンバス上のフレームをドラッグすると、カメラワークのキーが打たれます — フレーム調整のたびに新しいキーが作成されます。ガイドの「カメラ、音声とメディア」の章で専用のカーブエディタを解説しています。"],
    ["Le chapitre \"Components et StoryBoard\" du guide couvre les instances (vitesse/offset propres à chacune) et le montage nodal complet.", "The \"Components and StoryBoard\" chapter of the guide covers instances (each with its own speed/offset) and the full node-based editing.", "El capítulo \"Componentes y StoryBoard\" de la guía cubre las instancias (velocidad/desfase propios de cada una) y el montaje nodal completo.", "ガイドの「コンポーネントとストーリーボード」の章で、インスタンス（それぞれ独自の速度/オフセットを持つ）とノードベースの編集全体を解説しています。"],
    ["Le chapitre \"Dessiner\" du guide utilisateur détaille les options de police et de taille du texte.", "The \"Draw\" chapter of the user guide details the font and text size options.", "El capítulo \"Dibujar\" de la guía de usuario detalla las opciones de fuente y tamaño de texto.", "ユーザーガイドの「描く」の章で、フォントとテキストサイズのオプションを詳しく解説しています。"],
    ["Le chapitre \"Interpolation automatique\" du guide détaille les marqueurs de plage et le mode contours seuls.", "The \"Automatic tweening\" chapter of the guide details the range markers and outlines-only mode.", "El capítulo \"Interpolación automática\" de la guía detalla los marcadores de rango y el modo solo contornos.", "ガイドの「自動トゥイーン」の章で、範囲マーカーと輪郭のみモードを詳しく解説しています。"],
    ["Le chapitre \"Paramètres de l'application\" du guide couvre aussi Collaboration, Feedback et Labs.", "The \"App settings\" chapter of the guide also covers Collaboration, Feedback and Labs.", "El capítulo \"Ajustes de la aplicación\" de la guía también cubre Colaboración, Feedback y Labs.", "ガイドの「アプリ設定」の章では、Collaboration・Feedback・Labsについても解説しています。"],
    ["Le chapitre \"Travailler à plusieurs\" du guide couvre aussi les corrections à Accepter/Rejeter et le feedback d'équipe.", "The \"Working together\" chapter of the guide also covers corrections to Accept/Reject and team feedback.", "El capítulo \"Trabajar en equipo\" de la guía también cubre las correcciones a Aceptar/Rechazar y el feedback de equipo.", "ガイドの「複数人での作業」の章では、承認/却下する修正やチームフィードバックについても解説しています。"],
    ["Le guide de perspective affiche une grille de points de fuite et aimante l'outil Ligne dessus, depuis n'importe quel outil.", "The perspective guide shows a vanishing-point grid and snaps the Line tool to it, from any tool.", "La guía de perspectiva muestra una cuadrícula de puntos de fuga y ajusta la herramienta Línea a ella, desde cualquier herramienta.", "パースガイドは消失点グリッドを表示し、どのツールからでも直線ツールをそこにスナップさせます。"],
    ["Le mode Motion anime des PROPRIÉTÉS (position, rotation, échelle, opacité) par keyframes, façon After Effects — complémentaire du dessin frame par frame.", "Motion mode animates PROPERTIES (position, rotation, scale, opacity) by keyframes, After Effects style — complementary to frame-by-frame drawing.", "El modo Motion anima PROPIEDADES (posición, rotación, escala, opacidad) por fotogramas clave, al estilo After Effects — complementario al dibujo fotograma a fotograma.", "Motionモードは、After Effectsのようにキーフレームでプロパティ（位置・回転・拡大縮小・不透明度）をアニメーションします — フレームごとの作画を補完する機能です。"],
    ["Le nouveau calque doit apparaître dans la liste.", "The new layer should appear in the list.", "La nueva capa debe aparecer en la lista.", "新しいレイヤーがリストに表示されるはずです。"],
    ["Le panneau Palette garde des couleurs prêtes à réutiliser — plusieurs palettes nommées, glisser-déposer pour réordonner, clic pour appliquer.", "The Palette panel keeps colors ready to reuse — several named palettes, drag-and-drop to reorder, click to apply.", "El panel Paleta guarda colores listos para reutilizar — varias paletas con nombre, arrastrar y soltar para reordenar, clic para aplicar.", "パレットパネルは再利用できる色を保存します — 名前付きの複数パレット、ドラッグ&ドロップで並べ替え、クリックで適用。"],
    ["Le texte se pose comme une image (rendu, pas éditable ensuite) — pour du texte permanent façon titrage. La pipette prélève une couleur déjà présente sur le canevas.", "Text is placed as an image (rendered, not editable afterward) — for permanent, titling-style text. The eyedropper picks up a color already present on the canvas.", "El texto se coloca como una imagen (renderizado, no editable después) — para texto permanente estilo rótulo. El cuentagotas toma un color ya presente en el lienzo.", "テキストは画像として配置されます（レンダリング済みで後から編集不可） — タイトルのような固定テキスト用です。スポイトはキャンバス上に既にある色を採取します。"],
    ["Les poignées aux coins redimensionnent, celle au-dessus fait pivoter. Le panneau de droite (Position/Size/Rotate) permet aussi de saisir des valeurs exactes au clavier.", "The corner handles resize, the one above rotates. The right panel (Position/Size/Rotate) also lets you type exact values.", "Los tiradores de las esquinas redimensionan, el de arriba rota. El panel derecho (Position/Size/Rotate) también permite introducir valores exactos con el teclado.", "角のハンドルでリサイズ、上部のハンドルで回転できます。右パネル（Position/Size/Rotate）から正確な数値を入力することもできます。"],
    ["Même geste qu'au premier module — mais dessine autre chose, pour que le tween ait quelque chose à interpoler.", "Same gesture as the first module — but draw something different, so the tween has something to interpolate.", "El mismo gesto que en el primer módulo — pero dibuja algo distinto, para que el tween tenga algo que interpolar.", "最初のモジュールと同じ操作ですが、違うものを描いてください — トゥイーンが補間できる差分を作るためです。"],
    ["Nemo exporte en PNG/TIFF/GIF/MP4/ProRes, en SVG, en Lottie (JSON animé) ou en projet Rive/After Effects — un seul bouton, plusieurs formats.", "Nemo exports to PNG/TIFF/GIF/MP4/ProRes, SVG, Lottie (animated JSON), or a Rive/After Effects project — one button, several formats.", "Nemo exporta en PNG/TIFF/GIF/MP4/ProRes, en SVG, en Lottie (JSON animado) o en proyecto Rive/After Effects — un solo botón, varios formatos.", "Nemoは PNG/TIFF/GIF/MP4/ProRes、SVG、Lottie（アニメーションJSON）、またはRive/After Effectsプロジェクトとして書き出せます — ボタンひとつで複数形式に対応。"],
    ["Nemo importe de l'audio, des images (dont des séquences numérotées) et de la vidéo — chacune devient une piste ou un calque animé.", "Nemo imports audio, images (including numbered sequences) and video — each becomes a track or an animated layer.", "Nemo importa audio, imágenes (incluidas secuencias numeradas) y vídeo — cada uno se convierte en una pista o una capa animada.", "Nemoは音声、画像（連番シーケンスを含む）、動画をインポートできます — それぞれがトラックまたはアニメーションレイヤーになります。"],
    ["Nemo prend un instantané automatique toutes les 30 secondes — récupérable même après un crash, pas seulement la dernière session.", "Nemo takes an automatic snapshot every 30 seconds — recoverable even after a crash, not just the last session.", "Nemo toma una instantánea automática cada 30 segundos — recuperable incluso después de un fallo, no solo de la última sesión.", "Nemoは30秒ごとに自動スナップショットを取得します — クラッシュ後でも、直前のセッションだけでなく復元できます。"],
    ["On va dessiner un premier trait ensemble. À chaque étape, fais vraiment le geste demandé — le tutoriel avance tout seul dès que c'est fait.", "Let's draw a first stroke together. At each step, actually perform the requested action — the tutorial advances on its own once it's done.", "Vamos a dibujar un primer trazo juntos. En cada paso, realiza de verdad la acción pedida — el tutorial avanza solo en cuanto lo hagas.", "一緒に最初の一筆を描いてみましょう。各ステップで実際に指示された操作を行うと、チュートリアルは自動的に進みます。"],
    ["Ouvre le menu déroulant \"Format\" et choisis \"Lottie\" pour voir la fenêtre s'adapter (les options d'échelle disparaissent, un aperçu scrubbable s'ouvrira après export).", "Open the \"Format\" dropdown and choose \"Lottie\" to see the window adapt (the scale options disappear, a scrubbable preview will open after export).", "Abre el menú desplegable \"Format\" y elige \"Lottie\" para ver cómo se adapta la ventana (las opciones de escala desaparecen, se abrirá una vista previa navegable tras la exportación).", "Formatドロップダウンを開いて「Lottie」を選ぶと、ウィンドウが変化するのがわかります（スケールオプションが消え、書き出し後にスクラブ可能なプレビューが開きます）。"],
    ["Pose deux keyframes avec un dessin différent, puis laisse Nemo générer les frames intermédiaires tout seul. C'est le cœur de Nemo.", "Set two keyframes with different drawings, then let Nemo generate the in-between frames on its own. This is the heart of Nemo.", "Coloca dos fotogramas clave con un dibujo diferente, y deja que Nemo genere solo los fotogramas intermedios. Este es el corazón de Nemo.", "異なる絵を持つ2つのキーフレームを打ち、あとはNemoに中割りフレームを自動生成させます。これがNemoの核心機能です。"],
    ["Rectangle, Ellipse, Pot de peinture, Gomme — les outils de base pour construire des formes propres plutôt qu'à main levée.", "Rectangle, Ellipse, Paint Bucket, Eraser — the basic tools for building clean shapes rather than freehand.", "Rectángulo, Elipse, Bote de pintura, Borrador — las herramientas básicas para construir formas limpias en lugar de a mano alzada.", "長方形、楕円、塗りつぶし、消しゴム — 手描きではなくきれいな図形を作るための基本ツールです。"],
    ["Regarde la timeline : les frames entre tes deux keyframes sont maintenant interpolées. Le chapitre \"Interpolation automatique\" du guide couvre l'éditeur de courbes et l'onion skin.", "Look at the timeline: the frames between your two keyframes are now interpolated. The \"Automatic tweening\" chapter of the guide covers the curve editor and onion skin.", "Mira la línea de tiempo: los fotogramas entre tus dos fotogramas clave ahora están interpolados. El capítulo \"Interpolación automática\" de la guía cubre el editor de curvas y el onion skin.", "タイムラインを見てください：2つのキーフレームの間のフレームが補間されました。ガイドの「自動トゥイーン」の章でカーブエディタとオニオンスキンを解説しています。"],
    ["Rejoue (Entrée) pour voir le calque bouger entre les deux clés. Un calque avec 2 éléments ou plus devient automatiquement un Component dès qu'une propriété de calque est keyée — voir le module suivant.", "Play back (Enter) to see the layer move between the two keys. A layer with 2 or more elements automatically becomes a Component as soon as a layer property is keyed — see the next module.", "Reproduce (Intro) para ver la capa moverse entre las dos claves. Una capa con 2 o más elementos se convierte automáticamente en un Componente en cuanto se keyea una propiedad de la capa — ver el siguiente módulo.", "再生（Enter）して、レイヤーが2つのキーの間で動くのを確認しましょう。要素が2つ以上あるレイヤーは、レイヤープロパティにキーが打たれると自動的にコンポーネントになります — 次のモジュールを参照。"],
    ["Restaurer un ancien instantané prend d'abord un instantané de l'état actuel — l'opération reste donc elle-même annulable.", "Restoring an old snapshot first takes a snapshot of the current state — so the operation itself remains undoable.", "Restaurar una instantánea antigua primero toma una instantánea del estado actual — así que la operación en sí sigue siendo reversible.", "古いスナップショットを復元する際、まず現在の状態のスナップショットが自動的に取得されます — つまりこの操作自体も元に戻せます。"],
    ["Réglages regroupe la langue, ton profil, la collaboration, les raccourcis clavier et les prototypes expérimentaux (Labs).", "Settings groups together language, your profile, collaboration, keyboard shortcuts and experimental prototypes (Labs).", "Ajustes reúne el idioma, tu perfil, la colaboración, los atajos de teclado y los prototipos experimentales (Labs).", "設定には、言語、プロフィール、コラボレーション、キーボードショートカット、実験的なプロトタイプ（Labs）がまとまっています。"],
    ["Soustraction, Intersection et Exclusion suivent le même principe, juste à côté du bouton Union.", "Subtract, Intersect and Exclude follow the same principle, right next to the Union button.", "Sustracción, Intersección y Exclusión siguen el mismo principio, justo al lado del botón Unión.", "減算、交差、除外も同じ手順で、Unionボタンのすぐ隣にあります。"],
    ["Sélectionne \"Zone de travail\" ou \"Toute la timeline\" selon ce que tu veux sortir.", "Select \"Work area\" or \"Whole timeline\" depending on what you want to export.", "Selecciona \"Zona de trabajo\" o \"Toda la línea de tiempo\" según lo que quieras exportar.", "書き出したい内容に応じて「Zone de travail」または「Toute la timeline」を選択します。"],
    ["Tape ton nom dans le champ \"Nom\" de la section Profil.", "Type your name in the \"Name\" field of the Profile section.", "Escribe tu nombre en el campo \"Nombre\" de la sección Perfil.", "プロフィールセクションの「名前」欄に名前を入力します。"],
    ["Tape un mot ou deux dans le champ de texte qui vient de s'ouvrir.", "Type a word or two in the text field that just opened.", "Escribe una o dos palabras en el campo de texto que acaba de abrirse.", "開いたテキストフィールドに単語をいくつか入力します。"],
    ["Ton profil (nom + couleur) distingue tes traits de ceux d'un autre profil qui corrige ton travail. La Sync équipe publie/récupère les modifs via un dossier partagé (Drive, kDrive…), sans temps réel.", "Your profile (name + color) distinguishes your strokes from another profile's corrections to your work. Team Sync publishes/pulls changes through a shared folder (Drive, kDrive…), not in real time.", "Tu perfil (nombre + color) distingue tus trazos de las correcciones de otro perfil sobre tu trabajo. La Sincronización de equipo publica/recupera los cambios a través de una carpeta compartida (Drive, kDrive…), sin tiempo real.", "あなたのプロフィール（名前＋色）は、あなたの線を他のプロフィールによる修正と区別します。チーム同期は共有フォルダ（Drive、kDriveなど）経由で変更を公開/取得します（リアルタイムではありません）。"],
    ["Trace un second rectangle qui recouvre partiellement le premier.", "Draw a second rectangle that partially overlaps the first.", "Traza un segundo rectángulo que se superponga parcialmente al primero.", "最初の長方形と一部重なるように2つ目の長方形を描きます。"],
    ["Tu as un nouveau calque et une keyframe posée. Le chapitre \"Calques et timeline\" du guide couvre F5/F7, le drag & drop de frames, et le clic-droit sur la timeline.", "You now have a new layer and a keyframe set. The \"Layers and timeline\" chapter of the guide covers F5/F7, drag & drop of frames, and right-click on the timeline.", "Ahora tienes una capa nueva y un fotograma clave colocado. El capítulo \"Capas y línea de tiempo\" de la guía cubre F5/F7, el arrastrar y soltar fotogramas, y el clic derecho en la línea de tiempo.", "新しいレイヤーとキーフレームができました。ガイドの「レイヤーとタイムライン」の章で、F5/F7、フレームのドラッグ&ドロップ、タイムラインの右クリックを解説しています。"],
    ["Tu sais dessiner un trait et changer sa couleur. La suite du chapitre \"Dessiner\" du guide utilisateur détaille tous les autres outils (plume, formes, gomme…).", "You now know how to draw a stroke and change its color. The rest of the \"Draw\" chapter of the user guide details all the other tools (pen, shapes, eraser…).", "Ya sabes dibujar un trazo y cambiar su color. El resto del capítulo \"Dibujar\" de la guía de usuario detalla todas las demás herramientas (pluma, formas, borrador…).", "これで線を描いて色を変える方法がわかりました。ユーザーガイドの「描く」の章の続きで、他のすべてのツール（ペン、図形、消しゴムなど）を詳しく解説しています。"],
    ["Un Component est un calque transformé en symbole réutilisable — comme un symbole Flash/Animate. StoryBoard, le montage nodal de Nemo, ne manipule QUE des Components.", "A Component is a layer turned into a reusable symbol — like a Flash/Animate symbol. StoryBoard, Nemo's node-based editing, only ever works with Components.", "Un Componente es una capa convertida en símbolo reutilizable — como un símbolo de Flash/Animate. StoryBoard, el montaje nodal de Nemo, SOLO manipula Componentes.", "コンポーネントとは、レイヤーを再利用可能なシンボルに変換したものです — Flash/Animateのシンボルのようなものです。Nemoのノードベース編集であるストーリーボードは、コンポーネントのみを扱います。"],
    ["Un calque caméra anime le cadrage (zoom/pan/rotation) par-dessus toute la scène, avec des courbes de Bézier — comme dans TVPaint ou Callipeg.", "A camera layer animates the framing (zoom/pan/rotation) over the whole scene, with Bézier curves — like in TVPaint or Callipeg.", "Una capa de cámara anima el encuadre (zoom/paneo/rotación) sobre toda la escena, con curvas de Bézier — como en TVPaint o Callipeg.", "カメラレイヤーはシーン全体に対してベジェ曲線でカメラワーク（ズーム/パン/回転）をアニメーションします — TVPaintやCallipegのように。"],
    ["Un calque contient une série de frames. Une \"keyframe\" est une frame où tu as vraiment dessiné quelque chose de nouveau — c'est ce sur quoi l'interpolation automatique s'appuie.", "A layer contains a series of frames. A \"keyframe\" is a frame where you actually drew something new — that's what automatic tweening relies on.", "Una capa contiene una serie de fotogramas. Un \"fotograma clave\" es un fotograma en el que realmente dibujaste algo nuevo — es en lo que se basa la interpolación automática.", "レイヤーには一連のフレームが含まれます。「キーフレーム」とは、実際に新しく何かを描いたフレームのことです — これが自動トゥイーンの基盤になります。"],
    ["Union, soustraction, intersection, exclusion — combine plusieurs formes sélectionnées en une seule, plutôt que de redessiner à la main.", "Union, subtract, intersect, exclude — combine several selected shapes into one, instead of redrawing by hand.", "Unión, sustracción, intersección, exclusión — combina varias formas seleccionadas en una sola, en lugar de volver a dibujar a mano.", "結合、減算、交差、除外 — 選択した複数の形をひとつに合成でき、手で描き直す必要がありません。"],
    ["En attente de F6…", "Waiting for F6…", "Esperando F6…", "F6の入力待ち…"],
    ["En attente de T…", "Waiting for T…", "Esperando T…", "Tの入力待ち…"],
    ["En attente de ta forme…", "Waiting for your shape…", "Esperando tu forma…", "図形の入力待ち…"],
    ["En attente de ta saisie…", "Waiting for your input…", "Esperando tu texto…", "入力待ち…"],
    ["En attente de ton choix…", "Waiting for your choice…", "Esperando tu elección…", "選択待ち…"],
    ["En attente de ton clic…", "Waiting for your click…", "Esperando tu clic…", "クリック待ち…"],
    ["En attente de ton geste…", "Waiting for your action…", "Esperando tu gesto…", "操作待ち…"],
    ["En attente de ton trait…", "Waiting for your stroke…", "Esperando tu trazo…", "線の入力待ち…"],
    ["En attente…", "Waiting…", "Esperando…", "待機中…"],
    ["Un instant…", "One moment…", "Un momento…", "少々お待ちください…"],
    ["À toi de jouer…", "Your turn…", "Tu turno…", "あなたの番です…"],
    ["Quitter le tutoriel", "Exit tutorial", "Salir del tutorial", "チュートリアルを終了"],
    ["Passer ce module", "Skip this module", "Saltar este módulo", "このモジュールをスキップ"],
    ["Terminer", "Finish", "Finalizar", "終了"],
    ["Suivant", "Next", "Siguiente", "次へ"],
    ["Découvrir Nemo", "Discover Nemo", "Descubrir Nemo", "Nemoを発見する"],
    ["Des mini-leçons pas à pas, directement dans l'app — comme les tutoriels intégrés de Flash. Choisis une catégorie, puis un module ; tu peux quitter à tout moment.", "Step-by-step mini-lessons, right inside the app — like Flash's built-in tutorials. Choose a category, then a module; you can leave at any time.", "Mini-lecciones paso a paso, directamente en la app — como los tutoriales integrados de Flash. Elige una categoría, luego un módulo; puedes salir en cualquier momento.", "アプリ内で完結するステップバイステップのミニレッスン — Flashの内蔵チュートリアルのようなものです。カテゴリーを選び、次にモジュールを選んでください。いつでも終了できます。"],
    ["Retour", "Back", "Atrás", "戻る"],
    ["étape", "step", "paso", "ステップ"],
    ["Module \"%s\" terminé ✓", "Module \"%s\" completed ✓", "Módulo \"%s\" completado ✓", "「%s」モジュール完了 ✓"],
  ]);
  // Named T (not tt) — renderStep()'s local `tt` variable holds the tooltip
  // DOM element and would otherwise shadow a same-named lookup function.
  function T(fr) {
    if (!fr) return fr;
    var lang = (window.state && window.state.language) || 'fr';
    if (lang === 'fr') return fr;
    var table = TUT_TR[lang];
    return (table && table[fr]) || fr;
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
    },
    {
      id: 'export',
      category: 'Médias',
      icon: '18',
      title: 'Exporter',
      desc: 'Ouvrir la fenêtre d\'export, choisir un format',
      time: '1 min',
      // A real export writes to disk via une boîte de dialogue Tauri
      // native — pas quelque chose que ce tutoriel sandboxé peut piloter
      // ni vérifier (voir exportTauriAvailable(), export.js). Le module
      // reste donc volontairement au niveau "ouvrir/configurer" : chaque
      // étape est un vrai clic/changement réel sur la vraie fenêtre
      // d'export, sans jamais cliquer "Exporter" pour de vrai.
      // #btn-export vit dans le panneau de droite, section "Projet" —
      // COLLAPSÉE par défaut (même piège que #btn-history, voir module
      // 'history' plus haut). Étape tolérante façon 'perspective' : si la
      // section est déjà ouverte (ex. module Historique fait juste avant
      // dans la même session), un re-clic sur son en-tête la refermerait —
      // on attend juste que le bouton devienne visible, qu'il ait fallu
      // cliquer ou non.
      steps: [
        { type: 'info', title: 'Sortir ton animation', body: 'Nemo exporte en PNG/TIFF/GIF/MP4/ProRes, en SVG, en Lottie (JSON animé) ou en projet Rive/After Effects — un seul bouton, plusieurs formats.' },
        {
          type: 'state', target: '#phdr-project', title: 'Ouvre la section "Projet"', body: 'Clique l\'en-tête "Projet" dans le panneau de droite pour la déplier, si besoin.',
          hint: 'En attente…',
          check: function (win) { var el = win.document.getElementById('btn-export'); return !!(el && el.getBoundingClientRect().height > 0); }
        },
        { type: 'click', target: '#btn-export', title: 'Ouvre la fenêtre d\'export', body: 'Clique "Export…" dans le panneau de droite (section Projet).' },
        stateChangedStep({
          target: '#exp-format', title: 'Choisis un format', body: 'Ouvre le menu déroulant "Format" et choisis "Lottie" pour voir la fenêtre s\'adapter (les options d\'échelle disparaissent, un aperçu scrubbable s\'ouvrira après export).',
          hint: 'En attente de ton choix…', measure: function (win) { var el = win.document.getElementById('exp-format'); return el ? el.value : ''; }
        }),
        { type: 'click', target: '#exp-range', title: 'Choisis la plage à exporter', body: 'Sélectionne "Zone de travail" ou "Toute la timeline" selon ce que tu veux sortir.' },
        { type: 'click', target: '#export-close', title: 'Ferme la fenêtre', body: 'Clique la croix — on ne lance pas un vrai export ici, juste la découverte de la fenêtre.' },
        { type: 'info', title: 'Bien joué !', body: 'Le bouton "Exporter" (en bas de la fenêtre) lance le vrai rendu — image par image pour PNG/TIFF, ou en pilotant ffmpeg pour GIF/MP4/ProRes. Le chapitre "Export" du guide détaille chaque format.' }
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
      '<button class="tut-close" title="' + T('Quitter le tutoriel') + '">&times;</button>' +
      '<div class="tut-progress">' + T(mod.title) + ' — ' + T('étape') + ' ' + (active.stepIdx + 1) + '/' + mod.steps.length + '</div>' +
      '<div class="tut-title">' + T(step.title) + '</div>' +
      '<div class="tut-body">' + T(step.body) + '</div>' +
      (step.type !== 'info' ? '<div class="tut-hint"><span class="tut-dot"></span>' + T(step.hint || 'À toi de jouer…') + '</div>' : '') +
      '<div class="tut-actions">' +
      '<button class="tut-skip">' + T('Passer ce module') + '</button>' +
      (step.type === 'info' ? '<button class="tut-next">' + T(isLast ? 'Terminer' : 'Suivant') + '</button>' : '') +
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
      if (window.showToast) showToast(T('Module "%s" terminé ✓').replace('%s', T(mod.title)));
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
      '<div class="modal-hdr">' +
      '<button class="tut-launcher-back" id="tut-launcher-back" style="display:none" title="' + T('Retour') + '"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 6l-6 6 6 6"/></svg></button>' +
      '<span id="tut-launcher-title">' + T('Découvrir Nemo') + '</span> <button class="modal-x" id="tut-launcher-close">&times;</button></div>' +
      '<div class="modal-bdy">' +
      '<div class="tut-mod-list" id="tut-mod-list"></div>' +
      '</div></div>';
    document.body.appendChild(modal);
    modal.querySelector('#tut-launcher-close').addEventListener('click', closeLauncher);
    modal.querySelector('#tut-launcher-back').addEventListener('click', function () { renderCategoryScreen(); });
    modal.addEventListener('mousedown', function (e) { if (e.target === modal) closeLauncher(); });
    return modal;
  }

  // Categories render in this fixed order (not alphabetical, not
  // MODULES-array order) — roughly the order someone actually learning
  // Nemo would want: draw first, then animate, then organize/output.
  var CATEGORY_ORDER = ['Dessiner', 'Calques et animation', 'Organisation', 'Médias', 'Réglages'];

  // A real two-screen menu, not an inline accordion: the launcher opens on
  // a list of CATEGORIES only (icon + count, no modules shown yet);
  // clicking one navigates into a second screen listing just that
  // category's modules, with a back arrow (in the modal header) to return
  // to the category list. Matches how a phone Settings app drills down,
  // per explicit request — a first version that expanded categories inline
  // in the same list wasn't what was asked for.
  function groupByCategory() {
    var byCat = {};
    MODULES.forEach(function (m) {
      var cat = m.category || 'Autres';
      (byCat[cat] || (byCat[cat] = [])).push(m);
    });
    var cats = CATEGORY_ORDER.filter(function (c) { return byCat[c]; })
      .concat(Object.keys(byCat).filter(function (c) { return CATEGORY_ORDER.indexOf(c) === -1; }));
    return { byCat: byCat, cats: cats };
  }

  function renderCategoryScreen() {
    var list = $('#tut-mod-list'); if (!list) return;
    $('#tut-launcher-title').textContent = T('Découvrir Nemo');
    $('#tut-launcher-back').style.display = 'none';
    $('#tut-launcher-back').title = T('Retour');
    var done = loadDone();
    var g = groupByCategory();
    list.innerHTML = '';
    var intro = document.createElement('div');
    intro.className = 'tut-launcher-intro';
    intro.textContent = T('Des mini-leçons pas à pas, directement dans l\'app — comme les tutoriels intégrés de Flash. Choisis une catégorie, puis un module ; tu peux quitter à tout moment.');
    list.appendChild(intro);
    g.cats.forEach(function (cat) {
      var mods = g.byCat[cat];
      var doneCount = mods.filter(function (m) { return done.indexOf(m.id) !== -1; }).length;
      var btn = document.createElement('button');
      btn.className = 'tut-cat-hdr';
      btn.type = 'button';
      btn.innerHTML =
        '<span class="tut-cat-name">' + T(cat) + '</span>' +
        '<span class="tut-cat-count">' + doneCount + '/' + mods.length + '</span>' +
        '<span class="tut-cat-chevron">' + chevronSvg() + '</span>';
      btn.addEventListener('click', function () { renderModuleScreen(cat); });
      list.appendChild(btn);
    });
  }

  function renderModuleScreen(cat) {
    var list = $('#tut-mod-list'); if (!list) return;
    var g = groupByCategory();
    var mods = g.byCat[cat] || [];
    var done = loadDone();
    $('#tut-launcher-title').textContent = T(cat);
    $('#tut-launcher-back').style.display = 'flex';
    $('#tut-launcher-back').title = T('Retour');
    list.innerHTML = '';
    mods.forEach(function (m) {
      var isDone = done.indexOf(m.id) !== -1;
      var btn = document.createElement('button');
      btn.className = 'tut-mod' + (isDone ? ' done' : '');
      btn.innerHTML =
        '<span class="tut-mod-ico">' + (isDone ? '✓' : m.icon) + '</span>' +
        '<span class="tut-mod-body">' +
        '<span class="tut-mod-title">' + T(m.title) + '</span>' +
        '<span class="tut-mod-desc">' + T(m.desc) + '</span>' +
        '</span>' +
        '<span class="tut-mod-time">' + T(m.time) + '</span>';
      btn.addEventListener('click', function () { startModule(m.id); });
      list.appendChild(btn);
    });
  }

  function chevronSvg() {
    return '<svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M9 6l6 6-6 6"/></svg>';
  }

  function openLauncher() {
    ensureLauncher();
    renderCategoryScreen();
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
