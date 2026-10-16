/* D01/G41A: A2 owner, A1 reviewer. Injected transports and parser only.
 * Font bytes and parsed-font lifetime; never a document or glyph writer. */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NemoFontAssets = api;
}(typeof globalThis === 'object' ? globalThis : this, function () {
  'use strict';

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
    var cache = new Map();
    var active = true;
    var decode = options.decodeBase64 || decodeBase64;

    function requireActive() {
      if (!active) throw new Error('font assets disposed');
    }

    function request(key, readBytes, acceptedSpec) {
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
        if (acceptedSpec) fonts[key] = acceptedSpec;
        return font;
      }).catch(function (error) {
        if (cache.get(key) === pending) cache.delete(key);
        throw error;
      });
      cache.set(key, pending);
      return pending;
    }

    function load(key) {
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
    function googleWeight(family, weight, italic) {
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
      }, { url: null, label: family + (weight >= 600 ? ' Bold' : '') });
    }

    function addGoogle(family) {
      family = (family || '').trim();
      if (!family) return Promise.reject(new Error('empty family name'));
      return googleWeight(family, 400, false).then(function () {
        requireActive();
        googleWeight(family, 700, false).catch(function () {});
        return 'Google:' + family + '-Regular';
      });
    }

    function dispose() {
      active = false;
      cache.clear();
    }

    // A renderer may await a requested weight before its catalog entry is
    // accepted. This preserves live Bold selection without advertising it.
    function hasLoad(key) { return active && cache.has(key); }

    return Object.freeze({ fonts: fonts, load: load, googleWeight: googleWeight,
      addGoogle: addGoogle, hasLoad: hasLoad, dispose: dispose });
  }

  return Object.freeze({ create: create });
}));
