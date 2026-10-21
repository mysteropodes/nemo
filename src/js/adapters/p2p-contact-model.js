/* Local P2P identity/contact model only; no transport, handshake or document state. */
(function(root,factory){
  var api=factory();
  if(typeof module==='object'&&module.exports)module.exports=api;
  else root.NemoP2PContactModel=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  function create(ports){
    function randHex(bytes) {
      var arr = new Uint8Array(bytes);
      ports.crypto.getRandomValues(arr);
      return Array.from(arr).map(function (b) { return b.toString(16).padStart(2, '0'); }).join('');
    }
    function initIdentity() {
      try {
        var saved = JSON.parse(ports.storage.getItem('nemo-p2p-identity') || 'null');
        if (saved && saved.publicKey) { ports.state.p2pIdentity = saved; return; }
      } catch (e) { }
      // v1: a random 128-bit token stands in for a real asymmetric keypair —
      // with no live transport yet to actually verify/encrypt against, a
      // full Ed25519 identity would be unused cryptographic theater until
      // phase 2 wires in the real handshake (AlterSend derives its message
      // encryption key straight from this same identity key, see
      // packages/core/src/worklet/chat/crypto.ts — worth mirroring exactly
      // once there's a real channel to encrypt). Swap this for a real
      // keypair at that point; zero migration cost for anyone with an empty
      // contact list, since nothing depends on today's key format yet.
      ports.state.p2pIdentity = { publicKey: randHex(16), createdAt: ports.now() };
      try { ports.storage.setItem('nemo-p2p-identity', JSON.stringify(ports.state.p2pIdentity)); } catch (e) { }
    }
    function loadContacts() {
      try { return JSON.parse(ports.storage.getItem('nemo-p2p-contacts') || '[]'); } catch (e) { return []; }
    }
    function saveContacts() {
      try { ports.storage.setItem('nemo-p2p-contacts', JSON.stringify(ports.state.p2pContacts)); } catch (e) { }
    }
    ports.state.p2pContacts = loadContacts();
    initIdentity();

    function myDisplayName() { return (ports.state.userProfile && ports.state.userProfile.name) || 'Animateur'; }

    // Invite code: base64 of {k:pubkey, n:name, t:timestamp} — plain
    // copy/paste text, no QR yet (no QR library bundled in this app; a
    // scannable code can be layered on top of this same payload later
    // without changing the format). Mirrors AlterSend's OWN "or paste the
    // code" fallback alongside its QR option — text-only is a legitimate v1,
    // not a placeholder.
    function myInviteCode() {
      return ports.encodeBase64(JSON.stringify({ k: ports.state.p2pIdentity.publicKey, n: myDisplayName(), t: ports.now() }));
    }
    function parseInviteCode(code) {
      try {
        var payload = JSON.parse(ports.decodeBase64(code.trim()));
        if (!payload || !payload.k) return null;
        return { publicKey: payload.k, displayName: payload.n || 'Contact' };
      } catch (e) { return null; }
    }
    function addContact(code) {
      var parsed = parseInviteCode(code);
      if (!parsed) return { ok: false, error: 'Code invalide' };
      if (parsed.publicKey === ports.state.p2pIdentity.publicKey) return { ok: false, error: 'C’est ton propre code' };
      if (ports.state.p2pContacts.some(function (c) { return c.publicKey === parsed.publicKey; })) return { ok: false, error: 'Déjà dans tes contacts' };
      // status stays 'pending' forever until phase 2's real transport can
      // actually confirm a live handshake — never silently promoted to
      // "connected" by this file alone, so the UI never lies about a
      // connection that doesn't exist yet.
      var contact = { publicKey: parsed.publicKey, displayName: parsed.displayName, pairedAt: ports.now(), status: 'pending' };
      ports.state.p2pContacts.push(contact);
      saveContacts();
      return { ok: true, contact: contact };
    }
    function removeContact(publicKey) {
      ports.state.p2pContacts = ports.state.p2pContacts.filter(function (c) { return c.publicKey !== publicKey; });
      saveContacts();
    }

    return Object.freeze({myInviteCode:myInviteCode,parseInviteCode:parseInviteCode,
      addContact:addContact,removeContact:removeContact,
      contacts:function(){return ports.state.p2pContacts;}});
  }
  return Object.freeze({create:create});
}));
