// UI32-only persistent runner. No keyboard, coordinates, arbitrary route or product API.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

func runUI32(_ args:[String], _ environment:[String:String]) throws {
  let pinned:OwnedInitialWindow
  do { pinned=try discoverOwnedInitialWindow(args,environment,scopeCheck:ui32Scope) }
  catch let error as OwnedDiscoveryRefusal {
    var facts=error.diagnostics; facts["purpose"]="UI32"; facts["schema"]="nemo.ui32-discovery/1"
    throw ActionRefusal(diagnostics:facts)
  }
  let pid=pinned.pid, main=pinned.main
  var ordinal=0, originals=[String:String](), observed=false, menuOpened=false
  var fixture:UI32Fixture?, syncPanel:UI32Fixture?
  func identity() throws { try verifyOwnedWindow(pinned) }
  func tree() throws -> [AXUIElement] {
    try identity()
    // Reject native sheets/popups in other windows, even when they share the PID.
    guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement], windows.count == 1,
      owns(windows[0],pid), CFEqual(windows[0],main) else { throw Refusal(code:"sheet-active") }
    // Diagnostic only: retain the child edges already authenticated by this walk.
    // A mismatch still refuses before observation or any effectful command succeeds.
    var declaredEdges=[(child:AXUIElement,parent:AXUIElement)]()
    var refusedEdge:(child:AXUIElement,declaredParent:AXUIElement,reportedParent:AXUIElement)?
    do {
    return try boundedOwnedTree(main,owner:Int(pid),deadline:DispatchTime.now().uptimeNanoseconds+750_000_000,
      now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:{ node in
        var actual:pid_t=0; let error=AXUIElementGetPid(node,&actual)
        if error == .success && actual == pid && AXUIElementSetMessagingTimeout(node,0.03) != .success { return (-1,false,Int(actual)) }
        return (Int(error.rawValue),error == .success && actual == pid,Int(actual))
      },children:{ node in
        let (query,children)=ui32Children(node,pid)
        guard query == 0, let rows=children else { return (-1,nil) }
        // No role/value/descendant read of any unowned child.
        for child in rows {
          guard owns(child,pid), AXUIElementSetMessagingTimeout(child,0.03) == .success else { return (-1,nil) }
          guard attr(child,kAXRoleAttribute) as? String != kAXWindowRole else { return (-1,nil) }
        }
        for child in rows { declaredEdges.append((child:child,parent:node)) }
        return (0,rows)
      },parent:{ node in
        var raw:CFTypeRef?; let q=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
        let actual=axElement(raw)
        if q == .success, let actual,
          let edge=declaredEdges.last(where:{CFEqual($0.child,node)}), !CFEqual(actual,edge.parent) {
          // Retain references only; the original walk still makes its refusal.
          refusedEdge=(child:node,declaredParent:edge.parent,reportedParent:actual)
        }
        return (Int(q.rawValue),actual)
      })
    } catch let error as ActionRefusal {
      var diagnostics=error.diagnostics
      if diagnostics["stage"] as? String == "parent-edge", let refusedEdge {
        var identityCurrent=false
        do { try identity(); identityCurrent=true } catch { }
        func facts(_ element:AXUIElement) -> [String:Any] {
          guard identityCurrent else { return ["identityCurrent":false] }
          guard owns(element,pid) else { return ["owned":false] }
          guard AXUIElementSetMessagingTimeout(element,0.03) == .success else {
            return ["owned":true,"timeoutReady":false]
          }
          return ["owned":true,"timeoutReady":true,"isPinnedMain":CFEqual(element,main),
            "role":attr(element,kAXRoleAttribute) as? String ?? "",
            "identifier":attr(element,kAXIdentifierAttribute) as? String ?? ""]
        }
        // Attribute reads happen only after the unchanged parent-edge refusal,
        // and only after owner/lifetime and per-element timeout authentication.
        diagnostics["ui32OwnedEdge"]=["identityCurrent":identityCurrent,
          "child":facts(refusedEdge.child),"declaredParent":facts(refusedEdge.declaredParent),
          "reportedParent":facts(refusedEdge.reportedParent),"declaredEdgeCount":declaredEdges.count,
          "matchingDeclaredParents":declaredEdges.filter{CFEqual($0.child,refusedEdge.child)}.count,
          "reportedParentIsApplication":CFEqual(refusedEdge.reportedParent,pinned.application),
          "reportedParentMatchesAnyDeclaredNode":declaredEdges.contains{CFEqual($0.parent,refusedEdge.reportedParent)}]
      }
      throw ActionRefusal(diagnostics:diagnostics)
    }
  }
  func labels(_ node:AXUIElement) -> Set<String> {
    Set([kAXTitleAttribute,kAXDescriptionAttribute,kAXHelpAttribute,kAXValueAttribute].compactMap { attr(node,$0) as? String })
  }
  func ancestors(_ node:AXUIElement) throws -> Set<String> {
    var result=Set<String>(), current=node, visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains { CFEqual($0,current) }); visited.append(current)
      if let id=attr(current,kAXIdentifierAttribute) as? String { result.insert(id) }
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
          if let id=attr(row,kAXIdentifierAttribute) as? String { ids.insert(id) }
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
    return ui32Matches(rule,identifier:attr(node,kAXIdentifierAttribute) as? String ?? "",labels:labels(node),
      role:attr(node,kAXRoleAttribute) as? String ?? "",ancestors:try ancestors(node))
  }
  func resolve(_ rule:UI32Rule) throws -> AXUIElement {
    let rows=try tree().filter { try match(rule,$0) }
    try check(rows.count == 1); return rows[0]
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
    ordinal += 1; try identity()
    var result:[String:Any]=["purpose":"UI32","pid":pid,"windowId":pinned.number,"ordinal":ordinal,"command":command,"status":"ok","actionPosted":false]
    if command == "quit" { try check(request["option"] == nil && request["value"] == nil); try emit(result); return }
    if command == "observe" {
      try check(request["option"] == nil && request["value"] == nil)
      let nodes=try tree(); var facts=[[String:Any]]()
      for node in nodes {
        try check(owns(node,pid))
        let role=attr(node,kAXRoleAttribute) as? String ?? "", id=attr(node,kAXIdentifierAttribute) as? String ?? ""
        let rawValue=attr(node,kAXValueAttribute)
        let value=rawValue as? String
        var fact:[String:Any]=["role":role,"identifier":String(id.prefix(100)),"labels":labels(node).map {String($0.prefix(240))}.sorted()]
        if let value { fact["value"]=String(value.prefix(240)) }
        else if let numeric=rawValue as? NSNumber { fact["value"]=numeric }
        if let enabled=attr(node,kAXEnabledAttribute) as? Bool { fact["enabled"]=enabled }
        if let selected=attr(node,kAXSelectedAttribute) as? Bool { fact["selected"]=selected }
        if let focused=attr(node,kAXFocusedAttribute) as? Bool { fact["focused"]=focused }
        facts.append(fact)
      }
      try identity(); observed=true; result["nodes"]=facts; result["baselineObserved"]=true; try emit(result); continue
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
      result["capture"]=name; try emit(result); continue
    }
    try ui32RequireObserved(observed)
    if command == "selection.toggle.B" {
      try check(request["option"] == nil && request["value"] == nil && fixture?.completed == true && fixture?.confirmed == true)
      result.merge(try ui32ToggleB(pinned,resolve:resolve,visible:enabledVisible)) { _,new in new };try emit(result);continue
    }
    if ["motion.focus","motion.shortcut","motion.guard-typing"].contains(command) {
      try check(request["value"] == nil)
      result.merge(try runUI32Keys(command,request["option"] as? String,pinned,resolve:resolve,visible:enabledVisible)) { _,new in new }
      try emit(result); continue
    }
    if ["sync.panel-observe","sync.cancel-only","sync.complete"].contains(command) {
      try check(request["option"] == nil && request["value"] == nil)
      guard let syncPanel else { throw Refusal(code:"adapter-refused") }
      result.merge(try syncPanel.perform(command)) { _,new in new };try emit(result);continue
    }
    if command == "sync.open-cancel" {
      try check(syncPanel == nil);syncPanel=try UI32Fixture(pinned,environment,cancelOnly:true)
    }
    if command.hasPrefix("fixture.") && command != "fixture.open" {
      try check(request["option"] == nil && request["value"] == nil)
      guard let fixture else { throw Refusal(code:"adapter-refused") }
      result.merge(try fixture.perform(command)) { _,new in new }; try emit(result); continue
    }
    if command == "fixture.open" {
      try check(request["option"] == nil && request["value"] == nil && fixture == nil)
      fixture=try UI32Fixture(pinned,environment)
    }
    let restoring=command.hasSuffix(".restore"), base=restoring ? String(command.dropLast(8)) : command
    let option=request["option"] as? String, rule=try ui32Rule(base,option), selected=try resolve(rule)
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
        let current=try resolve(rule); try check(CFEqual(current,selected)); try enabledVisible(selected)
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
      },postScope:{ if base == "fixture.open" || base == "sync.open-cancel" { try identity() } else { _ = try tree() } },posted:{posted=true})
      if base == "settings.menu.open" { menuOpened=true }
      if base == "settings.menu.entry" { menuOpened=false }
      if base == "fixture.open" { fixture?.attempted=true }
      if base == "sync.open-cancel" { syncPanel?.attempted=true }
    if rule.operation == "value" {
      let current=try resolve(rule); try check(owns(current,pid))
      if let readback=attr(current,kAXValueAttribute) as? String { result["readback"]=readback } else { result["readback"]=NSNull() }
    }
    } catch {
      throw ActionRefusal(diagnostics:["purpose":"UI32","command":command,"ordinal":ordinal,"actionPosted":posted,"actionAttempted":attempted,
        "stage":posted ? "post-action-or-scope" : "pre-action-or-perform","note":"No retry; inspect actual result before any new appointment."])
    }
    result["actionPosted"]=posted; result["actionAttempted"]=attempted
    // AX acknowledgement/readback alone is not application acceptance.
    try emit(result)
  }
}
