/* Presentation-only localization. Dictionary bytes remain owned by i18n.js.
 * install ports: root (SM namespace), document, state() and storage().
 * State/storage are resolved at use time; no native document port is retained.
 * Existing SM lookup/language/apply and afterI18n contracts stay unchanged.
 */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else(root.window||root).NemoI18nRuntime=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function install(ports,I18N){
    var root=ports.root,document=ports.document;
    var LANGS=['en','fr','ja','es'];
    function currentLang(){return (root.state&&ports.state().language)||'fr';}
    function t(key){
      var table=I18N[currentLang()]||I18N.fr;
      return table[key]!==undefined?table[key]:(I18N.en[key]!==undefined?I18N.en[key]:key);
    }
    function applyI18n(){
      document.querySelectorAll('[data-i18n]').forEach(function(el){el.textContent=t(el.getAttribute('data-i18n'));});
      document.querySelectorAll('[data-i18n-title]').forEach(function(el){el.title=t(el.getAttribute('data-i18n-title'));});
      document.querySelectorAll('[data-i18n-placeholder]').forEach(function(el){el.placeholder=t(el.getAttribute('data-i18n-placeholder'));});
      document.querySelectorAll('[data-i18n-html]').forEach(function(el){el.innerHTML=t(el.getAttribute('data-i18n-html'));});
      var langSel=document.getElementById('settings-language');
      if(langSel&&langSel.value!==currentLang())langSel.value=currentLang();
      document.querySelectorAll('.pl').forEach(function(el){
        if(el.getAttribute('data-i18n-title'))return;
        var txt=(el.textContent||'').trim();if(txt)el.title=txt;
      });
      var after=(root.SM&&root.SM.afterI18n)||[];
      for(var i=0;i<after.length;i++){try{after[i]();}catch(e){}}
    }
    function setLanguage(lang){
      if(LANGS.indexOf(lang)<0)return;
      ports.state().language=lang;
      try{ports.storage().setItem('nemo-lang',lang);}catch(e){}
      applyI18n();
    }
    function initLanguage(){
      var saved=null;try{saved=ports.storage().getItem('nemo-lang');}catch(e){}
      ports.state().language=(saved&&LANGS.indexOf(saved)>=0)?saved:(ports.state().language||'en');
      applyI18n();
      var sel=document.getElementById('settings-language');
      if(sel)sel.addEventListener('change',function(){setLanguage(this.value);});
    }
    if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',initLanguage);else initLanguage();
    root.SM=root.SM||{};
    root.SM.afterI18n=root.SM.afterI18n||[];
    root.SM.setLanguage=setLanguage;
    root.SM.t=t;
    root.SM.applyI18n=applyI18n;
    return Object.freeze({t:t,setLanguage:setLanguage,applyI18n:applyI18n});
  }
  return Object.freeze({install:install});
}));
