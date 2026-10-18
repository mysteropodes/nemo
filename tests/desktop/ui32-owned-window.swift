// UI32-only persistent runner. No keyboard, coordinates, arbitrary route or product API.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

// Complete forward census is a scope certificate, NOT a successful whole-window
// reciprocal walk. The common verifier remains mandatory inside the WebArea.
struct UI32ContentCertificate<Node> {
  let nodes:[Node], roles:[String], declared:[Int?], reported:[Int?]
  let web:Int, inside:Set<Int>, outsideMismatches:[Int]
}
func ui32ScopeCheck(_ ok:Bool, _ reason:String) throws {
  if !ok { throw ActionRefusal(diagnostics:["stage":"content-scope","reason":reason]) }
}
func ui32ContentCertificate<Node>(_ main:Node, pid:Int, retained:Node?, deadline:UInt64,
  now:()->UInt64, equal:(Node,Node)->Bool, owner:(Node)->(Int,Bool,Int), prepare:(Node)->Bool,
  traits:(Node) throws -> (String,Bool), children:(Node)->(Int,[Node]?), parent:(Node)->(Int,Node?),
  strict:(Node,UInt64) throws -> [Node]) throws -> UI32ContentCertificate<Node> {
  var nodes=[Node](), roles=[String](), modals=[Bool](), declared=[Int?](), reportedRefs=[Node?]()
  func owned(_ node:Node) throws {
    let (error,matches,actual)=owner(node)
    try ui32ScopeCheck(error == 0 && matches && actual == pid,"owner")
  }
  func visit(_ node:Node, _ ancestor:Int?, _ depth:Int) throws {
    try ui32ScopeCheck(now() < deadline,"deadline")
    try ui32ScopeCheck(depth <= 12 && nodes.count < 512,"bounds")
    try owned(node);try ui32ScopeCheck(!nodes.contains {equal($0,node)},"cycle-or-alias")
    try ui32ScopeCheck(prepare(node),"timeout")
    let (role,modal)=try traits(node), index=nodes.count
    try ui32ScopeCheck(!role.isEmpty && (index == 0 ? role == "AXWindow" : role != "AXWindow"),"window-role")
    nodes.append(node);roles.append(role);modals.append(modal);declared.append(ancestor);reportedRefs.append(nil)
    if ancestor != nil {
      let (query,observed)=parent(node)
      try ui32ScopeCheck(query == 0 && observed != nil,"parent-query")
      try owned(observed!);reportedRefs[index]=observed!
    }
    let (query,rows)=children(node)
    try ui32ScopeCheck(query == 0 && rows != nil,"children-query")
    try ui32ScopeCheck(rows!.count <= 512,"children-count")
    for child in rows! { try visit(child,index,depth+1) }
    try ui32ScopeCheck(now() < deadline,"deadline")
  }
  try visit(main,nil,0)
  let candidates=roles.indices.filter {roles[$0] == "AXWebArea"}
  do { try ui32ScopeCheck(candidates.count == 1,"webarea-count") }
  catch let error as ActionRefusal {
    // This complete bounded census is already authenticated; no new AX reads.
    let categories=["AXWebArea","AXGroup","AXScrollArea","AXSplitGroup","AXButton","AXWindow"]
    var counts=Dictionary(uniqueKeysWithValues:categories.map {($0,0)})
    counts["other"]=0
    for role in roles { counts[categories.contains(role) ? role : "other",default:0] += 1 }
    var facts=error.diagnostics
    facts["webAreaCount"]=candidates.count;facts["ownedWindowNodeCount"]=nodes.count;facts["roleCounts"]=counts
    throw ActionRefusal(diagnostics:facts)
  }
  let web=candidates[0]
  if let retained { try ui32ScopeCheck(equal(nodes[web],retained),"webarea-replaced") }
  var reported=[Int?](repeating:nil,count:nodes.count)
  for i in nodes.indices where i != 0 {
    guard let ref=reportedRefs[i],let actual=nodes.firstIndex(where:{equal($0,ref)}) else {
      throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"parent-outside-window"])
    }
    reported[i]=actual
  }
  var cursor=web
  while cursor != 0 {
    guard let next=declared[cursor] else { throw Refusal(code:"adapter-refused") }
    try ui32ScopeCheck(reported[cursor] == next,"webarea-root-path");cursor=next
  }
  var inside=Set<Int>()
  for i in nodes.indices {
    var current:Int?=i
    while let value=current {
      if value == web { inside.insert(i);break };current=declared[value]
    }
  }
  for i in nodes.indices {
    try ui32ScopeCheck(roles[i] != "AXSheet" && (inside.contains(i) || !modals[i]),"native-modal-present")
  }
  // Use the original strict verifier with actual AXParent, never the declared
  // parent. Compare its complete membership/order to the already owned census.
  let content:[Node], expected=nodes.indices.filter {inside.contains($0)}
  do { content=try strict(nodes[web],deadline) }
  catch let error as ActionRefusal {
    var facts=error.diagnostics;facts["insideContent"]=true;throw ActionRefusal(diagnostics:facts)
  }
  try ui32ScopeCheck(content.count == expected.count,"content-changed")
  for (node,index) in zip(content,expected) {
    try ui32ScopeCheck(now() < deadline,"deadline")
    try ui32ScopeCheck(equal(node,nodes[index]),"content-changed");try owned(node)
    let (query,ref)=parent(node)
    try ui32ScopeCheck(query == 0 && ref != nil,"content-parent-query");try owned(ref!)
    try ui32ScopeCheck(equal(ref!,nodes[declared[index]!]),"content-parent-changed")
  }
  try ui32ScopeCheck(now() < deadline,"deadline")
  let (currentRole,_)=try traits(nodes[web]);try ui32ScopeCheck(currentRole == "AXWebArea","webarea-role-changed")
  try ui32ScopeCheck(now() < deadline,"deadline")
  let outside=nodes.indices.filter {$0 != 0 && !inside.contains($0) && reported[$0] != declared[$0]}
  return UI32ContentCertificate(nodes:nodes,roles:roles,declared:declared,reported:reported,web:web,inside:inside,outsideMismatches:outside)
}
func ui32ScopedCandidate(_ inside:[Int], _ outside:[Int]) throws -> Int {
  try ui32ScopeCheck(outside.isEmpty,"candidate-outside-content")
  try ui32ScopeCheck(inside.count == 1,"candidate-count");return inside[0]
}

