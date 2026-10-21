/* Kitsu HTTP adapter only; sessions and document admission remain with the caller. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoKitsuService=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(ports){
    function available(){return ports.available();}
  async function request(session, path, opts) {
    opts = opts || {};
    var url = session.serverUrl + '/api' + path;
    var headers = Object.assign({}, opts.headers || {});
    if (session.accessToken) headers['Authorization'] = 'Bearer ' + session.accessToken;
    var res = await ports.fetch(url, {
      method: opts.method || 'GET',
      headers: headers,
      body: opts.body
    });
    if (!res.ok) {
      var text = '';
      try { text = await res.text(); } catch (e) { }
      throw new Error('Kitsu (' + res.status + ') ' + (text || res.statusText || path));
    }
    return res.json();
  }

  async function login(serverUrl, email, password) {
    if (!available()) throw new Error('Connexion Kitsu disponible uniquement dans l\'app desktop');
    var base = serverUrl.replace(/\/+$/, '');
    var form = new URLSearchParams();
    form.set('email', email);
    form.set('password', password);
    var res = await ports.fetch(base + '/api/auth/login', {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: form.toString()
    });
    if (!res.ok) throw new Error('Échec de connexion (' + res.status + ') — vérifie l\'URL, l\'email et le mot de passe');
    var data = await res.json();
    if (!data.access_token) throw new Error('Réponse de connexion inattendue (pas de token)');
    var session = {
      serverUrl: base,
      accessToken: data.access_token,
      personName: (data.user && (data.user.full_name || (data.user.first_name || '') + ' ' + (data.user.last_name || ''))) || email,
      personId: data.user && data.user.id
    };
    return session;
  }
    return Object.freeze({available:available,request:request,login:login});
  }
  function desktop(root){
    return create({
      available:function(){return typeof root.__TAURI__!=='undefined'&&!!root.__TAURI__.http;},
      fetch:function(url,options){return root.__TAURI__.http.fetch(url,options);}
    });
  }
  return Object.freeze({create:create,desktop:desktop});
}));
