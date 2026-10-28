// UI32-only persistent runner. No keyboard, coordinates, arbitrary route or product API.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

// Finite fixture admission belongs to this runner's entry policy. Every
// caller selects one fixed source artifact; picker/file ownership stays separate.
let ui32FixtureHash="642bc4c681373c9a4e7ab3c1795f024ee1c7f277ac255298c5fdf8e1f57c8831"
let ui32F03FixtureHash="7fba834f64254aa27b828e0fc0fb7e2715977aea282a2b188c09782cb258bed2"
let ui32F03InitialFixtureHash="f3d471501a1e0fcfdd8add247b1e63f363541b025e72db3fa5ef5efb308fcddd"
func ui32FixedFixture(_ raw:String?) throws -> (name:String,size:Int,hash:String,f03:Bool) {
  if raw == nil || raw == "initial-text" { return ("Initial-text.json",7791,ui32FixtureHash,false) }
  if raw == "f03-original" { return ("candidate.project.json",7744,ui32F03FixtureHash,true) }
  if raw == "f03-initial" { return ("initial.project.json",5721,ui32F03InitialFixtureHash,true) }
  if raw == "camera-view" {return ui32CameraFixture()}
  throw Refusal(code:"adapter-refused")
}
func ui32UsesF03Fixture(_ raw:String?) throws -> Bool {try ui32FixedFixture(raw).f03}
func ui32FixedFixtureSelfTest() throws {
  try check(try ui32UsesF03Fixture(nil) == false)
  try check(try ui32UsesF03Fixture("initial-text") == false)
  try check(try ui32UsesF03Fixture("f03-original") == true)
  try check(try ui32UsesF03Fixture("f03-initial") == true)
  let initial=try ui32FixedFixture("f03-initial"),terminal=try ui32FixedFixture("f03-original"),text=try ui32FixedFixture(nil)
  try check(initial.name == "initial.project.json" && initial.size == 5721 && initial.hash == ui32F03InitialFixtureHash)
  try check(terminal.name == "candidate.project.json" && terminal.size == 7744 && terminal.hash == ui32F03FixtureHash)
  try check(text.name == "Initial-text.json" && text.size == 7791 && text.hash == ui32FixtureHash)
  for unknown in ["","Initial-text","f03","F03-initial","f03-initial ","/private/tmp/candidate.project.json","other"] {
    var refused=false;do { _ = try ui32UsesF03Fixture(unknown) } catch { refused=true };try check(refused)
  }
}