func runUI32(_ args:[String], _ environment:[String:String]) throws {
  let pinned:OwnedInitialWindow
  do { pinned=try discoverOwnedInitialWindow(args,environment,scopeCheck:ui32Scope) }
  catch let error as OwnedDiscoveryRefusal {
    var facts=error.diagnostics; facts["purpose"]="UI32"; facts["schema"]="nemo.ui32-discovery/1"
    throw ActionRefusal(diagnostics:facts)
  }
  let pid=pinned.pid, main=pinned.main
  var ordinal=0, originals=[String:String](), observed=false, menuOpened=false
  var currentCommand:String?
  let entryEnabled=try ui32EntryMode(environment["NEMO_UI32_ENTRY_MODE"]);var entryStage=0,sourceCaptureStage = -1,newSettingsOpen=false
  var fixture:UI32Fixture?, syncPanel:UI32Fixture?
  var retainedWebArea:AXUIElement?, latest:UI32ContentCertificate<AXUIElement>?
  func nativeIdentity() throws { try verifyOwnedWindow(pinned) }
  func identity() throws {
    try nativeIdentity()
    let phase=try ui32IdentityPhase(currentCommand,
      fixtureActive:fixture?.attempted == true && fixture?.completed != true,
      syncActive:syncPanel?.attempted == true && syncPanel?.completed != true)
    // Native picker commands prove their existing attached owner/context before
    // reads/effects; completion re-proves absence and full content after dismissal.
    if phase != .content { return }
    guard let web=retainedWebArea else { return }
    var current=web,seen=[AXUIElement]()
    for _ in 0..<13 {
      try check(owns(current,pid) && !seen.contains {CFEqual($0,current)})
      try check(AXUIElementSetMessagingTimeout(current,0.03) == .success);seen.append(current)
      if CFEqual(current,main) { return }
      guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pid) else { throw Refusal(code:"child-owner") }
      let (query,rows)=ui32Children(parent,pid)
      try check(query == 0 && rows != nil && rows!.contains {CFEqual($0,current)});current=parent
    }
    throw Refusal(code:"adapter-refused")
  }
  func text(_ node:AXUIElement, _ key:String) throws -> String? {
    try check(owns(node,pid) && AXUIElementSetMessagingTimeout(node,0.03) == .success)
    var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
    if query == .attributeUnsupported || query == .noValue { return nil }
    try check(query == .success)
    if let value=raw as? String { return value }
    if key == kAXValueAttribute && raw is NSNumber { return nil }
    throw Refusal(code:"adapter-refused")
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
  func tree() throws -> [AXUIElement] {
    try identity()
    guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement],windows.count == 1,
      owns(windows[0],pid),CFEqual(windows[0],main) else { throw Refusal(code:"sheet-active") }
    let deadline=DispatchTime.now().uptimeNanoseconds+750_000_000
    let certificate=try ui32ContentCertificate(main,pid:Int(pid),retained:retainedWebArea,deadline:deadline,
      now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:owner,
      prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},traits:{ node in
        guard let role=try text(node,kAXRoleAttribute) else { throw Refusal(code:"adapter-refused") }
        var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,kAXModalAttribute as CFString,&raw)
        var modal=false
        if query == .success { guard let value=raw as? Bool else { throw Refusal(code:"adapter-refused") };modal=value }
        else { try check(query == .attributeUnsupported || query == .noValue) }
        let subrole=try text(node,kAXSubroleAttribute)
        return (role,modal || subrole == kAXDialogSubrole)
      },children:{ui32Children($0,pid)},parent:parent,strict:{ web,deadline in
        try boundedOwnedTree(web,owner:Int(pid),deadline:deadline,now:{DispatchTime.now().uptimeNanoseconds},
          equal:{CFEqual($0,$1)},owner:owner,children:strictChildren,parent:parent)
      })
    try nativeIdentity();retainedWebArea=certificate.nodes[certificate.web];latest=certificate
    return certificate.nodes.indices.filter {certificate.inside.contains($0)}.map {certificate.nodes[$0]}
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
    guard let role=try text(node,kAXRoleAttribute) else { throw Refusal(code:"adapter-refused") }
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
    _ = try tree();guard let certificate=latest else { throw Refusal(code:"adapter-refused") }
    var inside=[Int](),outside=[Int]()
    for i in certificate.nodes.indices {
      let node=certificate.nodes[i]
      if try signature(rule,node) {
        if certificate.inside.contains(i) { if try match(rule,node) {inside.append(i)} }
        else { outside.append(i) }
      }
    }
    return certificate.nodes[try ui32ScopedCandidate(inside,outside)]
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
      let nodes=try tree(); var facts=[[String:Any]]()
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
    if command == "selection.toggle.B" {
      try check(request["option"] == nil && request["value"] == nil && fixture?.completed == true && fixture?.confirmed == true)
      _ = try tree()
      result.merge(try ui32ToggleB(pinned,resolve:resolve,visible:enabledVisible)) { _,new in new };try emit(result);continue
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
      try check(request["option"] == nil && request["value"] == nil && fixture == nil)
      fixture=try UI32Fixture(pinned,environment,afterDismissal:{ _ = try tree() })
    }
    let restoring=command.hasSuffix(".restore"), base=restoring ? String(command.dropLast(8)) : command
    try newContext(command)
    let nextSettingsOpen=try ui32NewSettingsNext(base,entryEnabled,newSettingsOpen)
    let option=request["option"] as? String, rule=try ui32NewSemanticRule(base,option,entryEnabled), selected=try resolve(rule)
    if base.hasPrefix("selection.") || base.hasPrefix("frame.") {
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
      try ui32GuardedAction(identity,{
        let current=try resolve(rule); try check(CFEqual(current,selected)); try newContext(command); try enabledVisible(selected)
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
          attempted=true
          try check(AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,wanted as CFString) == .success)
          result["requestedValue"]=wanted
        }
      },postScope:{ if base == "fixture.open" || base == "sync.open-cancel" { try identity() } else { _ = try tree() } },posted:{
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
      throw ActionRefusal(diagnostics:["purpose":"UI32","command":command,"ordinal":ordinal,"actionPosted":posted,"actionAttempted":attempted,
        "stage":posted ? "post-action-or-scope" : "pre-action-or-perform","underlying":ui32ActionCause(error),
        "note":"No retry; inspect actual result before any new appointment."])
    }
    result["actionPosted"]=posted; result["actionAttempted"]=attempted
    // AX acknowledgement/readback alone is not application acceptance.
    try emit(result)
  }
}

