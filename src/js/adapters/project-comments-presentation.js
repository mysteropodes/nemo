/* Existing comment hit-test and collaborator presentation, with confirmed native reads. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoProjectCommentsPresentation=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  var installed=new WeakMap();
  function create(root){
    if(installed.has(root))return installed.get(root);
    var document=root.document,window=root,state=root.state;
    function goToFrame(frame){return root.goToFrame(frame);}
    function native(){return !!(root.__TAURI__&&root.__TAURI__.core);}
    function read(){
      if(!native())return {items:state.comments||[]};
      var ui=root.NemoNativeCommentsUI,projection=ui&&ui.read(root);
      return projection?{projection:projection,items:projection.comments.items.map(function(item){return item.comment;})}:null;
    }
    function current(origin){return !native()||!!origin&&!!root.NemoNativeCommentsUI&&root.NemoNativeCommentsUI.read(root)===origin.projection;}
function fbInitial(name){return (name||'?').trim().charAt(0).toUpperCase()||'?';}
function refreshFbAvatars(){
  var el=document.getElementById('fb-avatars');
  if(!el)return;
  var origin=read();
  if(!origin){el.innerHTML='';el.dataset.status='unavailable';return;}
  el.dataset.status='available';
  var comments=origin.items;
  var seen={},people=[];
  if(state.userProfile){people.push({id:state.userProfile.id,name:state.userProfile.name,color:state.userProfile.color,me:true});seen[state.userProfile.id]=true;}
  comments.forEach(function(c){
    if(c.authorId&&!seen[c.authorId]){seen[c.authorId]=true;people.push({id:c.authorId,name:c.authorName,color:c.authorColor});}
  });
  el.innerHTML='';
  var maxShown=4;
  people.slice(0,maxShown).forEach(function(p){
    var av=document.createElement('div');
    av.className='fb-av';av.style.background=p.color||'#888';
    av.textContent=fbInitial(p.name);
    av.title=(p.name||'?')+(p.me?' (toi)':'');
    el.appendChild(av);
  });
  if(people.length>maxShown){
    var more=document.createElement('div');
    more.className='fb-av fb-av-more';more.textContent='+'+(people.length-maxShown);
    el.appendChild(more);
  }
}
function renderFbAvatarPopover(){
  var pop=document.getElementById('fb-avatars-pop');
  if(!pop)return;
  pop.innerHTML='';
  var origin=read();
  if(!origin){pop.textContent='Project comments are unavailable or unconfirmed.';return;}
  var comments=origin.items.slice().sort(function(a,b){return (b.createdAt||0)-(a.createdAt||0);});
  if(!comments.length){
    pop.innerHTML='<div class="fb-pop-empty">Aucun commentaire d\'équipe sur ce projet.</div>';
    return;
  }
  comments.forEach(function(cm){
    var item=document.createElement('div');item.className='fb-pop-item';
    var av=document.createElement('div');av.className='fb-pop-av';
    av.style.background=cm.authorColor||'#888';
    av.textContent=fbInitial(cm.authorName);
    item.appendChild(av);
    var body=document.createElement('div');body.className='fb-pop-body';
    var note=document.createElement('div');note.className='fb-pop-note';note.textContent=cm.text||'';note.title=cm.text||'';
    body.appendChild(note);
    var meta=document.createElement('div');meta.className='fb-pop-meta';
    meta.appendChild(document.createTextNode((cm.authorName||'?')+' · frame '+((cm.frame||0)+1)));
    var st=document.createElement('span');
    st.className='fb-pop-status'+(cm.resolved?' resolved':'');
    st.textContent=cm.resolved?'résolu':'à traiter';
    meta.appendChild(st);
    body.appendChild(meta);
    item.appendChild(body);
    item.addEventListener('click',function(){
      if(!current(origin))return;
      if(typeof cm.frame==='number')goToFrame(cm.frame);
      document.getElementById('fb-avatars-pop').classList.remove('open');
    });
    pop.appendChild(item);
  });
}
function toggleFbAvatarsPopover(){
  var pop=document.getElementById('fb-avatars-pop');
  if(!pop)return;
  if(pop.classList.contains('open')){pop.classList.remove('open');return;}
  renderFbAvatarPopover();
  var r=document.getElementById('fb-avatars').getBoundingClientRect();
  pop.style.top=(r.bottom+6)+'px';
  pop.style.right=(window.innerWidth-r.right)+'px';
  pop.classList.add('open');
}
function initFbAvatars(){
  var el=document.getElementById('fb-avatars');
  if(!el)return;
  el.addEventListener('click',function(e){e.stopPropagation();toggleFbAvatarsPopover();});
  document.addEventListener('click',function(e){
    var pop=document.getElementById('fb-avatars-pop');
    if(pop&&pop.classList.contains('open')&&!pop.contains(e.target))pop.classList.remove('open');
  });
  refreshFbAvatars();
}


    var api=Object.freeze({refresh:refreshFbAvatars,render:renderFbAvatarPopover,toggle:toggleFbAvatarsPopover,init:initFbAvatars});
    installed.set(root,api);return api;
  }
  function openAt(root,point,zoom){
    if(root.__TAURI__&&root.__TAURI__.core)return false; // The existing native Select bridge owns its admitted pointer port.
    var existing=(root.state.comments||[]).filter(function(cm){return cm.frame===root.state.currentFrame;})
      .find(function(cm){return point.getDistance(new root.Point(cm.x,cm.y))<14/zoom;});
    root.openCommentPopover(point,existing);return true;
  }
  function position(root,point,record){
    if(!(root.__TAURI__&&root.__TAURI__.core))return root.view.projectToView(point);
    var at=root.NemoNativeCommentsUI&&root.NemoNativeCommentsUI.screen(root,record);
    if(!at)throw new Error('Native comment origin is stale');
    var rect=root.document.getElementById('canvas-area').getBoundingClientRect();
    return {x:at.x-rect.left,y:at.y-rect.top};
  }
  return Object.freeze({create:create,openAt:openAt,position:position});
}));
