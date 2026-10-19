// Finite F03 native Save input and receipt. No path/hash from operator requests.
import Foundation
import ApplicationServices
import CryptoKit
import Darwin

let f03SavedName="F03-static-saved.project.json"
let f03ExpectedName="F03-static-before-save.drawing.json"
func f03Hash(_ bytes:Data) -> String {SHA256.hash(data:bytes).map {String(format:"%02x",$0)}.joined()}
func f03JSON(_ bytes:Data) throws -> [String:Any] {
  try check(!bytes.isEmpty && bytes.count <= 32000)
  guard let value=try JSONSerialization.jsonObject(with:bytes) as? [String:Any] else {throw Refusal(code:"adapter-refused")}
  return value
}
func f03Canonical(_ value:Any) throws -> Data {try JSONSerialization.data(withJSONObject:value,options:[.sortedKeys])}
// This is a fixed fixture/persistence oracle, not a native codec/evaluator.
func f03StaticDrawing(_ value:[String:Any]) throws {
  try check(value["format"] as? String == "nemo.native-object-document" && value["formatVersion"] as? Int == 2 && value["totalFrames"] as? Int == 21)
  guard let layers=value["layers"] as? [[String:Any]],layers.count == 1,
    layers[0]["layerUid"] as? String == "r08_curve_layer",
    let holders=value["propertyHolders"] as? [[String:Any]] else {throw Refusal(code:"adapter-refused")}
  var records=[[String:Any]]()
  for holder in holders {
    guard let target=holder["target"] as? [String:Any],let properties=holder["properties"] as? [[String:Any]] else {throw Refusal(code:"adapter-refused")}
    if target["contextId"] as? String == "scene-root" && target["layerUid"] as? String == "r08_curve_layer" && target["strokeId"] == nil {
      records += properties.filter {$0["property"] as? String == "opacity"}
    }
  }
  try check(records.count == 1)
  guard let scalar=records[0]["static"] as? [NSNumber],scalar.count == 1 else {throw Refusal(code:"adapter-refused")}
  try check(CFGetTypeID(scalar[0]) != CFBooleanGetTypeID() && scalar[0].doubleValue == 25)
  try check(records[0]["track"] == nil && records[0]["expression"] == nil)
}
func f03SavedProject(_ bytes:Data, expectedDrawing:Data) throws {
  let envelope=try f03JSON(bytes),expected=try f03JSON(expectedDrawing)
  try f03StaticDrawing(expected)
  try check(envelope["format"] as? String == "nemo.native-project-document" && envelope["formatVersion"] as? Int == 1)
  guard let drawing=envelope["drawing"] as? [String:Any],let project=envelope["project"] as? [String:Any] else {throw Refusal(code:"adapter-refused")}
  try f03StaticDrawing(drawing)
  try check(try f03Canonical(drawing) == f03Canonical(expected))
  try check(project["canvasW"] as? Int == 320 && project["canvasH"] as? Int == 180 && project["fps"] as? Int == 24 &&
    project["canvasBg"] as? String == "#ffffff" && project["waIn"] as? Int == 0 && project["waOut"] as? Int == 20)
}
// Every AX result, including optional absence, must finish the same
// post-query owner/current-context/deadline fences BEFORE status branching.
// Pure controls and the production read below call this exact predicate.
func f03ReadResult<T>(_ status:AXError, _ raw:T?, optional:Bool,
  owned:()->Bool, context:() throws -> Void) throws -> T? {
  try check(owned());try context()
  if optional && (status == .attributeUnsupported || status == .noValue) {
    try check(raw == nil);return nil
  }
  try check(status == .success);return raw
}
func f03Unique<Node>(_ rows:[Node], equal:(Node,Node)->Bool, matches:(Node) throws -> Bool) throws -> Node {
  try check(!rows.isEmpty && rows.count <= 512)
  var found=[Node]()
  for (i,node) in rows.enumerated() {
    try check(!rows.prefix(i).contains {equal($0,node)})
    if try matches(node) {found.append(node)}
  }
  try check(found.count == 1);return found[0]
}
func f03SaveDomain(_ ownSave:Int, _ serviceSave:Int, samePID:Bool) throws -> Bool {
  if samePID {try check(serviceSave == 1);return false}
  try check(ownSave >= 0 && serviceSave >= 0 && ownSave+serviceSave == 1)
  return ownSave == 1
}
func f03SaveNext(_ phase:Int, _ command:String, _ option:String?=nil, _ value:String?=nil) throws -> Int {
  try check(option == nil && value == nil)
  let commands=["f03.save.goto","f03.save.directory","f03.save.name","f03.save.confirm","f03.save.receipt"]
  try check(commands.indices.contains(phase) && commands[phase] == command);return phase+1
}
// A receipt can be minted only here, from the actual generated disk file.
// Root's saved-input UI32Fixture overload consumes it, never a caller hash.
struct F03SavedOutput {
  let url:URL,bytes:Int,sha256:String,device:UInt64,inode:UInt64,rootDevice:UInt64,rootInode:UInt64
  fileprivate init(_ url:URL, _ data:Data, _ info:stat, _ root:stat) {
    self.url=url;bytes=data.count;sha256=f03Hash(data);device=UInt64(info.st_dev);inode=UInt64(info.st_ino)
    rootDevice=UInt64(root.st_dev);rootInode=UInt64(root.st_ino)
  }
  func validate() throws {
    var root=stat();try check(lstat(url.deletingLastPathComponent().path,&root) == 0 && (root.st_mode & S_IFMT) == S_IFDIR)
    try check(UInt64(root.st_dev) == rootDevice && UInt64(root.st_ino) == rootInode)
    let (data,info)=try f03ReadSaved(url)
    try check(data.count == bytes && f03Hash(data) == sha256 && UInt64(info.st_dev) == device && UInt64(info.st_ino) == inode)
  }
}
func f03CaptureSaved(_ file:URL, expectedDrawing:Data) throws -> F03SavedOutput {
  let (data,info)=try f03ReadSaved(file);try f03SavedProject(data,expectedDrawing:expectedDrawing)
  var root=stat();try check(lstat(file.deletingLastPathComponent().path,&root) == 0 && (root.st_mode & S_IFMT) == S_IFDIR)
  let receipt=F03SavedOutput(file,data,info,root);try receipt.validate();return receipt
}
func f03ReadSaved(_ file:URL) throws -> (Data,stat) {try f03ReadFixed(file,f03SavedName)}
func f03ReadPreSaveDrawing(_ pinned:OwnedInitialWindow) throws -> Data {
  // Root writes this fixed report artifact from the actual advertised native
  // serialization reply at the pinned static25 revision, never a fixture.
  let (bytes,_)=try f03ReadFixed(pinned.physical.appendingPathComponent(f03ExpectedName),f03ExpectedName)
  try f03StaticDrawing(f03JSON(bytes));return bytes
}
fileprivate func f03ReadFixed(_ file:URL, _ name:String) throws -> (Data,stat) {
  try check(name == f03SavedName || name == f03ExpectedName)
  try check(file.isFileURL && file.lastPathComponent == name && file.standardizedFileURL == file && file.resolvingSymlinksInPath() == file)
  let fd=Darwin.open(file.path,O_RDONLY|O_NOFOLLOW|O_NONBLOCK);try check(fd >= 0);defer{Darwin.close(fd)}
  var before=stat(),after=stat(),path=stat()
  try check(fstat(fd,&before) == 0 && (before.st_mode & S_IFMT) == S_IFREG && before.st_size > 0 && before.st_size <= 32000 && before.st_uid == geteuid() && before.st_nlink == 1)
  var storage=[UInt8](repeating:0,count:Int(before.st_size));let size=storage.count
  let count=storage.withUnsafeMutableBytes {Darwin.read(fd,$0.baseAddress!,size)}
  try check(count == size && fstat(fd,&after) == 0 && lstat(file.path,&path) == 0)
  try check(before.st_dev == after.st_dev && before.st_ino == after.st_ino && before.st_size == after.st_size && before.st_mtimespec.tv_sec == after.st_mtimespec.tv_sec && before.st_mtimespec.tv_nsec == after.st_mtimespec.tv_nsec)
  try check(path.st_dev == before.st_dev && path.st_ino == before.st_ino && (path.st_mode & S_IFMT) == S_IFREG)
  return (Data(storage),after)
}
// The real single press and fault controls share the final monotonic fence.
// Identity/file proof may consume the budget; no AX effect starts afterward.
func f03BoundedPress(_ deadline:UInt64, clock:()->UInt64={DispatchTime.now().uptimeNanoseconds},
  owned:()->Bool, post:() throws -> Void) throws {
  try check(clock() < deadline);try check(owned());try check(clock() < deadline);try post()
}
final class F03OwnedSave {
  let pinned:OwnedInitialWindow,output:URL,expectedDrawing:Data
  private var reopenIssued=false
  private var domainOwner:pid_t?
  let panelProof:UI32Fixture,kernel=F03InputKernel()
  private var phase=0,oldNodes=[AXUIElement](),modal:AXUIElement?,field:AXUIElement?
  private(set) var saved:F03SavedOutput?
  init(_ pinned:OwnedInitialWindow, _ environment:[String:String], expectedDrawing:Data) throws {
    try check(environment["NEMO_UI32_FIXTURE_KIND"] == "f03-initial")
    try f03StaticDrawing(f03JSON(expectedDrawing))
    self.pinned=pinned;self.expectedDrawing=expectedDrawing
    output=pinned.physical.appendingPathComponent(f03SavedName)
    panelProof=try UI32Fixture(pinned,environment,afterDismissal:{})
    try rootProof();try absent()
  }
  func rootProof() throws {
    try verifyOwnedWindow(pinned)
    try check(pinned.physical.standardizedFileURL == pinned.physical && pinned.physical.resolvingSymlinksInPath() == pinned.physical)
    let info=try FileManager.default.attributesOfItem(atPath:pinned.physical.path)
    try check(info[.type] as? FileAttributeType == .typeDirectory && info[.systemNumber] as? NSNumber == pinned.reportIdentity[.systemNumber] as? NSNumber && info[.systemFileNumber] as? NSNumber == pinned.reportIdentity[.systemFileNumber] as? NSNumber)
    try check(output.deletingLastPathComponent() == pinned.physical && output.lastPathComponent == f03SavedName)
    try panelProof.fileProof()
  }
  func absent() throws {
    var value=stat();let status=lstat(output.path,&value),error=errno
    try check(status == -1 && error == ENOENT)
  }
  // Root consumes this ONCE to construct the ordinary fixed-file picker.
  // A new UI32Fixture saved-output overload must retain and validate the
  // actual receipt on every file proof, not accept env/request hashes.
  func takeReopenOutput() throws -> F03SavedOutput {
    do {
      try check(!kernel.failed && phase == 5 && !reopenIssued)
      reopenIssued=true;try rootProof()
      guard let saved else {throw Refusal(code:"adapter-refused")}
      try saved.validate();return saved
    }catch{kernel.stop();throw error}
  }
  func perform(_ command:String, option:String?, value:String?, afterDismissal:() throws -> Void) throws -> [String:Any] {
    try check(!kernel.failed)
    do {
      let next=try f03SaveNext(phase,command,option,value);try rootProof()
      let started=DispatchTime.now().uptimeNanoseconds
      try check(started <= UInt64.max-5_000_000_000)
      let deadline=started+5_000_000_000
      if command == "f03.save.receipt" {
        // One read, no polling or input retry. Root schedules this only after
        // the actual dialog has dismissed and application Save has settled.
        guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement] else {throw Refusal(code:"adapter-refused")}
        try check(windows.count == 1 && owns(windows[0],pinned.pid) && CFEqual(windows[0],pinned.main))
        try afterDismissal();try rootProof()
        let receipt=try f03CaptureSaved(output,expectedDrawing:expectedDrawing);saved=receipt;phase=next
        return ["inputAttempted":false,"attempted":[String](),"acknowledged":[String](),
          "savedFile":f03SavedName,"savedBytes":receipt.bytes,"savedSha256":receipt.sha256,
          "expectedDrawingSha256":f03Hash(expectedDrawing),"static25NoOpacityKeys":true,
          "note":"Actual disk persistence receipt only; reopened document/client/UI acceptance remains separate."]
      }
      try absent()
      func context() throws {try check(DispatchTime.now().uptimeNanoseconds < deadline);try rootProof();try absent();try check(DispatchTime.now().uptimeNanoseconds < deadline)}
      func panel() throws -> ([AXUIElement],pid_t) {
        try context();let (remote,owner)=try panelProof.panel()
        func saves(_ nodes:[AXUIElement], _ pid:pid_t) throws -> Int {
          var total=0
          for node in nodes {
            if try read(node,pid,kAXRoleAttribute) as? String == kAXButtonRole,
              try title(node,pid) == "Save" {total += 1}
          }
          return total
        }
        let own=owner == pinned.pid ? [] : panelProof.observedOwnedNodes
        let chosen=try f03SaveDomain(saves(own,pinned.pid),saves(remote,owner),samePID:owner == pinned.pid)
        let pid=chosen ? pinned.pid : owner,rows=chosen ? own : remote
        if let domainOwner {try check(domainOwner == pid)}else{domainOwner=pid}
        guard let focused=axElement(try read(pinned.application,pinned.pid,kAXFocusedWindowAttribute)),owns(focused,pinned.pid),
          CFEqual(focused,pinned.main) || panelProof.sheet.map({CFEqual(focused,$0)}) == true else {throw Refusal(code:"adapter-refused")}
        try context();return (rows,pid)
      }
      func bind(_ node:AXUIElement, _ owner:pid_t) throws {
        try context();try check(owns(node,owner))
        if owner == pinned.pid && panelProof.owner != pinned.pid {_ = try panelProof.filePath(node,deadline)}
      }
      func read(_ node:AXUIElement, _ pid:pid_t, _ key:String, optional:Bool=false) throws -> CFTypeRef? {
        try context();try check(owns(node,pid) && AXUIElementSetMessagingTimeout(node,0.03) == .success)
        var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
        return try f03ReadResult(status,raw,optional:optional,
          owned:{owns(node,pid)},context:context)
      }
      func supports(_ node:AXUIElement, _ pid:pid_t, _ action:String) throws -> Bool {
        try check(owns(node,pid));var raw:CFArray?
        let status=AXUIElementCopyActionNames(node,&raw);try check(status == .success);try check(owns(node,pid))
        return (raw as? [String] ?? []).contains(action)
      }
      func settable(_ node:AXUIElement, _ pid:pid_t, _ key:String) throws -> Bool {
        try check(owns(node,pid));var value=DarwinBoolean(false)
        let status=AXUIElementIsAttributeSettable(node,key as CFString,&value);try check(status == .success);try check(owns(node,pid));return value.boolValue
      }
      func title(_ node:AXUIElement, _ pid:pid_t) throws -> String {
        (try read(node,pid,kAXTitleAttribute,optional:true) as? String) ?? ""
      }
      func button(_ rows:[AXUIElement], _ pid:pid_t, _ labels:Set<String>, roles:Set<String>=[kAXButtonRole]) throws -> AXUIElement {
        try f03Unique(rows,equal:{CFEqual($0,$1)},matches:{node in
          try check(owns(node,pid))
          guard let role=try read(node,pid,kAXRoleAttribute) as? String,roles.contains(role) else {return false}
          guard try labels.contains(title(node,pid)) else {return false}
          try check(try read(node,pid,kAXEnabledAttribute) as? Bool == true)
          try check(try supports(node,pid,kAXPressAction as String));return true
        })
      }
      func publicationContext() throws {try check(DispatchTime.now().uptimeNanoseconds < deadline);try rootProof();try check(DispatchTime.now().uptimeNanoseconds < deadline)}
      func press(_ selected:AXUIElement, _ owner:pid_t, publishes:Bool=false, _ proof:() throws -> Void) throws {
        try kernel.effect(command,identity:{if publishes {try publicationContext()}else{try context()}},before:{try proof();try bind(selected,owner)},write:{
          try f03BoundedPress(deadline,owned:{owns(selected,owner)},post:{
            try check(AXUIElementPerformAction(selected,kAXPressAction as CFString) == .success)
          })
        },after:{try verifyOwnedWindow(pinned)})
      }
      let (rows,pid)=try panel()
      if command == "f03.save.goto" {
        // Prove this is the actual Save sheet before a Go-to-Folder effect.
        _ = try button(rows,pid,["Save"])
        let labels:Set<String>=["Go to Folder…","Go to Folder...","Go to Folder","Go to the Folder…","Go to the folder…","Go to the folder:"]
        let opener=try button(rows,pid,labels,roles:[kAXButtonRole,kAXMenuItemRole]);oldNodes=rows
        try press(opener,pid,{
          let (fresh,owner)=try panel();try check(owner == pid && CFEqual(try button(fresh,owner,labels,roles:[kAXButtonRole,kAXMenuItemRole]),opener))
          _ = try button(fresh,owner,["Save"])
        })
      } else if command == "f03.save.directory" {
        let modals=try rows.filter {node in
          if let root=panelProof.root,CFEqual(root,node) {return false}
          if let sheet=panelProof.sheet,CFEqual(sheet,node) {return false}
          let role=try read(node,pid,kAXRoleAttribute) as? String
          return try role == kAXSheetRole || role == kAXGroupRole && (read(node,pid,kAXSubroleAttribute,optional:true) as? String) == kAXDialogSubrole
        }
        try check(modals.count == 1 && !oldNodes.contains {CFEqual($0,modals[0])})
        let contents=try panelProof.ownedTree(modals[0],pid)
        let fields=try contents.filter {node in
          try read(node,pid,kAXRoleAttribute) as? String == kAXTextFieldRole && (try read(node,pid,kAXFocusedAttribute) as? Bool) == true
        }
        try check(fields.count == 1 && !oldNodes.contains {CFEqual($0,fields[0])})
        let labels:Set<String>=["Go to the folder:","Go to folder:","Go to Folder","Go to the folder","Go to Folder…"]
        var semantic=false
        for node in contents {
          for key in [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute] {
            if let text=try read(node,pid,key,optional:true) as? String,labels.contains(text) {semantic=true}
          }
        }
        try check(semantic);modal=modals[0];field=fields[0]
        func focusedField() throws {
          let (fresh,owner)=try panel();try check(owner == pid && fresh.contains {CFEqual($0,modals[0])})
          let inside=try panelProof.ownedTree(modals[0],owner)
          try check(inside.contains {CFEqual($0,fields[0])})
          let focused=try fresh.filter {try read($0,owner,kAXRoleAttribute) as? String == kAXTextFieldRole && (try read($0,owner,kAXFocusedAttribute) as? Bool) == true}
          try check(focused.count == 1 && CFEqual(focused[0],fields[0]));try bind(fields[0],owner)
        }
        let sub=try read(fields[0],pid,kAXSubroleAttribute,optional:true) as? String
        try check(sub == nil || sub == "" || sub == kAXUnknownSubrole)
        let facts:[String:Any]=["role":"AXTextField","textFieldKind":"plain","enabled":try read(fields[0],pid,kAXEnabledAttribute) as? Bool == true,
          "focusedMatches":true,"valueSettableQueryError":0,"valueSettable":try settable(fields[0],pid,kAXValueAttribute),
          "confirmQueryError":0,"confirmAdvertised":try supports(fields[0],pid,kAXConfirmAction as String)]
        try kernel.effect(command,identity:context,before:focusedField,write:{
          _ = try enterSheetText(facts,identity:focusedField,write:{Int(AXUIElementSetAttributeValue(fields[0],kAXValueAttribute as CFString,self.pinned.physical.path as CFString).rawValue)},
            read:{var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(fields[0],kAXValueAttribute as CFString,&raw);return (Int(status.rawValue),(raw as? String) == self.pinned.physical.path)},
            confirm:{Int(AXUIElementPerformAction(fields[0],kAXConfirmAction as CFString).rawValue)})
        },after:{try verifyOwnedWindow(pinned)})
      } else {
        if let modal {try check(!rows.contains {CFEqual($0,modal)})}
        func nameField(_ nodes:[AXUIElement], _ owner:pid_t) throws -> AXUIElement {
          try f03Unique(nodes,equal:{CFEqual($0,$1)},matches:{node in
            guard try read(node,owner,kAXRoleAttribute) as? String == kAXTextFieldRole else {return false}
            guard let label=axElement(try read(node,owner,kAXTitleUIElementAttribute,optional:true)) else {return false}
            try check(owns(label,owner) && nodes.contains {CFEqual($0,label)})
            for key in [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute] {
              if try read(label,owner,key,optional:true) as? String == "Save As:" {return true}
            }
            return false
          })
        }
        // Do not infer the current directory from an earlier Go acknowledgement.
        func directory(_ nodes:[AXUIElement], _ owner:pid_t) throws {
          _ = try f03Unique(nodes,equal:{CFEqual($0,$1)},matches:{node in
            guard try read(node,owner,kAXRoleAttribute) as? String == kAXPopUpButtonRole else {return false}
            guard let url=try read(node,owner,kAXURLAttribute,optional:true) as? URL else {return false}
            return url.isFileURL && url.standardizedFileURL == self.pinned.physical
          })
        }
        let name=try nameField(rows,pid);try directory(rows,pid)
        func sameName() throws {
          let (fresh,owner)=try panel();try check(owner == pid && CFEqual(try nameField(fresh,owner),name));try directory(fresh,owner)
          try check(try read(name,owner,kAXEnabledAttribute) as? Bool == true);try bind(name,owner)
        }
        if command == "f03.save.name" {
          try kernel.effect("save-name-focus",identity:context,before:{try sameName();try check(try settable(name,pid,kAXFocusedAttribute))},write:{
            try check(AXUIElementSetAttributeValue(name,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
          },after:{try sameName();try check(try read(name,pid,kAXFocusedAttribute) as? Bool == true)})
          try kernel.effect(command,identity:context,before:{try sameName();try check(try read(name,pid,kAXFocusedAttribute) as? Bool == true && settable(name,pid,kAXValueAttribute))},write:{
            try check(AXUIElementSetAttributeValue(name,kAXValueAttribute as CFString,f03SavedName as CFString) == .success)
          },after:{try sameName();try check(try read(name,pid,kAXFocusedAttribute) as? Bool == true);try check(try read(name,pid,kAXValueAttribute) as? String == f03SavedName)})
        } else {
          try check(command == "f03.save.confirm")
          let save=try button(rows,pid,["Save"])
          try press(save,pid,publishes:true,{
            try sameName();try check(try read(name,pid,kAXValueAttribute) as? String == f03SavedName)
            let (fresh,owner)=try panel();try check(owner == pid && CFEqual(try button(fresh,owner,["Save"]),save))
          })
        }
      }
      phase=next
      return ["command":command,"phase":phase,"attempted":kernel.attempted,"acknowledged":kernel.acknowledged,"retry":false,
        "note":"Finite native input only; actual file receipt must be captured separately."]
    } catch {kernel.stop();throw ActionRefusal(diagnostics:["purpose":"F03-save","command":command,"attempted":kernel.attempted,
      "acknowledged":kernel.acknowledged,"underlying":ui32ActionCause(error),"retry":false])}
  }
}

// Initial source path is immutable: only Save As to the new owned output.
// Any later ordinary Save needs a separately admitted generated-path stage.
func f03SaveEntryProof(_ input:F03Input, initial:Bool, generatedCurrent:Bool, savePrepared:Bool) throws {
  try check(input == .saveAs && initial && !generatedCurrent && !savePrepared)
}
// Input effects are described only by actual F03 attempt/ACK arrays; the
// ordinary runner's generic actionPosted default must never survive this join.
func f03Result(_ base:[String:Any], _ facts:[String:Any]) -> [String:Any] {
  var result=base;result.merge(facts) {_,new in new};result.removeValue(forKey:"actionPosted");return result
}
// Original identity policy is the sole ordinary/fixture/sync fallback.
func f03JoinedPhase(_ command:String?, saveActive:Bool, fixtureActive:Bool, syncActive:Bool) throws -> UI32IdentityPhase {
  if saveActive {
    try check(!fixtureActive && !syncActive)
    guard let command else {throw Refusal(code:"adapter-refused")}
    try check(["f03.save.goto","f03.save.directory","f03.save.name","f03.save.confirm","f03.save.receipt","quit"].contains(command))
    return command == "f03.save.receipt" ? .nativePickerCompletion : .nativePicker
  }
  if fixtureActive && command == "f03.file.open" {try check(!syncActive);return .nativePicker}
  return try ui32IdentityPhase(command,fixtureActive:fixtureActive,syncActive:syncActive)
}
// Thin caller join only: original UI32 resolver/picker owns all AX traversal.
// No SDK calls, activation, caller-selected files or parallel input authority.
final class F03OwnedOperator {
  let pinned:OwnedInitialWindow,environment:[String:String],kind:String
  let opacity:F03OwnedOpacity
  private var save:F03OwnedSave?,saveActive=false,intermediateOpened=false
  init(_ pinned:OwnedInitialWindow, _ environment:[String:String]) throws {
    guard let kind=environment["NEMO_UI32_FIXTURE_KIND"] else {throw Refusal(code:"adapter-refused")}
    try check(kind == "f03-initial" || kind == "f03-original")
    try check(environment["NEMO_UI32_ENTRY_MODE"] == nil)
    self.pinned=pinned;self.environment=environment;self.kind=kind
    opacity=try F03OwnedOpacity(pinned,kind)
  }
  func identityPhase(_ command:String?, fixtureActive:Bool, syncActive:Bool) throws -> UI32IdentityPhase {
    try f03JoinedPhase(command,saveActive:saveActive,fixtureActive:fixtureActive,syncActive:syncActive)
  }
  func perform(_ command:String, fixture:()->UI32Fixture?, setFixture:@escaping (UI32Fixture)->Void,
    syncActive:Bool, resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void,
    member:(AXUIElement) throws -> Void, afterDismissal:@escaping () throws -> Void) throws -> [String:Any] {
    try check(command.hasPrefix("f03.") && !syncActive)
    if command.hasPrefix("f03.save.") {
      guard saveActive,let save else {throw Refusal(code:"adapter-refused")}
      let result=try save.perform(command,option:nil,value:nil,afterDismissal:afterDismissal)
      if command == "f03.save.receipt" {saveActive=false};return result
    }
    try check(!saveActive)
    guard let current=fixture(),current.completed && current.confirmed && current.fixtureSpec.f03 else {throw Refusal(code:"adapter-refused")}
    let input=try f03Input(command,nil,nil)
    var confirmed:String?=current.fixtureSpec.name == "initial.project.json" ? "f03-initial" : "f03-original"
    var beforePress:() -> Void = {}
    var openRole:String?,openTargetHash:String?
    if input.staticStage {
      // Static40/60/history only AFTER actual saved-file Open completion.
      guard let generated=current.generatedSaved,let original=save?.saved else {throw Refusal(code:"adapter-refused")}
      try generated.validate();try original.validate()
      try check(kind == "f03-initial" && generated.url == original.url && generated.sha256 == original.sha256)
      confirmed="f03-initial"
    }
    if input == .save || input == .saveAs {
      try f03SaveEntryProof(input,initial:kind == "f03-initial" && confirmed == "f03-initial",
        generatedCurrent:current.generatedSaved != nil,savePrepared:save != nil)
      // Ordinary current UI projection is a precondition, not native evidence.
      for (action,wanted) in [(F03Input.opacity40,"25"),(.frame0,"1")] {
        let node=try resolve(f03Rule(action));try visible(node);try member(node)
        try check(owns(node,pinned.pid) && AXUIElementSetMessagingTimeout(node,0.03) == .success)
        var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,kAXValueAttribute as CFString,&raw)
        let value=try f03ReadResult(status,raw,optional:false,owned:{owns(node,self.pinned.pid)},context:{try verifyOwnedWindow(self.pinned)})
        try check(((value as? String) ?? (value as? NSNumber)?.stringValue) == wanted)
      }
      let prepared=try F03OwnedSave(pinned,environment,expectedDrawing:f03ReadPreSaveDrawing(pinned))
      save=prepared
      beforePress={self.saveActive=true}
    } else if input == .reopen {
      guard let save,let generated=save.saved else {throw Refusal(code:"adapter-refused")}
      try generated.validate()
      let replacement:UI32Fixture
      if !intermediateOpened {
        // Original installed oracle: replace with the fixed keyed document
        // first, so saved Open cannot merely reveal Save As's current tab.
        var fixed=environment;fixed["NEMO_UI32_FIXTURE_KIND"]="f03-original"
        fixed["NEMO_UI32_FIXTURE"]=pinned.physical.appendingPathComponent("candidate.project.json").path
        replacement=try UI32Fixture(pinned,fixed,afterDismissal:afterDismissal)
        openRole="intermediate-keyed"
        beforePress={self.intermediateOpened=true;replacement.attempted=true;setFixture(replacement)}
      } else {
        try check(current.generatedSaved == nil && current.fixtureSpec.name == "candidate.project.json" && current.fixtureSpec.hash == ui32F03FixtureHash)
        let actual=try save.takeReopenOutput()
        replacement=try UI32Fixture(pinned,savedOutput:actual,afterDismissal:afterDismissal)
        openRole="actual-saved"
        beforePress={replacement.attempted=true;setFixture(replacement)}
      }
      openTargetHash=replacement.fixtureSpec.hash
    }
    var result=try opacity.perform(command,option:nil,value:nil,confirmedFixtureKind:confirmed,
      resolve:resolve,visible:visible,contentMember:member,beforeMenuPress:beforePress)
    if let openRole {
      result["openRole"]=openRole;result["openSourcePreset"]=current.fixtureSpec.name;result["openTargetSha256"]=openTargetHash
      result["inputOwner"]=["pid":pinned.pid,"windowId":pinned.number,
        "processStartSeconds":pinned.start.seconds,"processStartMicroseconds":pinned.start.microseconds]
    }
    return result
  }
}
func f03Operator(_ pinned:OwnedInitialWindow, _ environment:[String:String]) throws -> F03OwnedOperator? {
  guard let kind=environment["NEMO_UI32_FIXTURE_KIND"],kind == "f03-initial" || kind == "f03-original" else {return nil}
  return try F03OwnedOperator(pinned,environment)
}
func f03IdentityPhase(_ joined:F03OwnedOperator?, _ command:String?, fixtureActive:Bool, syncActive:Bool) throws -> UI32IdentityPhase {
  if let joined {return try joined.identityPhase(command,fixtureActive:fixtureActive,syncActive:syncActive)}
  return try ui32IdentityPhase(command,fixtureActive:fixtureActive,syncActive:syncActive)
}
