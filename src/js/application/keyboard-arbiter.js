/* U05 (#2370): keyboard focus and clipboard arbitration, extracted from timeline.js onKeyDown. decide() is a pure function
 * from the key event and a snapshot of the selection and clipboard state to the one command the key means. It writes
 * nothing; timeline.js runs the command through the same functions as before. Rules (baseline 1065e9d8, unchanged):
 *  - in a text/number/select field or contenteditable, app undo, redo, copy, cut, paste and "convert to component" stand
 *    down so the field keeps its own editing; global Save still routes, and Enter in an input confirms and blurs it;
 *  - copy: a Motion key selection beats a canvas selection, which beats the frame-cell selection;
 *  - cut: a canvas selection beats the frame cells (Motion keys are copy-only);
 *  - paste: the Motion key clipboard wins in Motion, otherwise paste follows the clipboard filled last;
 *  - with no internal clipboard content, paste lets the browser paste event through when SVG import can take it.
 * mount() installs the one keydown/keyup listener pair per document: mounting again replaces it, dispose removes it. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoKeyboardArbiter=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function inField(target){
    return !!target&&(target.tagName==='INPUT'||target.tagName==='SELECT'||target.tagName==='TEXTAREA'||target.isContentEditable===true);
  }
  function cmd(event){return !!(event.metaKey||event.ctrlKey);}
  function pick(kind,prevent){return Object.freeze({kind:kind,preventDefault:prevent===true});}
  var NONE=null,FIELD=pick('field'),BLUR=pick('blurField');
  var UNDO=pick('undo',true),REDO=pick('redo',true),SAVE=pick('save',true),SAVE_AS=pick('saveAs',true),COMPONENT=pick('convertToComponent',true);
  var COPY_KEYS=pick('copyKeys',true),COPY_SHAPES=pick('copyShapes',true),COPY_FRAMES=pick('copyFrames',true);
  var CUT_SHAPES=pick('cutShapes',true),CUT_FRAMES=pick('cutFrames',true);
  var PASTE_SVG=pick('pasteSvgFallback'),PASTE_KEYS=pick('pasteKeys',true),PASTE_SHAPES=pick('pasteShapes',true),PASTE_FRAMES=pick('pasteFrames',true);
  /* ctx: {motionKeySelection, shapeSelection, motionKeyClipboard, canvasClipboard, frameClipboard, lastClipKind, svgImport}
   * (booleans, with lastClipKind 'canvas'|'frames'|other). Returns a frozen {kind,preventDefault}, or null when the key is
   * not an arbitrated one and onKeyDown carries on with its other shortcuts. */
  function decide(event,ctx){
    var field=inField(event.target),k=event.key;
    ctx=ctx||{};
    if(k==='Enter'&&event.target&&event.target.tagName==='INPUT')return BLUR;
    if(!cmd(event))return NONE;
    if(k==='z')return field?FIELD:(event.shiftKey?REDO:UNDO);
    if(k==='s')return event.shiftKey?SAVE_AS:SAVE;
    if(event.shiftKey&&(k==='c'||k==='C'))return field?FIELD:COMPONENT;
    if(k==='c')return field?FIELD:ctx.motionKeySelection?COPY_KEYS:ctx.shapeSelection?COPY_SHAPES:COPY_FRAMES;
    if(k==='x')return field?FIELD:ctx.shapeSelection?CUT_SHAPES:CUT_FRAMES;
    if(k==='v'){
      if(field)return FIELD;
      if(!(ctx.motionKeyClipboard||ctx.canvasClipboard||ctx.frameClipboard)&&ctx.svgImport)return PASTE_SVG;
      if(ctx.motionKeyClipboard)return PASTE_KEYS;
      if(ctx.lastClipKind==='canvas'&&ctx.canvasClipboard)return PASTE_SHAPES;
      if(ctx.lastClipKind==='frames'&&ctx.frameClipboard)return PASTE_FRAMES;
      return ctx.canvasClipboard?PASTE_SHAPES:PASTE_FRAMES;
    }
    return NONE;
  }
  var mounted=new WeakMap();
  // The single keydown/keyup pair for a document. Mounting again first removes the previous pair.
  function mount(doc,handlers){
    dispose(doc);
    var down=handlers&&handlers.keydown,up=handlers&&handlers.keyup;
    if(typeof down==='function')doc.addEventListener('keydown',down);
    if(typeof up==='function')doc.addEventListener('keyup',up);
    var entry={doc:doc,keydown:down,keyup:up};
    mounted.set(doc,entry);
    return Object.freeze({dispose:function(){if(mounted.get(doc)===entry)dispose(doc);}});
  }
  function dispose(doc){
    var entry=mounted.get(doc);if(!entry)return false;
    if(typeof entry.keydown==='function')doc.removeEventListener('keydown',entry.keydown);
    if(typeof entry.keyup==='function')doc.removeEventListener('keyup',entry.keyup);
    mounted.delete(doc);return true;
  }
  return Object.freeze({inField:inField,decide:decide,mount:mount,dispose:dispose});
}));
