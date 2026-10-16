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
  var model = window.NemoP2PContactModel.create({state: state,
    storage: {getItem: function (key) { return localStorage.getItem(key); }, setItem: function (key, value) { return localStorage.setItem(key, value); }},
    crypto: {getRandomValues: function (array) { return crypto.getRandomValues(array); }},
    now: function () { return Date.now(); },
    encodeBase64: function (value) { return btoa(value); },
    decodeBase64: function (value) { return atob(value); }});
  function myInviteCode() { return model.myInviteCode(); }
  function addContact(code) {
    var result = model.addContact(code);
    if (result.ok) render();
    return result;
  }
  function removeContact(publicKey) { model.removeContact(publicKey); render(); }

  function render() {
    var list = document.getElementById('p2p-contacts-list');
    if (!list) return;
    list.innerHTML = '';
    if (!model.contacts().length) {
      var empty = document.createElement('div');
      empty.style.cssText = 'font-size:9px;color:var(--text-dim);padding:4px 0;';
      empty.textContent = window.SM.t('p2pNoContacts');
      list.appendChild(empty);
      return;
    }
    model.contacts().forEach(function (c) {
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
      rm.className = 'pbtn'; rm.textContent = '×'; rm.title = SM.t('hsRemoveContact'); rm.style.cssText = 'flex-shrink:0;padding:2px 7px;';
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
