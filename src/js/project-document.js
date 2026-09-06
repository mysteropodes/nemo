// Pure project JSON validation, shared by browser and native Open.
(function(){
  function parse(json){
    var d=JSON.parse(json);
    if(!d||typeof d!=='object'||(!d.layers&&!d.frames))throw new Error('Invalid');
    if(!d.layers)d.layers=[{name:'Layer 1',visible:true,locked:false,frames:d.frames}];
    // Reject malformed layers and frames before the importer tears down the
    // current document. Older frame-only files retain their existing migration.
    if(!Array.isArray(d.layers)||!d.layers.length)throw new Error('Fichier invalide (layers)');
    d.layers.forEach(function(ld,li){
      if(!ld||!Array.isArray(ld.frames))throw new Error('Fichier invalide (calque '+(li+1)+')');
      ld.frames.forEach(function(f,fi){
        if(!f||!Array.isArray(f.strokes))throw new Error('Fichier invalide (calque '+(li+1)+', frame '+(fi+1)+')');
      });
    });
    return d;
  }
  window.SMProjectDocument={parse:parse};
})();
