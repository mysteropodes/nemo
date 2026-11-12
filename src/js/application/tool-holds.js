/* U06 (#2370): temporary tool holds, extracted from timeline.js (baseline 1065e9d8 behaviour, unchanged). Every input is a
 * port, so the holds run against any document-like object and write nothing but the passed state and the passed setTool.
 *  - Space: holding arms the pan (state.spaceDown); a quick release with no pointer press in between is a tap and plays.
 *    Auto-repeat keydowns do not re-enter, Cmd/Ctrl+Space is the OS shortcut and never arms, and the play tap is inert
 *    under a Pen path in progress.
 *  - Alt: state.altDown, read by the view tools to rotate the canvas while it is held.
 *  - Cmd/Ctrl: after an arm delay with no other key pressed meanwhile, the tool becomes Select until release; the tool held
 *    before is restored exactly once, by the modifier keyup or by a lost keyup (window blur, page hidden).
 * A lost keyup resets everything through release(): no stuck Space, Alt or Select. mount() installs the document and window
 * listeners; mounting again replaces them, dispose removes them. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoToolHolds=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var SPACE_TAP_MS=250,CMD_HOLD_MS=180;
  var CURSORS=Object.freeze({draw:'crosshair',pen:'crosshair',line:'crosshair',rect:'crosshair',ellipse:'crosshair',select:'default',subselect:'default',eraser:'crosshair',fill:'crosshair',fillbrush:'crosshair',eyedropper:'crosshair',hand:'grab',zoom:'zoom-in',rotate:'grab',perspective:'crosshair'});
  function typing(el){return !!el&&(el.tagName==='INPUT'||el.tagName==='SELECT'||el.tagName==='TEXTAREA'||el.isContentEditable===true);}
  function isCmdKey(e){return e.key==='Meta'||e.key==='Control';}
  /* ports: {state, doc, win, setTool(tool), togglePlay(), penActive() -> bool, setCursor(css), now() -> ms,
   * setTimer(fn,ms), clearTimer(id)} — the last four default to the globals. */
  function create(ports){
    var state=ports.state,doc=ports.doc,win=ports.win;
    var now=ports.now||function(){return (globalThis.performance&&globalThis.performance.now)?globalThis.performance.now():Date.now();};
    var setTimer=ports.setTimer||function(fn,ms){return setTimeout(fn,ms);},clearTimer=ports.clearTimer||function(id){clearTimeout(id);};
    function setCursor(css){if(ports.setCursor)ports.setCursor(css);}
    function penActive(){return !!(ports.penActive&&ports.penActive());}
    function toolCursor(){return CURSORS[state.tool]||'default';}

    // ---- Space and Alt (called from onKeyDown / onKeyUp in timeline.js) ----
    function spaceKeyDown(event){
      if(event.metaKey||event.ctrlKey)return false;
      event.preventDefault();
      if(state.spaceDown)return true; // auto-repeat: the hold is already armed, its start time must not move
      state.spaceDown=true;state._spaceAt=now();state.spaceUsedForPan=false;
      setCursor('grab');
      return true;
    }
    function altKeyDown(){state.altDown=true;}
    function keyUp(event){
      if(event.key===' '){
        // wasDown gates the whole thing: onKeyDown returns before its Space branch whenever a field has focus, so a Space
        // typed into a text input would otherwise start playback on release.
        var wasDown=state.spaceDown;
        var tapped=wasDown&&!state.spaceUsedForPan&&(now()-(state._spaceAt||0))<SPACE_TAP_MS;
        state.spaceDown=false;state.isPanning=false;state.spaceUsedForPan=false;
        setCursor(toolCursor());
        // Mid-path the Pen tool owns Enter/Escape; starting playback under it would strand a half-drawn path.
        if(tapped&&!(state.tool==='pen'&&penActive()))ports.togglePlay();
      }else if(event.key==='Alt'){state.altDown=false;}
    }

    // ---- Cmd/Ctrl hold: temporary Select ----
    var cmdTimer=null,cmdPrevTool=null,cmdDown=false;
    function cmdEligible(){
      return !typing(doc&&doc.activeElement)&&!state.playing&&state.tool!=='select'&&state.tool!=='subselect'
        &&!penActive()&&!state.spaceDown;
    }
    function cancelCmdTimer(){if(cmdTimer!==null){clearTimer(cmdTimer);cmdTimer=null;return true;}return false;}
    // The one place the held tool comes back. The slot is emptied first, so nothing re-entering through setTool can restore twice.
    function restoreCmdTool(){
      if(cmdPrevTool===null)return false;
      var prev=cmdPrevTool;cmdPrevTool=null;ports.setTool(prev);return true;
    }
    function modifierDown(e){
      if(isCmdKey(e)){
        if(cmdDown)return; // already tracking this hold
        cmdDown=true;
        if(!cmdEligible())return;
        cmdTimer=setTimer(function(){
          cmdTimer=null;
          if(!cmdDown||!cmdEligible())return; // released, or state changed during the wait
          cmdPrevTool=state.tool;ports.setTool('select');
        },CMD_HOLD_MS);
      }else if(cmdTimer!==null){
        // A real key landed while the modifier was still "maybe about to be a hold": this is a shortcut combo, not a hold.
        cancelCmdTimer();
      }
    }
    function modifierUp(e){
      if(!isCmdKey(e))return;
      cmdDown=false;
      if(cancelCmdTimer())return; // never armed: nothing to restore
      restoreCmdTool();
    }

    // ---- Lost keyup: window blur, page hidden ----
    function release(){
      var wasSpace=!!state.spaceDown;
      state.spaceDown=false;state.isPanning=false;state.spaceUsedForPan=false;state.altDown=false;
      if(wasSpace)setCursor(toolCursor());
      cancelCmdTimer();cmdDown=false;restoreCmdTool();
    }
    // Any pointer press while Space is held belongs to the pan gesture, whether or not it ends up moving the view.
    function pointerDown(){if(state.spaceDown)state.spaceUsedForPan=true;}
    function onVisibility(){if(doc&&doc.hidden)release();}

    function mount(){
      doc.addEventListener('keydown',modifierDown);doc.addEventListener('keyup',modifierUp);
      doc.addEventListener('pointerdown',pointerDown,true);doc.addEventListener('visibilitychange',onVisibility);
      win.addEventListener('blur',release);
      return Object.freeze({dispose:function(){
        doc.removeEventListener('keydown',modifierDown);doc.removeEventListener('keyup',modifierUp);
        doc.removeEventListener('pointerdown',pointerDown,true);doc.removeEventListener('visibilitychange',onVisibility);
        win.removeEventListener('blur',release);
      }});
    }
    return Object.freeze({spaceKeyDown:spaceKeyDown,altKeyDown:altKeyDown,keyUp:keyUp,modifierDown:modifierDown,modifierUp:modifierUp,
      release:release,pointerDown:pointerDown,mount:mount,holdingSelect:function(){return cmdPrevTool!==null;}});
  }
  return Object.freeze({create:create,SPACE_TAP_MS:SPACE_TAP_MS,CMD_HOLD_MS:CMD_HOLD_MS,CURSORS:CURSORS});
}));
