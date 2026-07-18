// ---- i18n ----
// Small, dependency-free translation layer: a key->string table per
// language, applied to any element tagged data-i18n (textContent) or
// data-i18n-title (title attribute). Switching is instant (re-scans and
// re-applies immediately, no reload) since every string is looked up fresh
// each time rather than baked into the DOM at load — matches the "à la
// volée" requirement rather than the weaker "restart to apply" fallback.
// Deliberately scoped to the Settings modal (the one place a user actually
// goes looking for this) plus the toolbar's tool tooltips (the next most
// visible surface) rather than attempting exhaustive coverage of every
// French string in the app in one pass — see CLAUDE.md on not rewriting
// more than a task calls for; the table/mechanism below is the real,
// extensible part, and any other string can be brought in later by just
// adding a data-i18n attribute + a key in each language block.
(function(){
  var I18N={
    en:{
      settingsTitle:'Settings',
      settingsTabGeneral:'General',settingsTabUpdates:'Updates',settingsTabCollab:'Collaboration',settingsTabFeedback:'Feedback',settingsTabShortcuts:'Shortcuts',
      settingsLanguage:'Language',
      settingsProfileHdr:'Profile (revision layers)',
      settingsProfileDesc:'Your name and color identify your strokes against another profile\'s corrections (supervisor, etc.) — see the Fill/Stroke Select tool and "Corrections" mode.',
      settingsName:'Name',
      settingsNamePh:'Your name',
      settingsRole:'Role',
      roleAnimator:'Animator',roleSupervisor:'Supervisor',roleProducer:'Producer',
      settingsSyncHdr:'Team sync (shared folder)',
      settingsSyncDesc:'Publish your changes to a shared folder (kDrive, mounted Drive, S3…) and pull others\' — async, not realtime. New strokes merge automatically; a stroke edited on both sides becomes a correction to Accept/Reject (like a revision layer).',
      settingsFolder:'Folder',
      settingsFolderPh:'No folder configured',
      settingsChoose:'Choose…',
      settingsPublish:'Publish',
      settingsCheckUpdates:'Check for updates',
      settingsShortcutsHdr:'Keyboard shortcuts — tools',
      settingsShortcutsDesc:'Click a key then press a new one to rebind it. Other shortcuts (transport, undo/save…) aren\'t editable here yet. Importing profiles (Animate, Blender…): coming soon.',
      settingsResetAll:'Reset all',
      toolSelect:'Select',toolSubselect:'Subselect (nodes)',toolFsselect:'Fill/Stroke Select',
      toolText:'Text',toolDraw:'Brush',toolPen:'Pen',toolFillbrush:'Fill Brush',
      toolLine:'Line',toolRect:'Rectangle',toolEllipse:'Ellipse',toolEraser:'Eraser',
      toolFill:'Fill',toolEyedropper:'Eyedropper',toolComment:'Comment',
      toolHand:'Hand',toolZoom:'Zoom',toolRotate:'Rotate canvas',toolPerspective:'Perspective',toolSymmetry:'Symmetry',
      hdrRevision:'Revision',hdrCamera:'Camera',hdrFill:'Fill',hdrStroke:'Stroke',hdrToolOptions:'Tool Options',
      hdrEffects:'Effects',hdrLayer:'Layer',hdrDocument:'Document',hdrSwatches:'Swatches',hdrPerspective:'Perspective Guide',hdrSymmetry:'Symmetry Guide',
      hdrReference:'Reference (roto)',hdrMedia:'Media',hdrTween:'Tween',hdrEasingCurve:'Easing Curve',hdrComponentInstance:'Component Instance',hdrProject:'Project',
    },
    fr:{
      settingsTitle:'Réglages',
      settingsTabGeneral:'Général',settingsTabUpdates:'Mises à jour',settingsTabCollab:'Collaboration',settingsTabFeedback:'Feedback',settingsTabShortcuts:'Raccourcis',
      settingsLanguage:'Langue',
      settingsProfileHdr:'Profil (calques de révision)',
      settingsProfileDesc:'Ton nom et ta couleur identifient tes traits face aux corrections d\'un autre profil (superviseur, etc.) — voir l\'outil Fill/Stroke Select et le mode "Corrections".',
      settingsName:'Nom',
      settingsNamePh:'Ton nom',
      settingsRole:'Rôle',
      roleAnimator:'Animateur',roleSupervisor:'Superviseur',roleProducer:'Producteur',
      settingsSyncHdr:'Sync équipe (dossier partagé)',
      settingsSyncDesc:'Publie tes modifs dans un dossier partagé (kDrive, Drive monté, S3…) et récupère celles des autres — async, pas de temps réel. Nouveaux traits fusionnés automatiquement ; un même trait modifié des deux côtés devient une correction à Accepter/Rejeter (comme un calque de révision).',
      settingsFolder:'Dossier',
      settingsFolderPh:'Aucun dossier configuré',
      settingsChoose:'Choisir…',
      settingsPublish:'Publier',
      settingsCheckUpdates:'Vérifier les mises à jour',
      settingsShortcutsHdr:'Raccourcis clavier — outils',
      settingsShortcutsDesc:'Cliquer une touche puis appuyer sur une nouvelle touche pour la réassigner. Les autres raccourcis (transport, undo/save…) ne sont pas encore modifiables ici. Import de profils (Animate, Blender…) : à venir.',
      settingsResetAll:'Réinitialiser tout',
      toolSelect:'Sélection',toolSubselect:'Sous-sélection (noeuds)',toolFsselect:'Sélection Fond/Trait',
      toolText:'Texte',toolDraw:'Pinceau',toolPen:'Plume',toolFillbrush:'Pinceau de remplissage',
      toolLine:'Ligne',toolRect:'Rectangle',toolEllipse:'Ellipse',toolEraser:'Gomme',
      toolFill:'Pot de peinture',toolEyedropper:'Pipette',toolComment:'Commentaire',
      toolHand:'Main',toolZoom:'Zoom',toolRotate:'Rotation du canevas',toolPerspective:'Perspective',toolSymmetry:'Symétrie',
      hdrRevision:'Révision',hdrCamera:'Caméra',hdrFill:'Fond',hdrStroke:'Trait',hdrToolOptions:'Options de l\'outil',
      hdrEffects:'Effets',hdrLayer:'Calque',hdrDocument:'Document',hdrSwatches:'Nuancier',hdrPerspective:'Guide de perspective',hdrSymmetry:'Guide de symétrie',
      hdrReference:'Référence (roto)',hdrMedia:'Médias',hdrTween:'Interpolation',hdrEasingCurve:'Courbe d\'accélération',hdrComponentInstance:'Instance de composant',hdrProject:'Projet',
    },
    ja:{
      settingsTitle:'設定',
      settingsTabGeneral:'一般',settingsTabUpdates:'アップデート',settingsTabCollab:'コラボレーション',settingsTabFeedback:'フィードバック',settingsTabShortcuts:'ショートカット',
      settingsLanguage:'言語',
      settingsProfileHdr:'プロフィール（修正レイヤー）',
      settingsProfileDesc:'名前と色は、他のプロフィール（スーパーバイザーなど）による修正に対して、あなたのストロークを識別します — Fill/Stroke Select ツールと「修正」モードを参照。',
      settingsName:'名前',
      settingsNamePh:'あなたの名前',
      settingsRole:'役割',
      roleAnimator:'アニメーター',roleSupervisor:'スーパーバイザー',roleProducer:'プロデューサー',
      settingsSyncHdr:'チーム同期（共有フォルダ）',
      settingsSyncDesc:'共有フォルダ（kDrive、マウントされたDrive、S3など）に変更を公開し、他の人の変更を取得します — 非同期、リアルタイムではありません。新しいストロークは自動的にマージされます。両側で編集されたストロークは、承認/却下の修正になります（修正レイヤーと同様）。',
      settingsFolder:'フォルダ',
      settingsFolderPh:'フォルダが設定されていません',
      settingsChoose:'選択…',
      settingsPublish:'公開',
      settingsCheckUpdates:'更新を確認',
      settingsShortcutsHdr:'キーボードショートカット — ツール',
      settingsShortcutsDesc:'キーをクリックしてから新しいキーを押すと再割り当てされます。他のショートカット（トランスポート、元に戻す/保存など）はまだここでは編集できません。プロファイルのインポート（Animate、Blenderなど）：近日公開。',
      settingsResetAll:'すべてリセット',
      toolSelect:'選択',toolSubselect:'サブ選択（ノード）',toolFsselect:'塗り/線 選択',
      toolText:'テキスト',toolDraw:'ブラシ',toolPen:'ペン',toolFillbrush:'塗りブラシ',
      toolLine:'直線',toolRect:'長方形',toolEllipse:'楕円',toolEraser:'消しゴム',
      toolFill:'塗りつぶし',toolEyedropper:'スポイト',toolComment:'コメント',
      toolHand:'手のひら',toolZoom:'ズーム',toolRotate:'キャンバス回転',toolPerspective:'パース',toolSymmetry:'シンメトリー',
      hdrRevision:'修正',hdrCamera:'カメラ',hdrFill:'塗り',hdrStroke:'線',hdrToolOptions:'ツールオプション',
      hdrEffects:'エフェクト',hdrLayer:'レイヤー',hdrDocument:'ドキュメント',hdrSwatches:'スウォッチ',hdrPerspective:'パースガイド',hdrSymmetry:'シンメトリーガイド',
      hdrReference:'参照（ロト）',hdrMedia:'メディア',hdrTween:'トゥイーン',hdrEasingCurve:'イージングカーブ',hdrComponentInstance:'コンポーネントインスタンス',hdrProject:'プロジェクト',
    },
    es:{
      settingsTitle:'Ajustes',
      settingsTabGeneral:'General',settingsTabUpdates:'Actualizaciones',settingsTabCollab:'Colaboración',settingsTabFeedback:'Feedback',settingsTabShortcuts:'Atajos',
      settingsLanguage:'Idioma',
      settingsProfileHdr:'Perfil (capas de revisión)',
      settingsProfileDesc:'Tu nombre y color identifican tus trazos frente a las correcciones de otro perfil (supervisor, etc.) — ver la herramienta Selección Relleno/Trazo y el modo "Correcciones".',
      settingsName:'Nombre',
      settingsNamePh:'Tu nombre',
      settingsRole:'Rol',
      roleAnimator:'Animador',roleSupervisor:'Supervisor',roleProducer:'Productor',
      settingsSyncHdr:'Sincronización de equipo (carpeta compartida)',
      settingsSyncDesc:'Publica tus cambios en una carpeta compartida (kDrive, Drive montado, S3…) y descarga los de otros — asíncrono, no en tiempo real. Los trazos nuevos se fusionan automáticamente; un trazo editado en ambos lados se convierte en una corrección para Aceptar/Rechazar (como una capa de revisión).',
      settingsFolder:'Carpeta',
      settingsFolderPh:'Ninguna carpeta configurada',
      settingsChoose:'Elegir…',
      settingsPublish:'Publicar',
      settingsCheckUpdates:'Buscar actualizaciones',
      settingsShortcutsHdr:'Atajos de teclado — herramientas',
      settingsShortcutsDesc:'Haz clic en una tecla y luego pulsa una nueva para reasignarla. Otros atajos (transporte, deshacer/guardar…) aún no son editables aquí. Importar perfiles (Animate, Blender…): próximamente.',
      settingsResetAll:'Restablecer todo',
      toolSelect:'Selección',toolSubselect:'Subselección (nodos)',toolFsselect:'Selección Relleno/Trazo',
      toolText:'Texto',toolDraw:'Pincel',toolPen:'Pluma',toolFillbrush:'Pincel de relleno',
      toolLine:'Línea',toolRect:'Rectángulo',toolEllipse:'Elipse',toolEraser:'Borrador',
      toolFill:'Bote de pintura',toolEyedropper:'Cuentagotas',toolComment:'Comentario',
      toolHand:'Mano',toolZoom:'Zoom',toolRotate:'Rotar lienzo',toolPerspective:'Perspectiva',toolSymmetry:'Simetría',
      hdrRevision:'Revisión',hdrCamera:'Cámara',hdrFill:'Relleno',hdrStroke:'Trazo',hdrToolOptions:'Opciones de herramienta',
      hdrEffects:'Efectos',hdrLayer:'Capa',hdrDocument:'Documento',hdrSwatches:'Muestras',hdrPerspective:'Guía de perspectiva',hdrSymmetry:'Guía de simetría',
      hdrReference:'Referencia (roto)',hdrMedia:'Medios',hdrTween:'Interpolación',hdrEasingCurve:'Curva de aceleración',hdrComponentInstance:'Instancia de componente',hdrProject:'Proyecto',
    },
  };
  var LANGS=['en','fr','ja','es'];
  function currentLang(){
    return (window.state&&state.language)||'fr';
  }
  function t(key){
    var lang=currentLang();
    var table=I18N[lang]||I18N.fr;
    return table[key]!==undefined?table[key]:(I18N.en[key]!==undefined?I18N.en[key]:key);
  }
  function applyI18n(){
    document.querySelectorAll('[data-i18n]').forEach(function(el){el.textContent=t(el.getAttribute('data-i18n'));});
    document.querySelectorAll('[data-i18n-title]').forEach(function(el){el.title=t(el.getAttribute('data-i18n-title'));});
    document.querySelectorAll('[data-i18n-placeholder]').forEach(function(el){el.placeholder=t(el.getAttribute('data-i18n-placeholder'));});
    var langSel=document.getElementById('settings-language');
    if(langSel&&langSel.value!==currentLang())langSel.value=currentLang();
  }
  function setLanguage(lang){
    if(LANGS.indexOf(lang)<0)return;
    state.language=lang;
    try{localStorage.setItem('nemo-lang',lang);}catch(e){}
    applyI18n();
  }
  function initLanguage(){
    var saved=null;
    try{saved=localStorage.getItem('nemo-lang');}catch(e){}
    state.language=(saved&&LANGS.indexOf(saved)>=0)?saved:(state.language||'fr');
    applyI18n();
    var sel=document.getElementById('settings-language');
    if(sel)sel.addEventListener('change',function(){setLanguage(this.value);});
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initLanguage);else initLanguage();
  window.SM=window.SM||{};
  window.SM.setLanguage=setLanguage;
  window.SM.t=t;
  window.SM.applyI18n=applyI18n;
})();
