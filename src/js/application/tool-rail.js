/* U06 (#2370): the tool rail's shortcut badges and the shape-tool flyout, extracted from timeline.js (baseline 1065e9d8
 * behaviour, unchanged except where noted). Every input is a port.
 *  - Badges: the <span class="sk"> letter on each rail button shows the key the registry currently binds. sync(action) writes
 *    one, syncAll() every tool, and reset or a rebind both go through them, so a remap (or a reset to defaults) never leaves
 *    the old letter behind. A button title is prose and is never rewritten.
 *  - Shape flyout: long-press on the stacked shape button opens the other shapes. It closes on a pick, on a press outside, on
 *    Escape and on window blur; closing removes every listener it armed, including one whose arming delay had not fired yet. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoToolRail=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var SHAPE_TOOLS=Object.freeze(['rect','line','ellipse','speechbubble','star']);
  var LONG_PRESS_MS=450;
  /* ports: {doc, registry:{keyFor(action),toolShortcuts()}} */
  function createBadges(ports){
    var doc=ports.doc,registry=ports.registry;
    function sync(action){
      var btn=doc.querySelector('.tool-btn[data-tool="'+action+'"]');if(!btn)return false;
      var sk=btn.querySelector('.sk');if(!sk)return false;
      var key=registry.keyFor(action);sk.textContent=key?key.toUpperCase():'';
      return true;
    }
    function syncAll(){registry.toolShortcuts().forEach(function(s){sync(s.action);});}
    return Object.freeze({sync:sync,syncAll:syncAll});
  }
  /* ports: {doc, win, getTool(), setTool(tool), setTimer(fn,ms), clearTimer(id)} */
  function createShapeGroup(ports){
    var doc=ports.doc,win=ports.win;
    var setTimer=ports.setTimer||function(fn,ms){return setTimeout(fn,ms);},clearTimer=ports.clearTimer||function(id){clearTimeout(id);};
    var pressTimer=null,armTimer=null,suppressClick=false,flyoutEl=null,armed=false;
    function ensureFront(tool){
      if(SHAPE_TOOLS.indexOf(tool)<0)return;
      SHAPE_TOOLS.forEach(function(t){
        var b=doc.querySelector('.tool-btn[data-tool="'+t+'"]');
        if(b)b.classList.toggle('stack-front',t===tool);
      });
    }
    function onOutsideDown(e){if(flyoutEl&&!flyoutEl.contains(e.target))close();}
    function onKey(e){if(e.key==='Escape')close();}
    function disarm(){
      if(armTimer!==null){clearTimer(armTimer);armTimer=null;}
      if(!armed)return;
      armed=false;
      doc.removeEventListener('pointerdown',onOutsideDown,true);doc.removeEventListener('keydown',onKey,true);
      win.removeEventListener('blur',close);
    }
    function close(){
      if(flyoutEl){flyoutEl.remove();flyoutEl=null;}
      disarm();
    }
    function open(originBtn){
      close();
      var rect=originBtn.getBoundingClientRect();
      flyoutEl=doc.createElement('div');
      flyoutEl.className='shape-tool-flyout';
      SHAPE_TOOLS.forEach(function(t){
        var src=doc.querySelector('.tool-btn[data-tool="'+t+'"]');
        if(!src)return;
        var item=doc.createElement('button');
        item.className='shape-tool-flyout-item'+(t===ports.getTool()?' active':'');
        item.innerHTML=src.innerHTML.replace(/<span class="sk">.*?<\/span>/,'');
        item.title=src.title;
        item.addEventListener('click',function(ev){ev.stopPropagation();ensureFront(t);ports.setTool(t);close();});
        flyoutEl.appendChild(item);
      });
      doc.body.appendChild(flyoutEl);
      var fr=flyoutEl.getBoundingClientRect();
      flyoutEl.style.left=Math.min(rect.right+6,win.innerWidth-fr.width-4)+'px';
      flyoutEl.style.top=Math.max(4,Math.min(rect.top,win.innerHeight-fr.height-4))+'px';
      // Armed one tick later so the press that opened the flyout does not close it; close() cancels this if it comes first.
      armTimer=setTimer(function(){
        armTimer=null;if(!flyoutEl)return;
        armed=true;
        doc.addEventListener('pointerdown',onOutsideDown,true);doc.addEventListener('keydown',onKey,true);win.addEventListener('blur',close);
      },0);
    }
    function attach(stack){
      if(!stack)return;
      stack.addEventListener('pointerdown',function(e){
        var btn=e.target.closest('.tool-btn');
        if(!btn)return;
        suppressClick=false;
        pressTimer=setTimer(function(){suppressClick=true;open(btn);},LONG_PRESS_MS);
      });
      ['pointerup','pointerleave'].forEach(function(ev){stack.addEventListener(ev,function(){clearTimer(pressTimer);});});
      stack.addEventListener('click',function(ev){if(suppressClick){ev.stopImmediatePropagation();suppressClick=false;}},true);
    }
    return Object.freeze({ensureFront:ensureFront,open:open,close:close,attach:attach,isOpen:function(){return flyoutEl!==null;}});
  }
  return Object.freeze({SHAPE_TOOLS:SHAPE_TOOLS,LONG_PRESS_MS:LONG_PRESS_MS,createBadges:createBadges,createShapeGroup:createShapeGroup});
}));
