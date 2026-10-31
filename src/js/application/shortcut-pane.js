/* U04a (#2370): the Settings > Shortcuts pane presenter, extracted from timeline.js renderShortcutsList (baseline 1065e9d8
 * markup, unchanged). It lists every tool, every rebindable command and the read-only keys, grouped by category with a search
 * box; a rebindable row is a button that arms ONE key capture, and the captured key goes through the registry:
 *  - Escape cancels, a pure modifier press (Shift, Ctrl, Alt, Meta) is ignored and keeps waiting;
 *  - a key another action already holds is refused with the existing "key already used by" notice and the row keeps its key;
 *  - otherwise the registry stores it (persisted by the registry's own storage port), the row shows it, and ports.onChange
 *    refreshes whatever else displays shortcuts. Dispatch reads the registry on every keypress, so a remap applies at once.
 * The capture listener sits on the window in the capture phase and stops the event immediately, so no other key handler
 * (tool shortcuts, the arbiter, bridge captures) also reacts to the key being assigned. Arming a second row cancels the first;
 * re-rendering, dispose() and cancelCapture() remove the listener. Every input is a port.
 * ports: {doc, win, registry, translate(key), notify(text), onChange(action)} */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoShortcutPane=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var MODIFIERS=['Shift','Control','Alt','Meta','AltGraph','OS'];
  var ROW_CSS='display:flex;align-items:center;justify-content:space-between;gap:8px;padding:5px 8px;border-radius:4px;font-size:11px;';
  var LABEL_CSS='flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
  var READONLY_CSS='min-width:60px;text-align:center;font-family:monospace;font-size:10px;color:var(--text-dim);border:1px dashed var(--border);border-radius:4px;padding:2px 6px;';
  var KEY_CSS='min-width:60px;font-family:monospace;text-transform:uppercase;';

  function create(ports){
    var doc=ports.doc,win=ports.win||ports.doc,registry=ports.registry;
    var armed=null; // {handler, restore}

    function t(key){return ports.translate(key);}
    function labelOf(def){return def.label&&def.label.indexOf('shortcut')===0?t(def.label):def.label;}

    function cancelCapture(){
      if(!armed)return false;
      var a=armed;armed=null;
      win.removeEventListener('keydown',a.handler,true);
      a.button.classList.remove('ac');
      a.button.textContent=registry.keyFor(a.action)||'—';
      return true;
    }

    /* Pure view model: the category groups the pane draws for a search text. */
    function model(query){
      var q=(query||'').trim().toLowerCase();
      function matches(label,key){return !q||(label||'').toLowerCase().indexOf(q)>=0||(key||'').toLowerCase().indexOf(q)>=0;}
      var groups=[];
      registry.categories().forEach(function(cat){
        var rows=[];
        if(cat.id==='tools')registry.toolShortcuts().forEach(function(d){
          var key=registry.keyFor(d.action)||'';
          if(matches(d.label,key))rows.push({action:d.action,label:d.label,key:key,rebindable:true});
        });
        registry.commandShortcuts().forEach(function(d){
          if(d.cat!==cat.id)return;
          var label=t(d.label),key=registry.keyFor(d.action)||'';
          if(matches(label,key))rows.push({action:d.action,label:label,key:key,rebindable:true});
        });
        registry.readonlyShortcuts().forEach(function(d){
          if(d.cat!==cat.id)return;
          var label=t(d.label);
          if(matches(label,d.keys))rows.push({label:label,key:d.keys,rebindable:false});
        });
        if(rows.length)groups.push({id:cat.id,label:t(cat.label),rows:rows});
      });
      return groups;
    }

    /* The outcome of assigning a pressed key to an action. Pure over the registry except for the store on success. */
    function assign(action,ev){
      if(ev.key==='Escape')return {kind:'cancel'};
      var clash=registry.clashFor(action,ev.key);
      if(clash)return {kind:'clash',clash:clash};
      registry.setKey(action,ev.key);
      return {kind:'set',key:ev.key.toLowerCase()};
    }

    function arm(button,def){
      cancelCapture();
      button.textContent='…';button.classList.add('ac');
      var entry={action:def.action,button:button};
      entry.handler=function(ev){
        if(MODIFIERS.indexOf(ev.key)>=0)return;
        ev.preventDefault();ev.stopPropagation();
        if(typeof ev.stopImmediatePropagation==='function')ev.stopImmediatePropagation();
        win.removeEventListener('keydown',entry.handler,true);
        armed=null;
        button.classList.remove('ac');
        var res=assign(def.action,ev);
        if(res.kind==='set'){
          button.textContent=res.key;
          if(typeof ports.onChange==='function')ports.onChange(def.action);
        }else{
          if(res.kind==='clash')ports.notify(t('shortcutKeyClashToast').replace('{label}',labelOf(res.clash)));
          button.textContent=registry.keyFor(def.action)||'—';
        }
      };
      armed=entry;
      win.addEventListener('keydown',entry.handler,true);
    }

    function keyRow(row,def){
      var el=doc.createElement('div');
      el.style.cssText=ROW_CSS;
      var lbl=doc.createElement('span');lbl.textContent=row.label;lbl.style.cssText=LABEL_CSS;
      el.appendChild(lbl);
      if(!row.rebindable){
        var ro=doc.createElement('span');
        ro.textContent=row.key;
        ro.title=t('shortcutNotRebindableTitle');
        ro.style.cssText=READONLY_CSS;
        el.appendChild(ro);
      }else{
        var keyBtn=doc.createElement('button');
        keyBtn.className='pbtn';keyBtn.style.cssText=KEY_CSS;
        keyBtn.textContent=row.key||'—';
        keyBtn.title=t('shortcutClickThenPressTitle');
        keyBtn.addEventListener('click',function(){arm(keyBtn,def);});
        el.appendChild(keyBtn);
      }
      return el;
    }

    /* Draws the pane into the list element (search box + grouped rows), replacing what was there. */
    function render(list){
      if(!list)return;
      cancelCapture();
      list.innerHTML='';
      var search=doc.createElement('input');
      search.type='search';search.className='pi';
      search.placeholder=t('shortcutSearchPlaceholder');
      search.style.cssText='width:100%;margin-bottom:8px;';
      list.appendChild(search);
      var body=doc.createElement('div');
      body.style.cssText='display:flex;flex-direction:column;gap:2px;';
      list.appendChild(body);
      function draw(){
        cancelCapture();
        body.innerHTML='';
        var groups=model(search.value),shown=0;
        groups.forEach(function(g){
          var hdr=doc.createElement('div');
          hdr.className='phdr';
          hdr.style.cssText='padding:8px 0 3px 0;font-size:9px;color:var(--text-dim);';
          hdr.textContent=g.label;
          body.appendChild(hdr);
          g.rows.forEach(function(row){body.appendChild(keyRow(row,row.rebindable?{action:row.action}:null));shown++;});
        });
        if(!shown){
          var none=doc.createElement('div');
          none.style.cssText='font-size:10px;color:var(--text-dim);padding:8px 4px;';
          none.textContent=t('shortcutNoMatch');
          body.appendChild(none);
        }
      }
      search.addEventListener('input',draw);
      // Typing in the search box must not trigger the shortcuts themselves.
      search.addEventListener('keydown',function(e){e.stopPropagation();});
      draw();
    }

    return Object.freeze({render:render,model:model,assign:assign,cancelCapture:cancelCapture,dispose:cancelCapture,
      capturing:function(){return !!armed;}});
  }
  return Object.freeze({create:create});
}));