// Pure injected ports reach the same cardinality refusal after a complete census.
func ui32WebAreaCountSelfTest() throws {
  for count in [0,2] {
    let roles=["AXWindow","AXGroup","AXScrollArea","AXSplitGroup","AXButton","AXStaticText"] + Array(repeating:"AXWebArea",count:count)
    var traits=0,parents=0,children=0,strict=0,facts:[String:Any]?
    do {
      let _:UI32ContentCertificate<Int>=try ui32ContentCertificate(0,pid:7,retained:nil,deadline:10,
        now:{0},equal:==,owner:{_ in (0,true,7)},prepare:{_ in true},traits:{node in
          traits += 1;return (roles[node],false)
        },children:{node in
          children += 1;return (0,node == 0 ? Array(1..<roles.count) : [])
        },parent:{_ in parents += 1;return (0,0)},strict:{_,_ in strict += 1;return []})
    } catch let error as ActionRefusal { facts=error.diagnostics }
    guard let facts,let counts=facts["roleCounts"] as? [String:Int] else { throw Refusal(code:"adapter-refused") }
    try check(facts["stage"] as? String == "content-scope" && facts["reason"] as? String == "webarea-count")
    try check(facts["webAreaCount"] as? Int == count && facts["ownedWindowNodeCount"] as? Int == roles.count)
    try check(counts == ["AXWebArea":count,"AXGroup":1,"AXScrollArea":1,"AXSplitGroup":1,"AXButton":1,"AXWindow":1,"other":1])
    try check(Set(facts.keys) == Set(["stage","reason","webAreaCount","ownedWindowNodeCount","roleCounts"]))
    try check(traits == roles.count && children == roles.count && parents == roles.count-1 && strict == 0)
  }
}
