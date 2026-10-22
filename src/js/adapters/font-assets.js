/* D01/G41A: A2 owner, A1 reviewer. Injected transports and parser only.
 * Font bytes and parsed-font lifetime; never a document or glyph writer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoFontAssets = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

  var catalogReader = null;
  function readCatalog() { return catalogReader ? catalogReader() : null; }

  function decodeBase64(base64) {
    var binary = atob(base64);
    var bytes = new Uint8Array(binary.length);
    for (var i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes.buffer;
  }

  // Transports and the retained opentype parser are supplied by the caller.
  // This cache is an asset cache, not native font persistence (D01/G27).
  function create(options) {
    var fonts = options.fonts;
    var sources = new Map();
    Object.keys(options.initialSources || {}).forEach(function(key) {
      if (!Object.prototype.hasOwnProperty.call(fonts, key)) return;
      var source = options.initialSources[key];
      if (!source || typeof source !== 'object' || Array.isArray(source)) return;
      var keys = Object.keys(source);
      var bundled = source.kind === 'bundled' && keys.length === 2 &&
        keys.every(function(k) { return ['kind','fontKey'].includes(k); }) && typeof source.fontKey === 'string';
      var google = source.kind === 'google' && keys.length === 4 &&
        keys.every(function(k) { return ['kind','family','weight','italic'].includes(k); }) && typeof source.family === 'string' &&
        Number.isInteger(source.weight) && source.weight >= 1 && source.weight <= 1000 && typeof source.italic === 'boolean';
      if (bundled || google) sources.set(key, Object.assign({}, source));
    });
    function catalog() {
      if (!active) return null;
      return Object.freeze(Object.keys(fonts).map(function(key) {
        var source = sources.get(key);
        return Object.freeze({key:key,label:fonts[key].label,source:source ? Object.freeze(Object.assign({},source)) : null});
      }));
    }
    function bindCatalog() { requireActive(); catalogReader = catalog; }
    var cache = new Map();
    var active = true;
    var nativeResources = null;
    function native(){if(!nativeResources)nativeResources=options.nativeResources();return nativeResources;}
    var nativeOwner=null,nativePending=new Map();
    function nativeLoad(source,key,spec,signal){
      return Promise.resolve().then(function(){
        requireActive();if(signal&&signal.aborted)throw new Error('Native font selection cancelled');var resources=native(),pin=resources.ownerKey();
        if(pin!==nativeOwner){nativePending.clear();nativeOwner=pin;}
        var token=JSON.stringify(source);
        if(nativePending.has(token))return nativeReuse(nativePending.get(token),pin,signal);
        var pending=resources.importSource(source,signal).then(function(value){
          requireActive();if(resources.ownerKey()!==pin||signal&&signal.aborted)throw new Error('Native font selection changed');
          if(spec){fonts[key]=spec;sources.set(key,Object.assign({},source));}return value;
        }).catch(function(error){if(error.code!=='font_import_indeterminate'&&nativePending.get(token)===pending)nativePending.delete(token);throw error;});
        nativePending.set(token,pending);return pending.then(function(value){return value.font;});
      });
    }
    function nativeReuse(pending,pin,signal){
      function selection(){requireActive();if(signal&&signal.aborted)throw new Error('Native font selection cancelled');}selection();
      return pending.then(function(value){selection();return native().loadAsset(value.assetId);}).then(function(font){
        requireActive();if(native().ownerKey()!==pin||signal&&signal.aborted)throw new Error('Native font selection changed');return font;
      });
    }
    var decode = options.decodeBase64 || decodeBase64;

    function requireActive() {
      if (!active) throw new Error('font assets disposed');
    }

    function request(key, readBytes, acceptedSpec, source) {
      if (!active) return Promise.reject(new Error('font assets disposed'));
      if (cache.has(key)) return cache.get(key);
      var pending = Promise.resolve().then(function () {
        requireActive();
        return readBytes();
      }).then(function (bytes) {
        requireActive();
        return options.parse(bytes);
      }).then(function (font) {
        requireActive();
        if (!font) throw new Error('font parser returned no font');
        // A failed or disposed load must never advertise an accepted font.
        if (acceptedSpec) { fonts[key] = acceptedSpec; sources.set(key, Object.assign({}, source)); }
        return font;
      }).catch(function (error) {
        if (cache.get(key) === pending) cache.delete(key);
        throw error;
      });
      cache.set(key, pending);
      return pending;
    }

    function load(key, signal) {
      if(options.nativeResources){
        if(!active)return Promise.reject(new Error('font assets disposed'));
        if(key.indexOf('Google:')===0){
          return Promise.resolve().then(function(){var pin=native().ownerKey();if(pin!==nativeOwner)throw new Error('Native Google font binding requires rediscovery');
            var entries=Array.from(nativePending.entries()).filter(function(entry){var source=JSON.parse(entry[0]);return source.kind==='google'&&'Google:'+source.family+'-'+(source.weight>=600?'Bold':'Regular')===key;});
            if(entries.length!==1)throw new Error('Native Google font binding is ambiguous or unavailable');return nativeReuse(entries[0][1],pin,signal);});
        }
        if(!Object.prototype.hasOwnProperty.call(fonts,key))return Promise.reject(new Error('unknown vector font '+key));
        return nativeLoad({kind:'bundled',fontKey:key},key,null,signal);
      }
      if (!active) return Promise.reject(new Error('font assets disposed'));
      if (cache.has(key)) return cache.get(key);
      var spec = Object.prototype.hasOwnProperty.call(fonts, key) && fonts[key];
      if (!spec || typeof spec.url !== 'string') {
        return Promise.reject(new Error('unknown vector font ' + key));
      }
      return request(key, function () {
        return options.fetch(spec.url).then(function (response) { return response.arrayBuffer(); });
      });
    }

    // Same desktop command and web Worker as the retained bridge. Desktop
    // failure is reported; it must not silently retry through another route.
    function googleWeight(family, weight, italic, signal) {
      if(options.nativeResources){
        if(!active)return Promise.reject(new Error('font assets disposed'));
        return nativeLoad({kind:'google',family:family,weight:weight,italic:!!italic},
          'Google:'+family+'-'+(weight>=600?'Bold':'Regular'),{url:null,label:family+(weight>=600?' Bold':'')},signal);
      }
      var key = 'Google:' + family + '-' + (weight >= 600 ? 'Bold' : 'Regular');
      return request(key, function () {
        if (options.isDesktop()) {
          return options.invoke('fetch_google_font', { family: family, weight: weight, italic: !!italic })
            .then(decode);
        }
        var query = 'family=' + encodeURIComponent(family) + '&weight=' + weight + (italic ? '&italic=1' : '');
        return options.fetch('/api/google-font?' + query).then(function (response) {
          if (!response.ok) throw new Error('Google Fonts: family not found or fetch failed');
          return response.arrayBuffer();
        });
      }, { url: null, label: family + (weight >= 600 ? ' Bold' : '') }, {kind:'google',family:family,weight:weight,italic:!!italic});
    }

    function addGoogle(family) {
      family = options.nativeResources ? (family || '') : (family || '').trim();
      if (!family) return Promise.reject(new Error('empty family name'));
      return googleWeight(family, 400, false).then(function () {
        requireActive();
        if(!options.nativeResources)googleWeight(family, 700, false).catch(function () {});
        return 'Google:' + family + '-Regular';
      });
    }

    function dispose() {
      active = false;
      if (catalogReader === catalog) catalogReader = null;
      sources.clear();
      cache.clear();
      nativePending.clear();
      if(nativeResources)nativeResources.dispose();
    }

    // A renderer may await a requested weight before its catalog entry is
    // accepted. This preserves live Bold selection without advertising it.
    function hasLoad(key) {
      if(options.nativeResources){try{return active&&native().ownerKey()===nativeOwner&&Array.from(nativePending.keys()).some(function(token){
        var source=JSON.parse(token);return (source.kind==='bundled'?source.fontKey:'Google:'+source.family+'-'+(source.weight>=600?'Bold':'Regular'))===key;
      });}catch(_){return false;}}
      return active && cache.has(key);
    }

    return Object.freeze({ fonts: fonts, readCatalog: catalog, bindCatalog: bindCatalog, load: load, googleWeight: googleWeight,
      addGoogle: addGoogle, hasLoad: hasLoad,
      importNativeSource: function(source, signal){if(!active)return Promise.reject(new Error('font assets disposed'));if(!options.nativeResources)return Promise.reject(new Error('Native font resources unavailable'));return native().importSource(source,signal);},
      discoverNative: function(){return native().discover();},
      loadNativeAsset: function(assetId){return native().loadAsset(assetId);}, dispose: dispose });
  }

  return Object.freeze({ create: create, readCatalog: readCatalog });
}));
