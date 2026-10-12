// @ts-check
// Native project-write adapter (P29): the filesystem block that
// `writeProjectTo` in project.js used to inline.
//
// The file writer owns no state and reads no globals. Everything comes in
// through explicit ports, so the write sequence — including its failure
// ordering — is exercisable headlessly against injected failures:
//
//   ports.writeTextFile(path, text) — Tauri fs.writeTextFile
//   ports.rename(from, to)          — Tauri fs.rename (needs fs:allow-rename)
//   ports.remove(path)              — Tauri fs.remove
//
// The caller keeps everything that is not the write itself: serializing the
// document, and the project state/recents/dirty/autosave updates that follow a
// successful save.
//
// ⚠️ The fallback is preserved verbatim, not endorsed. Writing to a temp
// sibling and renaming over the target is atomic at the OS level, so the
// project file is always either the old complete version or the new complete
// one. The catch branch gives that up: a direct write to PATH can leave it
// torn if the process dies mid-write. It exists because rename needs
// `fs:allow-rename` (added to capabilities 2026-07-13) and an app built before
// that — or an exotic filesystem refusing the rename — would otherwise fail
// the save outright, and a maybe-torn write still beats guaranteed data loss
// from refusing to save at all. Moving this code does NOT make the save
// atomic; that pre-existing crash-safety limitation travels with it.
//
// Two orderings in the catch branch are load-bearing and must not be "tidied":
// the temp file is removed BEFORE the direct write, and a failing remove is
// swallowed (the temp may never have been created), while a failing direct
// write propagates to the caller so the save is not silently reported as done.
var NemoProjectNativeSave = (function () {
  'use strict';

  // The in-progress sibling. Same directory as the target on purpose: rename
  // is only atomic within one filesystem.
  var TEMP_SUFFIX = '.saving';

  function tempPathFor(path) {
    return path + TEMP_SUFFIX;
  }

  // Resolves only once the bytes are in place under `path`. Rejects only if
  // the fallback direct write also fails.
  async function writeProjectFile(path, json, ports) {
    var tmp = tempPathFor(path);
    try {
      await ports.writeTextFile(tmp, json);
      await ports.rename(tmp, path);
    } catch (e) {
      try { await ports.remove(tmp); } catch (_e) {}
      await ports.writeTextFile(path, json);
    }
  }

  // Ephemeral UI attempt tickets only. Native ports remain document authority;
  // the existing P29 file publisher and its fallback are unchanged.
  function createSavedUI(ports) {
    var read=ports.read.bind(ports), serialize=ports.serialize.bind(ports);
    var publish=ports.publish.bind(ports), commit=ports.commit.bind(ports);
    var invalidate=ports.invalidate.bind(ports), pick=ports.pick.bind(ports);
    var notice=ports.notice.bind(ports), failure=ports.failure.bind(ports);
    var active=null, writing=false, dispatched=new WeakSet(), published=new WeakSet(), consumed=new WeakSet();
    function owner(native) {
      if(!native||typeof native.isActive!=='function'||native.isActive()!==true||
          typeof native.blocksLegacy!=='function'||native.blocksLegacy()!==true||
          typeof native.identity!=='function'||typeof native.getNativeIdentity!=='function')throw new Error('Native save session unavailable');
      var id=native.identity(), life=native.getNativeIdentity();
      if(!id||!life||typeof id.instanceId!=='string'||!id.instanceId||
          typeof id.documentId!=='string'||!id.documentId||life.documentId!==id.documentId||
          !Number.isSafeInteger(id.contentRevision)||id.contentRevision<0||
          !Number.isSafeInteger(life.generation)||life.generation<1)throw new Error('Native save identity unavailable');
      return {instanceId:id.instanceId,documentId:id.documentId,revision:id.contentRevision,generation:life.generation};
    }
    function sameOwner(a,b) {
      return a.native===b.native&&a.api===b.api&&a.fs===b.fs&&a.ui===b.ui&&a.tabId===b.tabId&&
        a.instanceId===b.instanceId&&a.documentId===b.documentId&&a.revision===b.revision&&a.generation===b.generation&&a.json===b.json;
    }
    function sameUI(a,b) {
      return a.path===b.path&&a.name===b.name&&a.canvasW===b.canvasW&&a.canvasH===b.canvasH&&a.fps===b.fps;
    }
    function capture() {
      var ui=Object.assign({},read()), before=owner(ui.native), json=serialize(), next=Object.assign({},read()), after=owner(next.native);
      var a=Object.assign({},ui,before,{json:json}), b=Object.assign({},next,after,{json:json});
      if(!ui.ui||typeof ui.ui!=='object'||typeof json!=='string'||!json||!sameOwner(a,b)||!sameUI(a,b))throw new Error('Native save capture changed');
      return Object.freeze(a);
    }
    function current(ticket,metadata) {
      try { var now=capture(); return active===ticket&&sameOwner(ticket.pin,now)&&(!metadata||sameUI(ticket.pin,now)); }
      catch(error) { return false; }
    }
    function result(status,written) { return Object.freeze({status:status,written:written}); }
    function announce(value,path,mode) {
      if(mode!=='write')notice(value,path,mode);
      return value;
    }
    function indeterminate(ticket,path,mode) {
      // A replacement UI is never dirtied or rolled back by the old attempt.
      if(current(ticket,false))invalidate();
      try { announce(result('written-indeterminate',true),path,mode); } catch(error) {}
      return result('written-indeterminate',true);
    }
    function begin() {
      if(writing)throw new Error('A save is already in progress');
      active=Object.freeze({pin:capture()});
      return active;
    }
    function acknowledge(ticket,path,mode) {
      if(!published.has(ticket)||consumed.has(ticket))return result('refused',published.has(ticket));
      consumed.add(ticket);
      if(!current(ticket,true))return announce(result('written-stale',true),path,mode);
      try {
        var applied=commit(path,ticket.pin.json,ticket.pin);
        if(applied!==true||!current(ticket,false))return indeterminate(ticket,path,mode);
        return announce(result('saved-current',true),path,mode);
      } catch(error) { return indeterminate(ticket,path,mode); }
    }
    async function write(path,ticket,mode) {
      ticket=ticket||begin(); mode=mode||'write';
      if(typeof path!=='string'||!path||path.indexOf('\0')!==-1)return result('refused',false);
      if(dispatched.has(ticket)||writing)return result('refused',published.has(ticket));
      if(!current(ticket,true))return announce(result('stale-before-write',false),path,mode);
      dispatched.add(ticket); writing=true;
      try {
        await publish(path,ticket.pin.json,ticket.pin);
        published.add(ticket);
        return acknowledge(ticket,path,mode);
      } finally { writing=false; }
    }
    async function run(mode,writer) {
      if(writing)return announce(result('save-in-progress',false),null,mode);
      var ticket=begin(), path=ticket.pin.path;
      if(mode==='save-as'||!path){
        path=await pick(ticket.pin);
        if(path===null||path===undefined||path===''){consumed.add(ticket);return result('cancelled',false);}
      }
      if(!current(ticket,true))return announce(result('stale-before-write',false),path,mode);
      try { return await (writer||write)(path,ticket,mode); }
      catch(error) { failure(error); throw error; }
    }
    return Object.freeze({begin:begin,write:write,acknowledge:acknowledge,
      saveAs:function(writer){return run('save-as',writer);},save:function(writer){return run('save',writer);}});
  }

  return {
    TEMP_SUFFIX: TEMP_SUFFIX,
    tempPathFor: tempPathFor,
    writeProjectFile: writeProjectFile,
    createSavedUI: createSavedUI,
  };
})();
if (typeof module !== 'undefined' && module.exports) module.exports = NemoProjectNativeSave;
