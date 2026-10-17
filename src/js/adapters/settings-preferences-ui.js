/* Settings, personal profile and shared-folder presentation; effects stay in caller ports. */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoSettingsPreferencesUI=api;}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(ports){
    var document=ports.document;
function initSettingsTabs(){
  var bar=document.getElementById('settings-tabs');if(!bar)return;
  bar.querySelectorAll('.settings-tab').forEach(function(tab){
    tab.addEventListener('click',function(){
      bar.querySelectorAll('.settings-tab').forEach(function(t){t.classList.remove('active');});
      tab.classList.add('active');
      document.querySelectorAll('#settings-modal .settings-pane').forEach(function(p){
        p.style.display=(p.dataset.pane===tab.dataset.tab)?'':'none';
      });
    });
  });
}
function initSettingsModal(){
  var btn=document.getElementById('btn-settings'),modal=document.getElementById('settings-modal');
  if(!btn||!modal)return;
  initSettingsTabs();
  btn.addEventListener('click',function(){ports.renderShortcuts();syncProfileFields();syncFolderFields();ports.refreshFeedback();ports.renderLabs();modal.style.display='flex';});
  var topBtn=document.getElementById('project-tabs-settings');
  if(topBtn)topBtn.addEventListener('click',function(){btn.click();});
  var closeBtn=document.getElementById('settings-close');
  if(closeBtn)closeBtn.addEventListener('click',function(){modal.style.display='none';});
  modal.addEventListener('click',function(e){if(e.target===modal)modal.style.display='none';});
  var resetBtn=document.getElementById('shortcuts-reset');
  if(resetBtn)resetBtn.addEventListener('click',function(){
    ports.resetShortcuts();
    ports.renderShortcuts();ports.showToast(ports.translate('toastShortcutsReset'));
  });
}
var ROLE_HINTS={
  animator:'Édite ses propres traits. Éditer le trait d\'un autre profil crée une correction à Accepter/Rejeter.',
  supervisor:'Édite n\'importe quel calque directement, sans créer de correction — autorité de révision.',
  producer:'Lecture seule + commentaires. Les outils de dessin/édition sont désactivés.'
};
function syncProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),wellEl=document.getElementById('profile-color-well'),roleEl=document.getElementById('profile-role'),hintEl=document.getElementById('profile-role-hint'),emailEl=document.getElementById('profile-email');
  if(!nameEl||!ports.readProfile())return;
  nameEl.value=ports.readProfile().name;
  if(emailEl)emailEl.value=ports.readProfile().email||'';
  var c=ports.readProfile().color;
  ports.setHex8Input(colorEl,c);
  if(wellEl)wellEl.style.background=c;
  var role=ports.readProfile().role||'animator';
  if(roleEl)roleEl.value=role;
  if(hintEl)hintEl.textContent=ROLE_HINTS[role]||'';
}
function initProfileFields(){
  var nameEl=document.getElementById('profile-name'),colorEl=document.getElementById('profile-color'),roleEl=document.getElementById('profile-role'),emailEl=document.getElementById('profile-email');
  if(!nameEl||!colorEl)return;
  syncProfileFields();
  nameEl.addEventListener('input',function(){
    ports.setProfileField('name',this.value||'Animateur');
    ports.saveProfile();
  });
  if(emailEl)emailEl.addEventListener('input',function(){
    ports.setProfileField('email',this.value||'');
    ports.saveProfile();
  });
  colorEl.addEventListener('input',function(){
    var v=this.dataset.hex8||this.value;
    ports.setProfileField('color',v);
    document.getElementById('profile-color-well').style.background=v;
    ports.saveProfile();
  });
  if(roleEl)roleEl.addEventListener('change',function(){
    ports.setProfileField('role',this.value);
    ports.saveProfile();
    var hintEl=document.getElementById('profile-role-hint');
    if(hintEl)hintEl.textContent=ROLE_HINTS[this.value]||'';
    // Switching TO producer while mid-edit with a non-view tool active would
    // leave a blocked tool selected — force back to a safe one immediately,
    // same as the boot-time role check.
    if(this.value==='producer'&&!ports.producerToolAllowed())ports.setTool('hand');
  });
  ports.wireColorSwatches([{wrap:'profile-color-well',input:'profile-color'}]);
}
function syncRelTime(ts){
  var s=Math.max(0,Math.round((ports.now()-ts)/1000));
  if(s<60)return 'il y a '+s+'s';
  var m=Math.round(s/60);
  if(m<60)return 'il y a '+m+' min';
  var h=Math.round(m/60);
  return 'il y a '+h+'h'+(m%60?Math.round(m%60)+'min':'');
}
function syncFolderFields(){
  var pathEl=document.getElementById('sync-folder-path');
  if(!pathEl)return;
  pathEl.value=ports.getSyncFolder()||'';
  var listEl=document.getElementById('sync-updates-list');
  if(listEl)listEl.innerHTML='';
}
function renderSyncUpdates(entries){
  var listEl=document.getElementById('sync-updates-list');if(!listEl)return;
  listEl.innerHTML='';
  if(!entries||!entries.length){
    listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Aucune mise à jour d\'un autre profil.</div>';
    return;
  }
  entries.forEach(function(entry){
    var row=document.createElement('div');
    row.style.cssText='display:flex;align-items:center;justify-content:space-between;gap:6px;padding:5px 6px;border-radius:4px;background:var(--panel3);font-size:10px';
    var label=document.createElement('span');
    var dot=document.createElement('span');dot.style.cssText='display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:5px;background:'+(entry.profileColor||'#888');
    label.appendChild(dot);
    label.appendChild(document.createTextNode(entry.profileName+' — '+syncRelTime(entry.ts)));
    var btn=document.createElement('button');
    btn.className='pbtn';btn.textContent=ports.translate('syncMergeBtn');btn.style.cssText='font-size:10px;padding:3px 8px';
    btn.addEventListener('click',function(){
      ports.pullAndMerge(entry).then(function(){
        btn.disabled=true;btn.textContent=ports.translate('syncMergedBtn');
      });
    });
    row.appendChild(label);row.appendChild(btn);
    listEl.appendChild(row);
  });
}
function initSyncUI(){
  var chooseBtn=document.getElementById('sync-choose-folder');
  var publishBtn=document.getElementById('sync-publish');
  var checkBtn=document.getElementById('sync-check');
  if(!chooseBtn||!publishBtn||!checkBtn)return;
  chooseBtn.addEventListener('click',function(){
    ports.chooseSyncFolder().then(function(){syncFolderFields();});
  });
  publishBtn.addEventListener('click',function(){ports.publishToShared();});
  checkBtn.addEventListener('click',function(){
    var listEl=document.getElementById('sync-updates-list');
    if(listEl)listEl.innerHTML='<div style="font-size:10px;color:var(--text-dim)">Recherche…</div>';
    ports.checkSharedUpdates().then(renderSyncUpdates);
  });
}
    return Object.freeze({initSettingsTabs:initSettingsTabs,initSettingsModal:initSettingsModal,syncProfileFields:syncProfileFields,initProfileFields:initProfileFields,syncRelTime:syncRelTime,syncFolderFields:syncFolderFields,renderSyncUpdates:renderSyncUpdates,initSyncUI:initSyncUI});
  }
  return Object.freeze({create:create});
}));
