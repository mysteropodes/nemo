// History Panel (2026-07, feedback: "Créer un panel d'historique") — a
// read+jump UI over state.undoStack/redoStack (tweens.js's pushUndoLayers/
// undo/redo), paired with the parallel undoLabels/redoLabels arrays (see
// their doc comment next to undoStack's init in app.js) that carry a
// human-readable {label,tool,frame,layer,t} for each snapshot. Session-only
// like the stacks themselves — nothing here is ever persisted, same
// convention as state.actionLog (CLAUDE.md §6: neither transits through
// exportJSON()).
//
// List order top→bottom = chronological: oldest action first, the current
// position highlighted, anything below is a future/redoable state (shown
// dimmed). Clicking an entry calls undo()/redo() the exact number of times
// needed to land on it — both already fully re-render canvas/timeline/UI
// per call, so no extra plumbing is needed here beyond the click handler.
// Same trigger-button + fixed-position-popover pattern as #fb-avatars/
// #fb-avatars-pop (timeline.js) — see that file's initFbAvatars for the
// twin implementation this one was modeled on.

function _histRelTime(t){
  var d=Date.now()-(t||0);
  if(d<1500)return 'à l\'instant';
  if(d<60000)return Math.round(d/1000)+'s';
  if(d<3600000)return Math.round(d/60000)+'min';
  return Math.round(d/3600000)+'h';
}
function _histRow(entry,extraCls,frameLabel){
  var item=document.createElement('div');
  item.className='hist-item'+(extraCls?' '+extraCls:'');
  var dot=document.createElement('div');dot.className='hist-dot';
  item.appendChild(dot);
  var body=document.createElement('div');body.className='hist-body';
  var label=document.createElement('div');label.className='hist-label';
  label.textContent=(entry&&entry.label)||'?';
  body.appendChild(label);
  var meta=document.createElement('div');meta.className='hist-meta';
  var bits=[];
  if(frameLabel)bits.push(frameLabel);
  else if(entry&&typeof entry.frame==='number')bits.push('frame '+(entry.frame+1));
  if(entry&&entry.layer)bits.push(entry.layer);
  if(entry&&entry.t)bits.push(_histRelTime(entry.t));
  meta.textContent=bits.join(' · ');
  body.appendChild(meta);
  item.appendChild(body);
  return item;
}
function renderHistoryPanel(){
  var pop=document.getElementById('history-pop');
  if(!pop)return;
  pop.innerHTML='';
  var undoStack=state.undoStack||[],undoLabels=state.undoLabels||[];
  var redoStack=state.redoStack||[],redoLabels=state.redoLabels||[];

  var header=document.createElement('div');header.className='hist-header';
  header.appendChild(document.createTextNode((undoStack.length+redoStack.length+1)+' états'));
  var clear=document.createElement('span');clear.className='hist-clear';clear.textContent='Effacer';
  clear.title='Vider l\'historique de session (n\'affecte pas le contenu du projet)';
  clear.addEventListener('click',function(e){
    e.stopPropagation();
    state.undoStack=[];state.undoLabels=[];state.redoStack=[];state.redoLabels=[];
    renderHistoryPanel();
    showToast('Historique vidé');
  });
  header.appendChild(clear);
  pop.appendChild(header);

  if(!undoStack.length&&!redoStack.length){
    var empty=document.createElement('div');empty.className='hist-empty';
    empty.textContent='Aucune action encore dans cette session.';
    pop.appendChild(empty);
    return;
  }

  // Past — oldest first, most recent action last (right above "current").
  // Clicking entry i calls undo() (undoStack.length - i) times, reading the
  // LIVE array at click time (never reassigned wholesale, only push/pop/
  // shift — see tweens.js) so this stays correct even if more actions
  // happened while the popover was left open.
  for(var i=0;i<undoStack.length;i++){
    (function(i){
      var row=_histRow(undoLabels[i]);
      row.addEventListener('click',function(){
        var steps=undoStack.length-i;
        for(var k=0;k<steps;k++)undo();
      });
      pop.appendChild(row);
    })(i);
  }

  // Current position — read fresh from live state, not a stored label
  // (there is nothing to jump to, it's just a marker).
  var ld=state.layers&&state.layers[state.activeLayerIdx];
  var curRow=_histRow({label:'État actuel',frame:state.currentFrame,layer:ld?ld.name:''},'current');
  pop.appendChild(curRow);

  // Future — nearest redo first. redoStack's LAST element is what the next
  // redo() call actually restores (see tweens.js's push/pop symmetry), so
  // walking a reversed copy lists it in true chronological-forward order.
  var fwd=redoStack.slice().reverse(),fwdLabels=redoLabels.slice().reverse();
  for(var j=0;j<fwd.length;j++){
    (function(j){
      var row=_histRow(fwdLabels[j],'future');
      row.addEventListener('click',function(){
        var steps=j+1;
        for(var k=0;k<steps;k++)redo();
      });
      pop.appendChild(row);
    })(j);
  }

  // Keep the current-position row in view (matches Photoshop's History
  // panel auto-scroll) without forcing every open to jump to an end.
  requestAnimationFrame(function(){curRow.scrollIntoView({block:'nearest'});});
}
// Cheap guard used by tweens.js's pushUndoLayers/undo/redo so the panel
// stays live while open, without paying render cost while it's closed.
function renderHistoryPanelIfOpen(){
  var pop=document.getElementById('history-pop');
  if(pop&&pop.classList.contains('open'))renderHistoryPanel();
}
function toggleHistoryPopover(){
  var pop=document.getElementById('history-pop');
  if(!pop)return;
  if(pop.classList.contains('open')){pop.classList.remove('open');return;}
  renderHistoryPanel();
  var r=document.getElementById('history-btn').getBoundingClientRect();
  pop.style.top=(r.bottom+6)+'px';
  pop.style.right=(window.innerWidth-r.right)+'px';
  pop.classList.add('open');
}
function initHistoryPanel(){
  var btn=document.getElementById('history-btn');
  if(!btn)return;
  btn.addEventListener('click',function(e){e.stopPropagation();toggleHistoryPopover();});
  document.addEventListener('click',function(e){
    var pop=document.getElementById('history-pop');
    if(pop&&pop.classList.contains('open')&&!pop.contains(e.target)&&e.target!==btn)pop.classList.remove('open');
  });
}
if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initHistoryPanel);else initHistoryPanel();