func runUI32(_ args:[String], _ environment:[String:String]) throws {
  let pinned:OwnedInitialWindow
  do { pinned=try discoverOwnedInitialWindow(args,environment,scopeCheck:ui32Scope) }
  catch let error as OwnedDiscoveryRefusal {
    var facts=error.diagnostics; facts["purpose"]="UI32"; facts["schema"]="nemo.ui32-discovery/1"
    throw ActionRefusal(diagnostics:facts)
  }
  let pid=pinned.pid, main=pinned.main
  var ordinal=0, originals=[String:String](), observed=false, menuOpened=false, phase2ReadUsed=false
  var currentCommand:String?,cs4KeysUsed=false
  let entryEnabled=try ui32EntryMode(environment["NEMO_UI32_ENTRY_MODE"]);var entryStage=0,sourceCaptureStage = -1,newSettingsOpen=false
  var fixture:UI32Fixture?, syncPanel:UI32Fixture?
  let f03=try f03Operator(pinned,environment)
  let camera=UI32CameraOperator(pinned)
  var retainedWebArea:AXUIElement?, latest:UI32ContentCertificate<AXUIElement>?
  func nativeIdentity() throws { try verifyOwnedWindow(pinned) }
  func identity() throws {
    try nativeIdentity()
    let phase=try f03IdentityPhase(f03,currentCommand,
      fixtureActive:fixture?.attempted == true && fixture?.completed != true,
      syncActive:syncPanel?.attempted == true && syncPanel?.completed != true)
    // Native picker commands prove their existing attached owner/context before
    // reads/effects; completion re-proves absence and full content after dismissal.
    if phase != .content { return }
    guard let web=retainedWebArea else { return }
    var current=web,seen=[AXUIElement]()
    for _ in 0..<13 {
      try ui32ProofCheck(owns(current,pid) && !seen.contains {CFEqual($0,current)},"retained-web-owner-cycle")
      try ui32ProofCheck(AXUIElementSetMessagingTimeout(current,0.03) == .success,"retained-web-timeout");seen.append(current)
      if CFEqual(current,main) { return }
      guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pid) else { throw Refusal(code:"child-owner") }
      let (query,rows)=ui32Children(parent,pid)
      try ui32ProofCheck(query == 0 && rows != nil && rows!.contains {CFEqual($0,current)},"retained-web-children",["queryError":query,"childCount":rows?.count ?? -1]);current=parent
    }
    throw Refusal(code:"adapter-refused")
  }
  func text(_ node:AXUIElement, _ key:String) throws -> String? {
    try ui32DiagnosticText(node,pid,key)
  }
  func parent(_ node:AXUIElement) -> (Int,AXUIElement?) {
    var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
    return (Int(query.rawValue),axElement(raw))
  }
  func owner(_ node:AXUIElement) -> (Int,Bool,Int) {
    var actual:pid_t=0;let query=AXUIElementGetPid(node,&actual)
    if query == .success && actual == pid && AXUIElementSetMessagingTimeout(node,0.03) != .success { return (-1,false,Int(actual)) }
    return (Int(query.rawValue),query == .success && actual == pid,Int(actual))
  }
  func strictChildren(_ node:AXUIElement) -> (Int,[AXUIElement]?) {
    let (query,rows)=ui32Children(node,pid)
    guard query == 0,let rows else { return (-1,nil) }
    for child in rows {
      guard owns(child,pid),AXUIElementSetMessagingTimeout(child,0.03) == .success,
        attr(child,kAXRoleAttribute) as? String != kAXWindowRole else { return (-1,nil) }
    }
    return (0,rows)
  }
  func readContentCertificate(_ suppliedDeadline:UInt64?, _ expected:AXUIElement?) throws -> UI32ContentCertificate<AXUIElement> {
    _ = try ui32ContentWindowContext(pinned)
    let deadline=suppliedDeadline ?? (DispatchTime.now().uptimeNanoseconds+750_000_000)
    let certificate=try ui32ContentCertificate(main,pid:Int(pid),retained:expected,deadline:deadline,
      now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:owner,
      prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},traits:{ node in
        guard let role=try text(node,kAXRoleAttribute) else {throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"content-role-missing"])}
        var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,kAXModalAttribute as CFString,&raw)
        var modal=false
        if query == .success {guard let value=raw as? Bool else {throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"content-modal-shape"])};modal=value}
        else {try ui32ProofCheck(query == .attributeUnsupported || query == .noValue,"content-modal-query",["queryError":Int(query.rawValue)])}
        let subrole=try text(node,kAXSubroleAttribute)
        return (role,modal || subrole == kAXDialogSubrole)
      },children:{ui32Children($0,pid)},parent:parent,strict:{ web,deadline in
        try ui32ContentWalk(web,owner:Int(pid),deadline:deadline,now:{DispatchTime.now().uptimeNanoseconds},
          equal:{CFEqual($0,$1)},owner:owner,children:strictChildren,parent:parent)
      })
    try ui32ContentReadStep("tree-final-native-identity"){try nativeIdentity()};return certificate
  }
  func publishContent(_ certificate:UI32ContentCertificate<AXUIElement>) -> [AXUIElement] {
    retainedWebArea=certificate.nodes[certificate.web];latest=certificate
    return ui32ContentMembers(certificate)
  }
  func tree(_ suppliedDeadline:UInt64? = nil) throws -> [AXUIElement] {
    try ui32ContentReadStep("tree-identity"){try identity()}
    return publishContent(try readContentCertificate(suppliedDeadline,retainedWebArea))
  }
  func newContext(_ command:String) throws {
    guard entryEnabled else {return};_ = try tree()
    guard let certificate=latest else {throw Refusal(code:"adapter-refused")}
    try ui32NewContextProof(command,entryStage,sourceCaptureStage,newSettingsOpen,certificate,pid)
  }
  func labels(_ node:AXUIElement) -> Set<String> {
    Set([kAXTitleAttribute,kAXDescriptionAttribute,kAXHelpAttribute,kAXValueAttribute].compactMap { attr(node,$0) as? String })
  }
  func ancestors(_ node:AXUIElement) throws -> Set<String> {
    var result=Set<String>(), current=node, visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains { CFEqual($0,current) }); visited.append(current)
      if let id=try ui32DOMIdentifiers(current,pid).resolved { result.insert(id) }
      if CFEqual(current,main) { return result }
      guard let parent=axElement(attr(current,kAXParentAttribute)), owns(parent,pid),
        let siblings=attr(parent,kAXChildrenAttribute) as? [AXUIElement], siblings.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"child-owner") }
      current=parent
    }
    throw Refusal(code:"adapter-refused")
  }
  func match(_ rule:UI32Rule, _ node:AXUIElement) throws -> Bool {
    try check(owns(node,pid))
    if rule.operation == "filter-press" || rule.labels == Set(["Settings","Réglages","設定","Ajustes"]) {
      var current=node,proved=false
      let modes:Set<String>=["all","animated","modified","expressions","errors","effects","mattes"]
      for _ in 0..<4 {
        try check(owns(current,pid))
        guard let parent=axElement(attr(current,kAXParentAttribute)), owns(parent,pid),let rows=ownedAXChildren(parent,expectedOwner:Int(pid)).values,rows.count <= 64 else { break }
        var found=Set<String>(), ids=Set<String>()
        for row in rows {
          try check(owns(row,pid))
          if let id=try ui32DOMIdentifiers(row,pid).resolved { ids.insert(id) }
          found.formUnion(labels(row).intersection(modes))
          guard let cells=ownedAXChildren(row,expectedOwner:Int(pid)).values,cells.count <= 16 else { throw Refusal(code:"children-unavailable") }
          for cell in cells { try check(owns(cell,pid));found.formUnion(labels(cell).intersection(modes)) }
        }
        if rule.operation == "filter-press" { proved = rows.count == 7 && found == modes }
        else { proved = ids.contains("ctx-export") && ids.contains("ctx-render-manager") }
        if proved { break };current=parent
      }
      if !proved { return false }
    }
    if rule.region == "labs-float-params" {
      guard let parent=axElement(attr(node,kAXParentAttribute)), owns(parent,pid),let siblings=ownedAXChildren(parent,expectedOwner:Int(pid)).values,siblings.count <= 8 else { return false }
      var stepLabel=false, numbers=0
      for sibling in siblings {
        try check(owns(sibling,pid))
        if !labels(sibling).isDisjoint(with:["Step","Pas","間隔","Paso"]) { stepLabel=true }
        if attr(sibling,kAXRoleAttribute) as? String == kAXTextFieldRole { numbers += 1 }
      }
      if !stepLabel || numbers != 1 { return false }
    }
    return ui32Matches(rule,identifier:try ui32DOMIdentifiers(node,pid).resolved ?? "",labels:labels(node),
      role:attr(node,kAXRoleAttribute) as? String ?? "",ancestors:try ancestors(node))
  }
  func signature(_ rule:UI32Rule, _ node:AXUIElement) throws -> Bool {
    let role=try ui32ContentReadStep("resolve-role"){guard let role=try text(node,kAXRoleAttribute) else {throw Refusal(code:"adapter-refused")};return role}
    if !rule.roles.contains(role) { return false }
    if let id=rule.identifier,try ui32DOMIdentifiers(node,pid).resolved != id { return false }
    if rule.labels.isEmpty { return true }
    var names=Set<String>()
    for key in [kAXTitleAttribute,kAXDescriptionAttribute,kAXHelpAttribute,kAXValueAttribute] {
      if let value=try text(node,key) { names.insert(value) }
    }
    return !names.isDisjoint(with:rule.labels)
  }
  func resolve(_ rule:UI32Rule) throws -> AXUIElement {
    _ = try ui32ContentReadStep("resolve-tree"){try tree()};let certificate=try ui32ContentReadStep("resolve-certificate"){guard let certificate=latest else {throw Refusal(code:"adapter-refused")};return certificate}
    var inside=[Int](),outside=[Int]()
    for i in certificate.nodes.indices {
      let node=certificate.nodes[i]
      if try ui32ContentReadStep("resolve-signature",{try signature(rule,node)}) {
        if certificate.inside.contains(i) { if try ui32ContentReadStep("resolve-match",{try match(rule,node)}) {inside.append(i)} }
        else { outside.append(i) }
      }
    }
    return certificate.nodes[try ui32ContentReadStep("resolve-candidate-unique"){try ui32ScopedCandidate(inside,outside)}]
  }
  func motionContent(_ focused:AXUIElement) throws {
    _ = try tree();guard let certificate=latest else { throw Refusal(code:"adapter-refused") }
    try ui32MotionContent(certificate,focused,equal:{CFEqual($0,$1)},identifier:{try ui32DOMIdentifiers($0,pid).resolved})
    guard let current=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(current,pid),
      CFEqual(current,focused) else { throw Refusal(code:"adapter-refused") }
  }
  func enabledVisible(_ node:AXUIElement) throws {
    try check(owns(node,pid) && attr(node,kAXEnabledAttribute) as? Bool == true)
    guard let p=xy(node), let size=wh(node), let origin=xy(main), let extent=wh(main) else { throw Refusal(code:"adapter-refused") }
    let rectangle=CGRect(origin:p,size:size), window=CGRect(origin:origin,size:extent)
    try check(size.width > 0 && size.height > 0 && window.contains(rectangle))
  }
  func f03Member(_ node:AXUIElement) throws {
    let current=try tree();try check(owns(node,pid) && current.contains {CFEqual($0,node)})
  }
  try identity()
  try emit(["status":"pinned","purpose":"UI32","pid":pid,"windowId":pinned.number,
    "processStartSeconds":pinned.start.seconds,"processStartMicroseconds":pinned.start.microseconds,"matchedAX":true])
  while let line=readLine() {
    try check(line.utf8.count <= 2048 && ordinal < 256)
    guard let data=line.data(using:.utf8), let request=try JSONSerialization.jsonObject(with:data) as? [String:Any],
      Set(request.keys).isSubset(of:["purpose","pid","windowId","command","option","value"]),
      request["purpose"] as? String == "UI32", request["pid"] as? Int32 == pid,
      request["windowId"] as? UInt32 == pinned.number, let command=request["command"] as? String else { throw Refusal(code:"adapter-refused") }
    currentCommand=command
    let nextEntryStage=try ui32EntryNext(command,entryEnabled,entryStage); ordinal += 1; try identity()
    var result:[String:Any]=["purpose":"UI32","pid":pid,"windowId":pinned.number,"ordinal":ordinal,"command":command,"status":"ok","actionPosted":false]
    if command == "quit" { try check(request["option"] == nil && request["value"] == nil); try emit(result); return }
    if command == "observe" {
      try check(request["option"] == nil && request["value"] == nil)
      let nodes:[AXUIElement]
      if !observed && ordinal == 1 && entryStage == 0 && fixture == nil && syncPanel == nil {
        let ready=try ui32InitialOwnedReadiness(first:true,now:{DispatchTime.now().uptimeNanoseconds},identity:identity,
          pause:{Thread.sleep(forTimeInterval:Double($0)/1_000_000_000)},read:{try tree($0)})
        nodes=ready.value;result["initialReadiness"]=ready.proof
      } else {
        let p=try ui32Phase2Read(entryEnabled,entryStage,sourceCaptureStage,newSettingsOpen,&phase2ReadUsed,identity:{try ui32ContentReadStep("tree-identity"){try identity()}},read:{try readContentCertificate($0,retainedWebArea)})
        nodes=publishContent(p.value);if let proof=p.proof {result["phase2Readiness"]=proof}
      }
      var facts=[[String:Any]]()
      for node in nodes {
        try check(owns(node,pid))
        let role=attr(node,kAXRoleAttribute) as? String ?? "", ids=try ui32DOMIdentifiers(node,pid)
        let rawValue=attr(node,kAXValueAttribute)
        let value=rawValue as? String
        var fact:[String:Any]=["role":role,"identifier":String((ids.native ?? "").prefix(100)),"labels":labels(node).map {String($0.prefix(240))}.sorted()]
        fact["identifiers"]=ids.observation
        if let value { fact["value"]=String(value.prefix(240)) }
        else if let numeric=rawValue as? NSNumber { fact["value"]=numeric }
        if let enabled=attr(node,kAXEnabledAttribute) as? Bool { fact["enabled"]=enabled }
        if let selected=attr(node,kAXSelectedAttribute) as? Bool { fact["selected"]=selected }
        if let focused=attr(node,kAXFocusedAttribute) as? Bool { fact["focused"]=focused }
        facts.append(fact)
      }
      try identity()
      if let certificate=latest {
        result["contentCertificate"]=["ownedWindowNodeCount":certificate.nodes.count,"webAreaCount":1,
          "contentNodeCount":certificate.inside.count,"strictContentReciprocity":true,
          "outsideMismatches":certificate.outsideMismatches.map { i -> [String:Any] in
            ["childIndex":i,"childRole":certificate.roles[i],"declaredParentIndex":certificate.declared[i]!,
             "declaredParentRole":certificate.roles[certificate.declared[i]!],"reportedParentIndex":certificate.reported[i]!,
             "reportedParentRole":certificate.roles[certificate.reported[i]!],"insideContent":false]
          },"wholeWindowReciprocity":"not claimed; outside mismatches retained"]
      }
      observed=true; result["nodes"]=facts; result["baselineObserved"]=true; try emit(result); continue
    }
    if command == "capture" {
      try check(request["value"] == nil)
      guard let name=request["option"] as? String, name.range(of:"^capture-[1-9][0-9]{0,2}\\.png$",options:.regularExpression) != nil else { throw Refusal(code:"adapter-refused") }
      _ = try tree()
      let now=try FileManager.default.attributesOfItem(atPath:pinned.physical.path)
      try check(now[.systemNumber] as? NSNumber == pinned.reportIdentity[.systemNumber] as? NSNumber &&
        now[.systemFileNumber] as? NSNumber == pinned.reportIdentity[.systemFileNumber] as? NSNumber)
      let target=pinned.physical.appendingPathComponent(name); try check(!FileManager.default.fileExists(atPath:target.path))
      let child=Process(); child.executableURL=URL(fileURLWithPath:"/usr/sbin/screencapture"); child.arguments=["-x","-o","-l"+String(pinned.number),target.path]
      let exited=DispatchSemaphore(value:0); child.terminationHandler={ _ in exited.signal() }
      try child.run(); let childPID=child.processIdentifier
      let outcome=superviseBoundedWorker(deadline:DispatchTime.now().uptimeNanoseconds+2_000_000_000,
        now:{DispatchTime.now().uptimeNanoseconds},waitFor:{ remaining in
          guard exited.wait(timeout:.now() + .nanoseconds(Int(min(remaining,UInt64(Int.max))))) == .success else { return .timedOut }
          return child.terminationReason == .exit && child.terminationStatus == 0 ? .completed(Data()) : .failed
        },terminateAndCollect:{
          if child.isRunning { child.terminate() };return exited.wait(timeout:.now() + .milliseconds(80)) == .success
        },killAndCollect:{
          if child.isRunning { _ = Darwin.kill(childPID,SIGKILL) };return exited.wait(timeout:.now() + .milliseconds(80)) == .success
        })
      result["captureChildPID"]=childPID
      switch outcome {
      case .completed: result["captureChildReaped"]=true
      case .failed: throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"capture-child-exit","childPID":childPID,"cleanupComplete":true])
      case .deadline(let complete): throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"capture-child-deadline","childPID":childPID,"cleanupComplete":complete])
      }
      try identity()
      if entryEnabled {sourceCaptureStage=entryStage}
      result["capture"]=name; try emit(result); continue
    }
    try ui32RequireObserved(observed)
    if command == "cs4.start-keys" {
      try cs7StartAdmission(command,entry:entryEnabled,fixture:fixture != nil,sync:syncPanel != nil,
        f03:f03 != nil,used:cs4KeysUsed,option:request["option"],value:request["value"])
      cs4KeysUsed=true
      result.merge(try cs7StartKeys(pinned,resolve:resolve,visible:enabledVisible)) {_,new in new}
      try emit(result);continue
    }
    if command.hasPrefix("f03.") {
      try check(request["option"] == nil && request["value"] == nil)
      guard let f03 else {throw Refusal(code:"adapter-refused")}
      result=f03Result(result,try f03.perform(command,fixture:{fixture},setFixture:{fixture=$0},
        syncActive:syncPanel?.attempted == true && syncPanel?.completed != true,
        resolve:resolve,visible:enabledVisible,member:f03Member,afterDismissal:{_ = try tree()}))
      try emit(result);continue
    }
    if command == "selection.toggle.B" {
      try check(request["option"] == nil && request["value"] == nil && fixture?.completed == true && fixture?.confirmed == true)
      _ = try tree()
      result.merge(try ui32ToggleB(pinned,resolve:resolve,visible:enabledVisible)) { _,new in new };try emit(result);continue
    }
    if command.hasPrefix("camera.") {
      try check(request["option"] == nil && request["value"] == nil && !entryEnabled && f03 == nil &&
        environment["NEMO_UI32_FIXTURE_KIND"] == "camera-view")
      guard let loaded=fixture,loaded.completed && loaded.confirmed else {throw Refusal(code:"adapter-refused")}
      result.merge(try camera.perform(command,resolve:resolve,visible:enabledVisible,certificate:{
        _ = try tree();guard let current=latest else {throw Refusal(code:"adapter-refused")};return current
      },fixture:{try loaded.fileProof()})) {_,new in new};try emit(result);continue
    }
    if ["motion.focus","motion.shortcut","motion.guard-typing"].contains(command) {
      try check(request["value"] == nil)
      _ = try tree()
      result.merge(try runUI32Keys(command,request["option"] as? String,pinned,resolve:resolve,visible:enabledVisible,contentScope:motionContent)) { _,new in new }
      try emit(result); continue
    }
    if ["sync.panel-observe","sync.cancel-only","sync.complete"].contains(command) {
      try check(request["option"] == nil && request["value"] == nil)
      guard let syncPanel else { throw Refusal(code:"adapter-refused") }
      result.merge(try syncPanel.perform(command)) { _,new in new };try emit(result);continue
    }
    if command == "sync.open-cancel" {
      try check(syncPanel == nil);syncPanel=try UI32Fixture(pinned,environment,cancelOnly:true,afterDismissal:{ _ = try tree() })
    }
    if command.hasPrefix("fixture.") && command != "fixture.open" {
      try check(request["option"] == nil && request["value"] == nil)
      guard let fixture else { throw Refusal(code:"adapter-refused") }
      result.merge(try fixture.perform(command)) { _,new in new }; try emit(result); continue
    }
    if command == "fixture.open" {
      try ui32ContentReadStep("fixture-open-arguments"){try check(request["option"] == nil && request["value"] == nil && fixture == nil)}
      fixture=try ui32ContentReadStep("fixture-open-init"){try UI32Fixture(pinned,environment,afterDismissal:{ _ = try tree() })}
    }
    let restoring=command.hasSuffix(".restore"), base=restoring ? String(command.dropLast(8)) : command
    try ui32ContentReadStep("pre-action-new-context"){try newContext(command)}
    let nextSettingsOpen=try ui32ContentReadStep("pre-action-settings-phase"){try ui32NewSettingsNext(base,entryEnabled,newSettingsOpen)}
    let option=request["option"] as? String, rule=try ui32ContentReadStep("pre-action-rule"){try ui32NewSemanticRule(base,option,entryEnabled)}, selected=try ui32ContentReadStep("pre-action-resolve"){try resolve(rule)}
    if base.hasPrefix("selection.") || base.hasPrefix("frame.") || base == "tutorial.close" {
      try check(fixture?.completed == true && fixture?.confirmed == true)
    }
    let value=request["value"] as? String
    try check(!restoring || rule.operation == "value")
    if ["press","menu","filter-press","focus"].contains(rule.operation) { try check(value == nil && !restoring) }
    else if restoring { try check(value == nil && option == nil && originals[base] != nil) }
    else { try check(option == nil && value != nil && rule.values.contains(value!)) }
    let wanted=restoring ? originals[base] : value
    var posted=false, attempted=false
    do {
      try ui32GuardedAction({
        if base == "fixture.open" && attempted && posted {
          result["fixtureOpenIdentity"]=try ui32FixtureOpenIdentity(base,attempted,posted,
            now:{DispatchTime.now().uptimeNanoseconds},
            pause:{Thread.sleep(forTimeInterval:Double($0)/1_000_000_000)},identity:identity)
        } else {try identity()}
      },{
        let current=try resolve(rule); try check(CFEqual(current,selected)); try ui32ContentReadStep("guarded-new-context"){try newContext(command)}; try enabledVisible(selected)
        if entryEnabled && !base.hasPrefix("entry.") {
          var overlays=[String:Set<String>]()
          for node in try tree() {
            guard let id=try ui32DOMIdentifiers(node,pid).resolved,ui32EntryOverlayIDs.contains(id) else {continue}
            try check(overlays[id] == nil);overlays[id]=try ui32EntryDOMClasses(node,pid)
          }
          try ui32EntrySurface(base,true,overlays)
        }
        if base == "entry.create.default" {
          try ui32NewDefaultProof(pid,resolve)
        }
        if base == "settings.menu.entry" { try check(menuOpened) }
        if base == "motion.cancel-pending" {
          let currentTree=try tree();try check(currentTree.contains { labels($0).contains("Loading properties…") })
        }
        if base == "playback.pause" {
          let descendants=children(selected); try check(descendants.count <= 8 && !descendants.isEmpty)
          for child in descendants { try check(owns(child,pid)) }
          try check(ui32PauseAllowed(descendants.reduce(into:Set<String>()) { $0.formUnion(labels($1)) }))
        }
        if base == "sync.check-empty" || base == "sync.open-cancel" {
          let folder=try resolve(UI32Rule(identifier:"sync-folder-path",region:"settings-modal",labels:[],roles:["AXTextField"],operation:"observe",values:[]))
          var writable=DarwinBoolean(false)
          let query=AXUIElementIsAttributeSettable(folder,kAXValueAttribute as CFString,&writable)
          try check(ui32EmptySyncAllowed(attr(folder,kAXValueAttribute) as? String,query == .success,writable.boolValue))
        }
      },{
        if ["press","menu","filter-press"].contains(rule.operation) {
          let axAction=rule.operation == "menu" ? kAXShowMenuAction : kAXPressAction
          var names:CFArray?; try check(AXUIElementCopyActionNames(selected,&names) == .success &&
            (names as? [String] ?? []).contains(axAction as String))
          if entryEnabled {
            try check(owns(selected,pid))
            try ui32NewSemanticPress(base,true,try ui32DOMIdentifiers(selected,pid).resolved ?? "",labels(selected),
              attr(selected,kAXRoleAttribute) as? String ?? "",Set(names as? [String] ?? []));try check(owns(selected,pid))
          }
          attempted=true
          try check(AXUIElementPerformAction(selected,axAction as CFString) == .success)
        } else if rule.operation == "focus" {
          var focusable=DarwinBoolean(false)
          try check(AXUIElementIsAttributeSettable(selected,kAXFocusedAttribute as CFString,&focusable) == .success && focusable.boolValue)
          attempted=true;try check(AXUIElementSetAttributeValue(selected,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
          try check(attr(selected,kAXFocusedAttribute) as? Bool == true)
        } else {
          var writable=DarwinBoolean(false)
          var raw:CFTypeRef?
          try check(AXUIElementIsAttributeSettable(selected,kAXValueAttribute as CFString,&writable) == .success && writable.boolValue)
          try check(AXUIElementCopyAttributeValue(selected,kAXValueAttribute as CFString,&raw) == .success)
          let originalValue=(raw as? String) ?? (base == "grid.step.value" ? (raw as? NSNumber)?.stringValue : nil)
          guard let original=originalValue, original.utf8.count <= 240, let wanted else { throw Refusal(code:"adapter-refused") }
          if originals[base] == nil { originals[base]=original }
          try ui32ProfileFocus(base,selected,pinned,{try resolve(rule)},enabledVisible,{attempted=true},&result)
          attempted=true
          try check(AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,wanted as CFString) == .success)
          result["requestedValue"]=wanted
        }
      },postScope:{
        if entryEnabled && base == "entry.create.default" {
          guard let web=retainedWebArea else {throw Refusal(code:"adapter-refused")}
          let accepted=try ui32PostedNewContentRead(base,entryEnabled,attempted,posted,entryStage,expected:web,
            now:{DispatchTime.now().uptimeNanoseconds},retained:{retainedWebArea},equal:{CFEqual($0,$1)},native:nativeIdentity,
            certificate:{try readContentCertificate($0,$1)})
          _ = publishContent(accepted.certificate)
          result["entryContentRead"]=accepted.diagnostics
        } else if base == "fixture.open" || base == "sync.open-cancel" {try identity()} else {_ = try tree()}
      },posted:{
        posted=true;entryStage=nextEntryStage;newSettingsOpen=nextSettingsOpen
        if base == "fixture.open" { fixture?.attempted=true }
        if base == "sync.open-cancel" { syncPanel?.attempted=true }
      })
      if base == "settings.menu.open" { menuOpened=true }
      if base == "settings.menu.entry" { menuOpened=false }
    if rule.operation == "value" {
      let current=try resolve(rule); try check(owns(current,pid))
      if let readback=attr(current,kAXValueAttribute) as? String { result["readback"]=readback } else { result["readback"]=NSNull() }
    }
    } catch {
      var facts:[String:Any]=["purpose":"UI32","command":command,"ordinal":ordinal,"actionPosted":posted,"actionAttempted":attempted,
        "stage":posted ? "post-action-or-scope" : "pre-action-or-perform","underlying":ui32ActionCause(error),"profileFocus":result["profileFocus"] ?? NSNull(),
        "note":"No retry; inspect actual result before any new appointment."]
      if let proof=result["fixtureOpenIdentity"] {facts["fixtureOpenIdentity"]=proof}
      throw ActionRefusal(diagnostics:facts)
    }
    result["actionPosted"]=posted; result["actionAttempted"]=attempted
    // AX acknowledgement/readback alone is not application acceptance.
    try emit(result)
  }
}

// Pure injected ports reach the same cardinality refusal after a complete census.
