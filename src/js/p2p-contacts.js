// ---- P2P CONTACTS (v1, 2026-07) — contact/pairing MODEL only ----
// User explicitly pointed at p2p-messenger (AlterSend, a separate P2P
// encrypted messenger project) as the reference for "add contacts to work
// on the same file P2P with remote collaborators". Researched its actual
// engine: Hyperswarm/Hypercore (the Holepunch/Pear stack), running inside
// a separate "Bare" JS runtime process that Electron/React Native spawns
// and talks to over IPC — not something a Tauri v2 webview can embed
// directly. Wiring in a REAL P2P transport is its own architecture
// decision (most likely Iroh, the natural Rust/Tauri-native analog to
// Hyperswarm) and its own multi-session chantier — explicit user decision
// after being told this: build contact management now, live network sync
// as a separate follow-up phase once that transport choice is made. NO
// data is actually exchanged between machines yet — this file only builds
// and stores the local identity + contact list.
//
// Model deliberately mirrors AlterSend's own pattern (packages/core's
// RememberedPeer), not because the code is shared (it isn't — completely
// different runtime) but because the SHAPE of the approach is sound and
// worth keeping when the real transport lands:
//   - identity = a public key, never a username/email/server account
//   - an "invite code" is an ephemeral PAIRING token, not the permanent
//     identity itself
//   - a contact is only persisted after a deliberate paste on both sides,
//     never silently — same spirit as AlterSend's mutual "remember" vote
//     before a peer becomes a permanent contact
//
// Local identity/contacts live in localStorage, NOT the project JSON —
// same reasoning and same pattern as state.userProfile (app.js's
// initUserProfile/saveUserProfile): this is per-installation identity,
// stable across every project this machine opens, not per-project data.
(function () {
  function randHex(bytes) {
    var arr = new Uint8Array(bytes);
    crypto.getRandomValues(arr);
    return Array.from(arr).map(function (b) { return b.toString(16).padStart(2, '0'); }).join('');
  }
  function initIdentity() {
    try {
      var saved = JSON.parse(localStorage.getItem('nemo-p2p-identity') || 'null');
      if (saved && saved.publicKey) { state.p2pIdentity = saved; return; }
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
    state.p2pIdentity = { publicKey: randHex(16), createdAt: Date.now() };
    try { localStorage.setItem('nemo-p2p-identity', JSON.stringify(state.p2pIdentity)); } catch (e) { }
  }
  function loadContacts() {
    try { return JSON.parse(localStorage.getItem('nemo-p2p-contacts') || '[]'); } catch (e) { return []; }
  }
  function saveContacts() {
    try { localStorage.setItem('nemo-p2p-contacts', JSON.stringify(state.p2pContacts)); } catch (e) { }
  }
  state.p2pContacts = loadContacts();
  initIdentity();

  function myDisplayName() { return (state.userProfile && state.userProfile.name) || 'Animateur'; }

  // Invite code: base64 of {k:pubkey, n:name, t:timestamp} — plain
  // copy/paste text, no QR yet (no QR library bundled in this app; a
  // scannable code can be layered on top of this same payload later
  // without changing the format). Mirrors AlterSend's OWN "or paste the
  // code" fallback alongside its QR option — text-only is a legitimate v1,
  // not a placeholder.
  function myInviteCode() {
    return btoa(JSON.stringify({ k: state.p2pIdentity.publicKey, n: myDisplayName(), t: Date.now() }));
  }
  function parseInviteCode(code) {
    try {
      var payload = JSON.parse(atob(code.trim()));
      if (!payload || !payload.k) return null;
      return { publicKey: payload.k, displayName: payload.n || 'Contact' };
    } catch (e) { return null; }
  }
  function addContact(code) {
    var parsed = parseInviteCode(code);
    if (!parsed) return { ok: false, error: 'Code invalide' };
    if (parsed.publicKey === state.p2pIdentity.publicKey) return { ok: false, error: 'C’est ton propre code' };
    if (state.p2pContacts.some(function (c) { return c.publicKey === parsed.publicKey; })) return { ok: false, error: 'Déjà dans tes contacts' };
    // status stays 'pending' forever until phase 2's real transport can
    // actually confirm a live handshake — never silently promoted to
    // "connected" by this file alone, so the UI never lies about a
    // connection that doesn't exist yet.
    var contact = { publicKey: parsed.publicKey, displayName: parsed.displayName, pairedAt: Date.now(), status: 'pending' };
    state.p2pContacts.push(contact);
    saveContacts();
    render();
    return { ok: true, contact: contact };
  }
  function removeContact(publicKey) {
    state.p2pContacts = state.p2pContacts.filter(function (c) { return c.publicKey !== publicKey; });
    saveContacts();
    render();
  }

  function render() {
    var list = document.getElementById('p2p-contacts-list');
    if (!list) return;
    list.innerHTML = '';
    if (!state.p2pContacts.length) {
      var empty = document.createElement('div');
      empty.style.cssText = 'font-size:9px;color:var(--text-dim);padding:4px 0;';
      empty.textContent = window.SM.t('p2pNoContacts');
      list.appendChild(empty);
      return;
    }
    state.p2pContacts.forEach(function (c) {
      var row = document.createElement('div');
      row.style.cssText = 'display:flex;align-items:center;gap:6px;padding:5px 6px;border-radius:5px;background:var(--panel3);';
      var dot = document.createElement('div');
      dot.style.cssText = 'width:7px;height:7px;border-radius:50%;flex-shrink:0;background:' + (c.status === 'connected' ? '#4caf50' : '#888');
      dot.title = c.status === 'connected' ? window.SM.t('p2pConnected') : window.SM.t('p2pWaiting');
      var nm = document.createElement('div');
      nm.style.cssText = 'flex:1;font-size:11px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;';
      nm.textContent = c.displayName;
      var key = document.createElement('div');
      key.style.cssText = 'font-size:8.5px;color:var(--text-dim);font-family:monospace;flex-shrink:0;';
      key.textContent = c.publicKey.slice(0, 10) + '…';
      var rm = document.createElement('button');
      rm.className = 'pbtn'; rm.textContent = '×'; rm.title = 'Retirer ce contact'; rm.style.cssText = 'flex-shrink:0;padding:2px 7px;';
      rm.addEventListener('click', function () { removeContact(c.publicKey); });
      row.appendChild(dot); row.appendChild(nm); row.appendChild(key); row.appendChild(rm);
      list.appendChild(row);
    });
  }

  function init() {
    var myCodeField = document.getElementById('p2p-my-code');
    if (myCodeField) myCodeField.value = myInviteCode();
    var copyBtn = document.getElementById('p2p-copy-code');
    if (copyBtn) copyBtn.addEventListener('click', function () {
      if (myCodeField) {
        myCodeField.select();
        if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(myCodeField.value);
        else document.execCommand('copy');
      }
      if (window.showToast) showToast(SM.t('toastCodeCopied'));
    });
    var addBtn = document.getElementById('p2p-add-contact');
    var input = document.getElementById('p2p-invite-input');
    if (addBtn && input) {
      var doAdd = function () {
        if (!input.value.trim()) return;
        var res = addContact(input.value);
        if (!res.ok) { if (window.showToast) showToast(res.error); return; }
        input.value = '';
        if (window.showToast) showToast(SM.t('toastContactAdded') + res.contact.displayName);
      };
      addBtn.addEventListener('click', doAdd);
      input.addEventListener('keydown', function (e) { if (e.key === 'Enter') doAdd(); });
    }
    render();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();

  window.SMP2P = { addContact: addContact, removeContact: removeContact, myInviteCode: myInviteCode };
})();
