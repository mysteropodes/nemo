// P38V2 test-only persistent main/sheet control; no global input or rediscovery.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin
import Security

struct Refusal: Error { let code: String }
struct SelectionRefusal: Error { let diagnostics: [String:Any] }
struct ActionRefusal: Error { let diagnostics: [String:Any] }
func locationCheck(_ ok: Bool, _ stage: String, _ reason: String? = nil) throws {
  if !ok {
    var facts: [String:Any] = ["stage":stage]
    if let reason { facts["gotoReason"] = reason }
    throw ActionRefusal(diagnostics:facts)
  }
}
// This records execution of one retained AX action, never native UI acceptance.
final class LocationTrace {
  var phase = "request"
  private(set) var actionPerformed = false
  func actionCompleted() { actionPerformed = true; phase = "posted-pair" }
  func facts(_ original: [String:Any]) -> [String:Any] {
    var result = original
    result["gotoPhase"] = phase; result["commandPosted"] = actionPerformed
    return result
  }
  func observe<T>(_ body: () throws -> T) throws -> T {
    do { return try body() }
    catch let error as ActionRefusal { throw ActionRefusal(diagnostics:facts(error.diagnostics)) }
    catch { throw ActionRefusal(diagnostics:facts(["stage":"goto-state","gotoReason":"unexpected-refusal"])) }
  }
}
func locationPredicate(_ ok: Bool, _ reason: String, _ facts: [String:Any] = [:]) throws {
  if !ok {
    var result = facts; result["stage"] = "goto-discovery"; result["gotoReason"] = reason
    throw ActionRefusal(diagnostics:result)
  }
}
// Production AX and static fault controls use this same complete walk. An
// unreadable owner is not absence; no role/parent/children read follows it.
func locationWalk<Node>(_ root: Node, deadline: UInt64, now: () -> UInt64,
  equal: (Node,Node)->Bool, owner: (Node)->(Int,Bool,Int), foreignChild: (String,Int)->Void = { _,_ in }, isWindow: (Node)->Bool,
  timeout: (Node)->Int, children: (Node)->(Int,[Node]?), parent: (Node)->(Int,Node?)) throws -> [Node] {
  var found = [Node]()
  func walk(_ current: Node, _ route: String, _ depth: Int) throws {
    let facts: [String:Any] = ["gotoRoute":route,"gotoDepth":depth,"gotoNodeCount":found.count]
    func require(_ ok: Bool, _ reason: String, _ extra: [String:Any] = [:]) throws {
      var observed = facts; observed["gotoNodeCount"] = found.count
      try locationPredicate(ok,reason,observed.merging(extra) { _,new in new })
    }
    func owned(_ node: Node, _ kind: String) throws {
      let (error,matches,_) = owner(node)
      try require(error == 0,kind+"-owner-query",["gotoQueryError":error,"ownerMatches":false])
      try require(matches,kind+"-owner-mismatch",["gotoQueryError":error,"ownerMatches":matches])
    }
    if now() >= deadline { throw ActionRefusal(diagnostics:facts.merging(["stage":"goto-deadline","gotoReason":"walk-deadline"]) { _,new in new }) }
    try require(depth <= 12,"depth-limit"); try require(found.count < 512,"node-limit")
    try owned(current,"node")
    try require(!found.contains { equal($0,current) },"cycle")
    try require(!isWindow(current),"standalone-window")
    let timeoutError = timeout(current)
    try require(timeoutError == 0,"timeout-config",["gotoQueryError":timeoutError])
    found.append(current)
    let (error,rows) = children(current)
    if error == Int(AXError.attributeUnsupported.rawValue) || error == Int(AXError.noValue.rawValue) { return }
    try require(error == 0,"children-query",["gotoQueryError":error])
    try require(rows != nil,"children-shape",["gotoQueryError":error])
    let list = rows!
    try require(list.count <= 512,"children-limit",["gotoChildCount":min(list.count,513)])
    for (index,child) in list.enumerated() {
      let childRoute = route.isEmpty ? String(index) : route+"."+String(index)
      // Attribute a failed edge to its child route, before entering that node.
      let edge: [String:Any] = ["gotoRoute":childRoute,"gotoDepth":depth+1,"gotoNodeCount":found.count]
      func edgeRequire(_ ok: Bool, _ reason: String, _ extra: [String:Any] = [:]) throws {
        try locationPredicate(ok,reason,edge.merging(extra) { _,new in new })
      }
      let (childError,childOwned,childPid) = owner(child)
      try edgeRequire(childError == 0,"child-owner-query",["gotoQueryError":childError,"ownerMatches":false])
      if !childOwned { foreignChild(childRoute,childPid) }
      try edgeRequire(childOwned,"child-owner-mismatch",["gotoQueryError":childError,"ownerMatches":childOwned])
      let (parentError,ancestor) = parent(child)
      try edgeRequire(parentError == 0,"parent-query",["gotoQueryError":parentError])
      try edgeRequire(ancestor != nil,"parent-shape",["gotoQueryError":parentError])
      let (ownerError,parentOwned,_) = owner(ancestor!)
      try edgeRequire(ownerError == 0,"parent-owner-query",["gotoQueryError":ownerError,"ownerMatches":false])
      try edgeRequire(parentOwned,"parent-owner-mismatch",["gotoQueryError":ownerError,"ownerMatches":parentOwned])
      try edgeRequire(equal(ancestor!,current),"parent-edge",["parentMatches":false])
      try walk(child,childRoute,depth+1)
    }
  }
  try walk(root,"",0)
  if now() >= deadline { throw ActionRefusal(diagnostics:["stage":"goto-deadline","gotoReason":"walk-deadline","gotoRoute":"","gotoDepth":0,"gotoNodeCount":found.count]) }
  return found
}
// Production and fault controls share the one-shot exact-element ordering.
// No process-targeted or global keyboard event is part of this transition.
func goToFolder(_ reference: Bool, attempted: Bool, trace: LocationTrace, consume: () -> Void, context: () throws -> Void,
  perform: () throws -> Void, discover: () throws -> Void) throws {
  try locationCheck(reference && !attempted,"goto-state"); consume()
  trace.phase = "pre-context"
  try context()
  try perform(); trace.actionCompleted()
  trace.phase = "post-context"
  try context()
  trace.phase = "discovery"
  try discover()
}
func attachedForeignRoot<Node>(_ root: Node, owner expectedOwner: Int, deadline: UInt64,
  now: () -> UInt64, equal: (Node,Node)->Bool,
  owner: (Node)->(Int,Bool,Int), children: (Node)->(Int,[Node]?),
  parent: (Node)->(Int,Node?), foreignChild: (String,Int)->Void = { _,_ in }) throws -> (root:Node,parent:Node,pid:Int,route:String) {
  var visited = [Node](), foreign = [(Node,Node,Int,String)]()
  func walk(_ current: Node, _ route:String, _ depth: Int) throws {
    try locationCheck(now() < deadline,"walk-deadline")
    try locationCheck(depth <= 12,"depth-limit")
    try locationCheck(visited.count < 512,"node-limit")
    let (ownerError,owned,actual)=owner(current)
    try locationCheck(ownerError == 0,"node-owner-query")
    try locationCheck(owned && actual == expectedOwner,"node-owner-mismatch")
    try locationCheck(!visited.contains { equal($0,current) },"cycle")
    visited.append(current)
    let (query,rows)=children(current)
    try locationCheck(query == 0 && rows != nil,"children-query")
    guard let rows else { throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"children-shape"]) }
    try locationCheck(rows.count <= 512,"children-limit")
    for (index,child) in rows.enumerated() {
      try locationCheck(now() < deadline,"walk-deadline")
      let (childError,childOwned,childPid)=owner(child)
      try locationCheck(childError == 0,"child-owner-query")
      if !childOwned || childPid != expectedOwner {
        // Do not read any foreign child's role, parent, children, or value
        // until its PID has been authenticated by the caller.
        let childRoute=route.isEmpty ? String(index) : route+"."+String(index)
        foreignChild(childRoute,childPid); foreign.append((child,current,childPid,childRoute)); continue
      }
      let (parentError,ancestor)=parent(child)
      try locationCheck(parentError == 0 && ancestor != nil,"parent-query")
      try locationCheck(equal(ancestor!,current),"parent-edge")
      let childRoute=route.isEmpty ? String(index) : route+"."+String(index)
      try walk(child,childRoute,depth+1)
    }
  }
  try walk(root,"",0)
  try locationCheck(foreign.count == 1,"candidate-count")
  let (candidate,ancestor,pid,route)=foreign[0]
  try locationCheck(pid > 0 && !equal(candidate,ancestor),"context-identity")
  return (candidate,ancestor,pid,route)
}
func boundedOwnedTree<Node>(_ root: Node, owner expectedOwner: Int, deadline: UInt64,
  now: () -> UInt64, equal: (Node,Node)->Bool,
  owner: (Node)->(Int,Bool,Int), children: (Node)->(Int,[Node]?),
  parent: (Node)->(Int,Node?)) throws -> [Node] {
  var visited = [Node]()
  func walk(_ current: Node, _ depth: Int) throws {
    try locationCheck(now() < deadline,"walk-deadline")
    try locationCheck(depth <= 12 && visited.count < 512,"node-limit")
    let (ownerError,owned,actual)=owner(current)
    try locationCheck(ownerError == 0 && owned && actual == expectedOwner,"node-owner-mismatch")
    try locationCheck(!visited.contains { equal($0,current) },"cycle")
    visited.append(current)
    let (query,rows)=children(current)
    try locationCheck(query == 0 && rows != nil,"children-query")
    guard let rows else { throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"children-shape"]) }
    try locationCheck(rows.count <= 512,"children-limit")
    for child in rows {
      try locationCheck(now() < deadline,"walk-deadline")
      let (childError,childOwned,childPid)=owner(child)
      try locationCheck(childError == 0 && childOwned && childPid == expectedOwner,"child-owner-mismatch")
      let (parentError,ancestor)=parent(child)
      try locationCheck(parentError == 0 && ancestor != nil,"parent-query")
      try locationCheck(equal(ancestor!,current),"parent-edge")
      try walk(child,depth+1)
    }
  }
  try walk(root,0)
  return visited
}
func guardedElementAction(_ identity:() throws->Bool, _ action:() throws->Void) throws {
  guard try identity() else { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"]) }
  try action()
}
func unresolvedServiceBlocksSheetMutation(_ target:String, _ action:String, _ serviceRootPresent:Bool,
  _ locationTokenPresent:Bool) -> Bool {
  target == "sheet" && ["press","set","select"].contains(action) && serviceRootPresent && !locationTokenPresent
}
func requireResolvedSheetMutation(_ target:String, _ action:String, _ serviceRootPresent:Bool,
  _ locationTokenPresent:Bool) throws {
  try locationCheck(!unresolvedServiceBlocksSheetMutation(target,action,serviceRootPresent,locationTokenPresent),
    "goto-state","unresolved-transition")
}
func locationCandidateValid(_ fresh: Bool, _ owned: Bool, _ modal: Bool, _ semantic: Bool,
  _ fields: Int, _ kind: String, _ enabled: Bool, _ writable: Bool, _ confirm: Bool) -> Bool {
  fresh && owned && modal && semantic && fields == 1 && kind == "plain" && enabled && writable && confirm
}
func ownedAncestor<Node>(_ candidate: Node, _ ancestor: Node, equal: (Node,Node)->Bool,
  owned: (Node)->Bool, isWindow: (Node)->Bool, parent: (Node)->Node?, children: (Node)->[Node]?) -> Bool {
  var current = candidate, visited = [Node]()
  for _ in 0..<20 {
    guard owned(current), !visited.contains(where:{ equal($0,current) }) else { return false }
    if equal(current,ancestor) { return true }
    guard !isWindow(current), let next = parent(current), owned(next),
      let rows = children(next), rows.count <= 512, rows.contains(where:{ equal($0,current) }) else { return false }
    visited.append(current); current = next
  }
  return false
}
func locationContextValid(_ mains: Int, _ windowsOwned: Bool, _ focusedWindowOwned: Bool, _ focusDescends: Bool) -> Bool {
  locationContextFailure(mains,windowsOwned,focusedWindowOwned,focusDescends) == nil
}
func locationContextFailure(_ mains: Int, _ windowsOwned: Bool, _ focusedWindowOwned: Bool, _ focusDescends: Bool) -> String? {
  if mains != 1 { return "context-main-count" }
  if !windowsOwned { return "context-windows-owner" }
  if !focusedWindowOwned { return "context-focused-window" }
  return focusDescends ? nil : "context-focus-ancestry"
}
func textFieldKind(_ query: AXError, _ subrole: String?) -> String {
  if query == .attributeUnsupported || query == .noValue { return "plain" }
  guard query == .success, let subrole else { return "unreadable" }
  if subrole == kAXSearchFieldSubrole { return "search" }
  return subrole.isEmpty || subrole == kAXUnknownSubrole ? "plain" : "other"
}
// The production AX writer and static fault controls share this exact sequence.
// Focus is diagnostic only: AXValue addresses the retained field, not the keyboard.
func enterSheetText(_ facts: [String:Any], identity: () throws -> Void,
  write: () -> Int, read: () -> (Int, Bool), confirm: () -> Int) throws -> [String:Any] {
  var d = facts
  func require(_ valid: Bool, _ stage: String) throws {
    if !valid { d["stage"] = stage; throw ActionRefusal(diagnostics:d) }
  }
  func pinned() throws {
    do { try identity() } catch { d["stage"] = "axtext-identity"; throw ActionRefusal(diagnostics:d) }
  }
  try pinned()
  try require(d["role"] as? String == "AXTextField" && d["textFieldKind"] as? String == "plain" &&
    d["enabled"] as? Bool == true && d["focusedMatches"] as? Bool == true, "axtext-role")
  try require(d["confirmQueryError"] as? Int == 0 && d["confirmAdvertised"] as? Bool == true, "axtext-confirm-support")
  try require(d["valueSettableQueryError"] as? Int == 0 && d["valueSettable"] as? Bool == true, "axtext-value-settable")
  try pinned()
  let written = write(); d["valueWriteError"] = written
  try require(written == 0, "axtext-write")
  try pinned()
  let (readError, matches) = read(); d["valueReadbackError"] = readError; d["valueMatches"] = matches
  try require(readError == 0 && matches, "axtext-readback")
  try pinned()
  let confirmed = confirm(); d["confirmError"] = confirmed
  try require(confirmed == 0, "axtext-confirm")
  d["stage"] = "axtext-confirm"; return d
}
struct IdentityRefusal: Error { let stage: String; let facts: [String:Any] }
struct InspectionRefusal: Error { let stage: String; let queryClass: String }
struct InspectionContext { var stage: String; var queryClass: String }
var activeInspectionContext: InspectionContext? = nil
let inspectionQueryClasses: [String:Set<String>] = [
  "identity-precondition":["identity-check-failed"],
  "request-precondition":["request-refused"],
  "main-children-query":["attribute-query-failed"],
  "main-children-value":["attribute-value-unreadable"],
  "application-windows-query":["attribute-query-failed"],
  "application-windows-value":["attribute-value-unreadable"],
  "main-sheet-state":["retained-sheet-present","attached-sheet-present"],
  "root-resolution":["route-refused"],
  "projection-budget":["budget-refused"],
  "ax-owner-query":["ownership-refused"],
  "ax-attribute-query":["attribute-query-failed"],
  "ax-children-query":["attribute-query-failed","attribute-value-unreadable"],
  "ax-projection":["topology-refused"]
]
func safeInspectionContext(_ stage: String, _ queryClass: String) -> [String:String]? {
  guard inspectionQueryClasses[stage]?.contains(queryClass) == true else { return nil }
  return ["stage":stage,"queryClass":queryClass]
}
func inspectionDiagnosticForRefusal(_ code: String, _ stage: String, _ queryClass: String) -> [String:String]? {
  guard code == "adapter-refused" else { return nil }
  return safeInspectionContext(stage,queryClass)
}
func setInspectionContext(_ stage: String, _ queryClass: String) {
  if activeInspectionContext != nil { activeInspectionContext = InspectionContext(stage:stage,queryClass:queryClass) }
}
let allowedReasons: Set<String> = ["adapter-refused","sheet-owner","sheet-parent","sheet-replaced","child-owner","confirm-unsupported","children-unavailable","sheet-ambiguous","sheet-active","standalone-window"]
func check(_ ok: Bool) throws { if !ok { throw Refusal(code:"adapter-refused") } }
let actions: Set<String> = ["inspect", "press", "set", "select", "raise", "capture", "check", "pin-sheet", "sheet-dismissed", "go-to-folder", "location-dismissed", "quit"]
func operatorScope(_ mode: String, _ environment: [String:String]) throws -> (reference:Bool, report:String) {
  try check(["--persistent","--reference-persistent","--png-persistent"].contains(mode))
  let reference = mode != "--persistent"
  let prefix = mode == "--png-persistent" ? "NEMO_PNG_UI_" : (reference ? "NEMO_REFERENCE_" : "NEMO_P38_")
  try check(environment[prefix+"GRANT"]?.isEmpty == false)
  guard let report = environment[prefix+"REPORT_ROOT"], !report.isEmpty else { throw Refusal(code:"adapter-refused") }
  return (reference,report)
}
func sheetProof(_ members: [Int], _ owners: [Int:Int], _ parents: [Int:Int], _ pid: Int, _ main: Int, _ retained: Int?) throws -> Int {
  try check(members.count == 1)
  let candidate = members[0]; try check(owners[candidate] == pid && parents[candidate] == main &&
    (retained == nil || retained == candidate)); return candidate
}
func route(_ raw: String) throws -> [Int] {
  if raw.isEmpty { return [] }
  let parts = raw.split(separator: ".", omittingEmptySubsequences: false)
  try check(parts.count <= 20 && raw.count <= 128)
  return try parts.map { part in
    try check(!part.isEmpty && part.allSatisfy { $0.isASCII && $0.isNumber })
    guard let n = Int(part), n >= 0, n < 4096 else { throw Refusal(code:"adapter-refused") }; return n
  }
}
func attr(_ node: AXUIElement, _ key: String) -> CFTypeRef? {
  var value: CFTypeRef?
  return AXUIElementCopyAttributeValue(node, key as CFString, &value) == .success ? value : nil
}
func axElement(_ value: CFTypeRef?) -> AXUIElement? {
  guard let value, CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
  return (value as! AXUIElement)
}
func children(_ node: AXUIElement) -> [AXUIElement] { attr(node, kAXChildrenAttribute) as? [AXUIElement] ?? [] }
func owns(_ node: AXUIElement, _ pid: pid_t) -> Bool {
  var actual: pid_t = 0; return AXUIElementGetPid(node, &actual) == .success && actual == pid
}
struct ProcessStart: Equatable { let pid:pid_t; let seconds:UInt64; let microseconds:UInt64 }
func panelServiceIdentityValid(_ pid:pid_t, _ start:ProcessStart?, _ expectedPID:pid_t,
  _ expectedStart:ProcessStart?, _ identifier:String?, _ signatureValid:Bool) -> Bool {
  pid > 0 && pid == expectedPID && start != nil && start == expectedStart &&
    identifier == "com.apple.appkit.xpc.openAndSavePanelService" && signatureValid
}
func attachedServiceParentValid(_ sameParent:Bool, _ observedParentPID:pid_t, _ nemoPID:pid_t,
  _ reachesPinnedSheet:Bool) -> Bool {
  sameParent && observedParentPID == nemoPID && nemoPID > 0 && reachesPinnedSheet
}
func uniqueFocusedField(_ count:Int, _ retainedMatches:Bool) -> Bool { count == 1 && retainedMatches }
func processStart(_ pid:pid_t) -> ProcessStart? {
  guard pid > 0 else { return nil }
  var mib=[CTL_KERN,KERN_PROC,KERN_PROC_PID,pid]
  var info=kinfo_proc(),size=MemoryLayout<kinfo_proc>.size
  let result=mib.withUnsafeMutableBufferPointer { buffer in
    sysctl(buffer.baseAddress,u_int(buffer.count),&info,&size,nil,0)
  }
  guard result == 0,size == MemoryLayout<kinfo_proc>.size,
    info.kp_proc.p_pid == pid,info.kp_proc.p_starttime.tv_sec >= 0,info.kp_proc.p_starttime.tv_usec >= 0 else { return nil }
  return ProcessStart(pid:pid,seconds:UInt64(info.kp_proc.p_starttime.tv_sec),microseconds:UInt64(info.kp_proc.p_starttime.tv_usec))
}
func authenticatedPanelService(_ pid:pid_t) -> Bool {
  guard pid > 0 else { return false }
  let attributes=[kSecGuestAttributePid as String:NSNumber(value:pid)] as CFDictionary
  var code:SecCode?
  guard SecCodeCopyGuestWithAttributes(nil,attributes,[],&code) == errSecSuccess,let code else { return false }
  var requirement:SecRequirement?
  let text="anchor apple and identifier \"com.apple.appkit.xpc.openAndSavePanelService\""
  guard SecRequirementCreateWithString(text as CFString,[],&requirement) == errSecSuccess,
        let requirement else { return false }
  let valid=SecCodeCheckValidity(code,[],requirement) == errSecSuccess
  var staticCode:SecStaticCode?
  guard SecCodeCopyStaticCode(code,[],&staticCode) == errSecSuccess,let staticCode else { return false }
  var information:CFDictionary?
  guard SecCodeCopySigningInformation(staticCode,[],&information) == errSecSuccess,
    let identifier=(information as? [CFString:Any])?[kSecCodeInfoIdentifier] as? String else { return false }
  return panelServiceIdentityValid(pid,processStart(pid),pid,processStart(pid),identifier,valid)
}
func localFacts(_ path: String, _ owner: pid_t?, _ pid: pid_t, _ read: () -> [String:Any]) -> [String:Any] {
  guard owner == pid else {
    return ["path":path,"role":"other","blocked":"child-owner","ownerQuery":OwnedOwnerQuery(owner.map{Int($0)},Int(pid)).rawValue]
  }
  return read()
}
// A path is diagnostic provenance, never authority to traverse a foreign AX node.
func foreignOwnerPathClass(_ path: String?) -> String {
  guard let path, !path.isEmpty else { return "process-path-unreadable" }
  if path.hasPrefix("/System/Library/") && path.contains("/com.apple.appkit.xpc.openAndSavePanelService.xpc/") {
    return "appkit-open-save-panel-path"
  }
  if path.hasPrefix("/System/Library/") || path.hasPrefix("/usr/libexec/") { return "system-path" }
  return "other-path"
}
// One private, mode-0600 local artifact under the already pinned report directory.
// It is not emitted through the helper's public JSON protocol or copied into receipts.
func recordForeignOwner(_ observed: Int, route: String, report: URL, identity: [FileAttributeKey:Any]) {
  guard route.utf8.count <= 128 else { return }
  var path: String? = nil
  if observed > 0 && observed <= Int(Int32.max) {
    var buffer = [CChar](repeating:0,count:4096)
    let length = buffer.withUnsafeMutableBufferPointer { proc_pidpath(pid_t(observed),$0.baseAddress,UInt32($0.count)) }
    if length > 0 && Int(length) < buffer.count { path = String(cString:buffer) }
  }
  let facts: [String:Any] = ["schema":"nemo.ax-owner-provenance.private/1", "route":route,
    "observedPid":observed, "pathClass":foreignOwnerPathClass(path),
    "executableName":path.map { URL(fileURLWithPath:$0).lastPathComponent } ?? "unreadable"]
  guard let data = try? JSONSerialization.data(withJSONObject:facts,options:[.sortedKeys]) else { return }
  let directory = Darwin.open(report.path,O_RDONLY | O_DIRECTORY | O_NOFOLLOW)
  guard directory >= 0 else { return }
  defer { _ = Darwin.close(directory) }
  var pinned = stat()
  guard fstat(directory,&pinned) == 0,
    UInt64(pinned.st_dev) == (identity[.systemNumber] as? NSNumber)?.uint64Value,
    UInt64(pinned.st_ino) == (identity[.systemFileNumber] as? NSNumber)?.uint64Value else { return }
  let name = "ax-owner-provenance.private.json"
  let file = name.withCString { Darwin.openat(directory,$0,O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW,mode_t(0o600)) }
  guard file >= 0 else { return }
  var complete = false
  defer { _ = Darwin.close(file); if !complete { _ = name.withCString { Darwin.unlinkat(directory,$0,0) } } }
  let count = data.withUnsafeBytes { Darwin.write(file,$0.baseAddress,data.count) }
  complete = count == data.count && fsync(file) == 0
}
func projectionLimit(_ depth: Int, _ nodes: Int) throws { try check(depth >= 1 && depth <= 5 && nodes >= 1 && nodes <= 128) }
func identityFailureStage(_ ownerQuery: Int, _ ownerMatches: Bool, _ windowsQuery: Int,
  _ windowsReadable: Bool, _ pinnedAXMember: Bool, _ minimizedQuery: Int, _ minimizedReadable: Bool, _ minimized: Bool,
  _ pinnedCGReadable: Bool, _ pinnedCGCount: Int) -> String? {
  if ownerQuery != 0 || !ownerMatches { return "ax-owner" }
  if windowsQuery != 0 || !windowsReadable || !pinnedAXMember { return "application-window-membership" }
  if !pinnedCGReadable { return "pinned-cg-window-list" }
  if pinnedCGCount != 1 { return "pinned-cg-window" }
  if minimizedQuery != 0 || !minimizedReadable || minimized { return "minimized" }
  return nil
}
func diagnosticSheetMatch(_ candidate: [String:Any]) -> Bool {
  candidate["candidatePidMatches"] as? Bool == true && candidate["candidateRoleReadable"] as? Bool == true &&
    candidate["candidateRole"] as? String == "AXSheet" && candidate["candidateParentReadable"] as? Bool == true &&
    candidate["candidateParentPidMatches"] as? Bool == true && candidate["candidateParentIsRetainedMain"] as? Bool == true
}
func retainedChildrenProbeClass(_ queryError: Int, _ readable: Bool, _ truncated: Bool, _ incomplete: Bool, _ matches: Int) -> String {
  if queryError != 0 { return "children-query-error" }
  if !readable { return "children-unreadable" }
  if truncated { return "children-truncated" }
  if incomplete { return "candidate-probe-incomplete" }
  if matches == 0 { return "no-attached-sheet" }
  if matches == 1 { return "one-attached-sheet" }
  return "multiple-attached-sheets"
}
func shouldProbeRetainedMainChildren(_ stage: String, _ facts: [String:Any]) -> Bool {
  stage == "application-window-membership" && facts["axPidMatches"] as? Bool == true &&
    facts["applicationWindowsQueryError"] as? Int == 0 && facts["applicationWindowsReadable"] as? Bool == true &&
    facts["pinnedAXWindowMember"] as? Bool == false && facts["minimizedQueryError"] as? Int == 0 &&
    facts["minimizedReadable"] as? Bool == true && facts["minimized"] as? Bool == false &&
    facts["pinnedCGWindowListReadable"] as? Bool == true && facts["pinnedCGWindowCount"] as? Int == 1
}
func pinnedCGWindowCount(_ rows:[[String:Any]]?, _ pid:pid_t, _ window:UInt32) -> Int? {
  guard let rows else { return nil }
  return rows.filter { $0[kCGWindowNumber as String] as? UInt32 == window &&
    ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0 }.count
}
func transitionObservationStatus(_ samples: [[String:Any]], _ deadlineExceeded: Bool = false) -> String {
  if deadlineExceeded { return "deadline" }
  guard samples.count == 2 else { return "inconclusive" }
  for sample in samples {
    guard sample["axPidMatches"] as? Bool == true,
      sample["applicationWindowsQueryError"] as? Int == 0,
      sample["applicationWindowsReadable"] as? Bool == true,
      sample["minimizedQueryError"] as? Int == 0,
      sample["minimizedReadable"] as? Bool == true,
      sample["minimized"] as? Bool == false,
      sample["pinnedCGWindowListReadable"] as? Bool == true,
      sample["pinnedCGWindowCount"] as? Int == 1,
      sample["retainedMainChildrenQueryError"] as? Int == 0,
      sample["retainedMainChildrenReadable"] as? Bool == true else { return "inconclusive" }
  }
  if samples.contains(where: { $0["pinnedAXWindowMember"] as? Bool == true }) { return "membership-returned" }
  if samples.allSatisfy({ $0["pinnedAXWindowMember"] as? Bool == false }) { return "persistent-omission" }
  return "inconclusive"
}
enum BoundedWorkerWait { case completed(Data), timedOut, failed }
enum BoundedWorkerResult { case completed(Data), deadline(cleanupComplete:Bool), failed }
func superviseBoundedWorker(deadline:UInt64, now:()->UInt64,
  waitFor:(UInt64)->BoundedWorkerWait, terminateAndCollect:()->Bool, killAndCollect:()->Bool) -> BoundedWorkerResult {
  let current=now(), remaining=deadline > current ? deadline-current : 0
  switch waitFor(remaining) {
  case .failed: return .failed
  case .completed(let data):
    return now() < deadline ? .completed(data) : .deadline(cleanupComplete:true)
  case .timedOut:
    if terminateAndCollect() { return .deadline(cleanupComplete:true) }
    return .deadline(cleanupComplete:killAndCollect())
  }
}
func xy(_ node: AXUIElement) -> CGPoint? {
  guard let raw = attr(node, kAXPositionAttribute), CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
  var value = CGPoint.zero
  return AXValueGetValue(raw as! AXValue, .cgPoint, &value) ? value : nil
}
func wh(_ node: AXUIElement) -> CGSize? {
  guard let raw = attr(node, kAXSizeAttribute), CFGetTypeID(raw) == AXValueGetTypeID() else { return nil }
  var value = CGSize.zero
  return AXValueGetValue(raw as! AXValue, .cgSize, &value) ? value : nil
}
func emit(_ value: [String: Any]) throws {
  let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
  print(String(data: data, encoding: .utf8)!); fflush(stdout)
}
func cgWindowSampleWorker(_ arguments:[String]) throws {
  try check(arguments.count == 4 && arguments[1] == "--p38-cg-sample")
  guard let pid=Int32(arguments[2]),pid>0,let window=UInt32(arguments[3]),window>0 else { throw Refusal(code:"adapter-refused") }
  let rows=CGWindowListCopyWindowInfo([.optionAll,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]]
  guard let count=pinnedCGWindowCount(rows,pid,window) else {
    try emit(["schema":"nemo.p38-cg-sample/1","pid":Int(pid),"windowId":Int(window),"readable":false]);return
  }
  try emit(["schema":"nemo.p38-cg-sample/1","pid":Int(pid),"windowId":Int(window),"readable":true,"count":count])
}
func boundedPinnedCGWindowSample(_ pid:pid_t, _ window:UInt32, _ deadline:UInt64) -> [String:Any] {
  let now = { DispatchTime.now().uptimeNanoseconds }
  func failed() -> [String:Any] { ["pinnedCGWindowListReadable":false,"cgWorkerStatus":"failed","cgWorkerCleanup":"complete"] }
  guard now() < deadline, let executable=Bundle.main.executableURL else {
    return ["pinnedCGWindowListReadable":false,"cgWorkerStatus":"deadline","cgWorkerCleanup":"not-started","sampleDeadlineReached":true]
  }
  let process=Process(), output=Pipe(), exited=DispatchSemaphore(value:0)
  process.executableURL=executable
  process.arguments=["--p38-cg-sample",String(pid),String(window)]
  process.environment=[:]
  process.standardInput=FileHandle.nullDevice
  process.standardOutput=output
  process.standardError=FileHandle.nullDevice
  process.terminationHandler={ _ in exited.signal() }
  do { try process.run() } catch { return failed() }
  let result=superviseBoundedWorker(deadline:deadline,now:now,waitFor:{ remaining in
    let nanos=min(remaining,UInt64(Int.max))
    guard exited.wait(timeout: .now() + .nanoseconds(Int(nanos))) == .success else { return .timedOut }
    guard process.terminationReason == .exit && process.terminationStatus == 0 else { return .failed }
    let data=output.fileHandleForReading.readDataToEndOfFile()
    return data.count <= 4096 ? .completed(data) : .failed
  },terminateAndCollect:{
    if process.isRunning { process.terminate() }
    return exited.wait(timeout: .now() + .milliseconds(40)) == .success || !process.isRunning
  },killAndCollect:{
    if process.isRunning { _ = Darwin.kill(process.processIdentifier,SIGKILL) }
    return exited.wait(timeout: .now() + .milliseconds(40)) == .success || !process.isRunning
  })
  switch result {
  case .failed: return failed()
  case .deadline(let cleanupComplete):
    return ["pinnedCGWindowListReadable":false,"cgWorkerStatus":"deadline",
      "cgWorkerCleanup":cleanupComplete ? "complete" : "incomplete","sampleDeadlineReached":true]
  case .completed(let data):
    guard let object=try? JSONSerialization.jsonObject(with:data),let value=object as? [String:Any],
      value["schema"] as? String == "nemo.p38-cg-sample/1",value["pid"] as? Int == Int(pid),
      value["windowId"] as? Int == Int(window),let readable=value["readable"] as? Bool else { return failed() }
    var facts:[String:Any]=["pinnedCGWindowListReadable":readable,"cgWorkerStatus":"complete","cgWorkerCleanup":"complete"]
    if readable {
      guard let count=value["count"] as? Int,count>=0,count<=128 else { return failed() }
      facts["pinnedCGWindowCount"]=count
    }
    return facts
  }
}
func selfTest() throws {
  try ownedChildrenQuerySelfTest()
  for fault in ["none","foreign","window","parent","child","cycle","unreadable","oversized"] {
    var reads = 0
    let ok = ownedAncestor(3,1,equal:==,owned:{ $0 != (fault == "foreign" ? 3 : -1) },
      isWindow:{ _ in reads += 1; return fault == "window" },
      parent:{ item in fault == "parent" ? nil : (fault == "cycle" ? 3 : item-1) },
      children:{ item in fault == "unreadable" ? nil : (fault == "oversized" ? Array(repeating:3,count:513) : [fault == "child" ? 99 : item+1]) })
    try check(ok == (fault == "none"))
    if fault == "foreign" { try check(reads == 0) }
  }
  try check(locationContextValid(1,true,true,true))
  try check(!locationContextValid(2,true,true,true) && !locationContextValid(1,false,true,true) &&
    !locationContextValid(1,true,false,true) && !locationContextValid(1,true,true,false))
  var foreignAXReads=0,serviceOwnerObservations=[Int]()
  let processA=ProcessStart(pid:77,seconds:100,microseconds:20),processB=ProcessStart(pid:77,seconds:101,microseconds:20)
  try check(panelServiceIdentityValid(77,processA,77,processA,"com.apple.appkit.xpc.openAndSavePanelService",true))
  try check(!panelServiceIdentityValid(78,processA,77,processA,"com.apple.appkit.xpc.openAndSavePanelService",true) &&
    !panelServiceIdentityValid(77,processB,77,processA,"com.apple.appkit.xpc.openAndSavePanelService",true) &&
    !panelServiceIdentityValid(77,processA,77,processA,"com.apple.other",true) &&
    !panelServiceIdentityValid(77,processA,77,processA,"com.apple.appkit.xpc.openAndSavePanelService",false))
  try check(attachedServiceParentValid(true,10,10,true))
  for facts in [(false,10,true),(true,11,true),(true,10,false),(true,0,true)] as [(Bool,pid_t,Bool)] {
    try check(!attachedServiceParentValid(facts.0,facts.1,10,facts.2))
  }
  try check(uniqueFocusedField(1,true) && !uniqueFocusedField(0,false) &&
    !uniqueFocusedField(2,true) && !uniqueFocusedField(1,false))
  let parentMap=[2:1,3:2,4:1,5:3]
  let ownerMap=[1:10,2:10,3:20,4:21,5:20]
  func ownerFor(_ node:Int)->(Int,Bool,Int) { let p=ownerMap[node] ?? 0;serviceOwnerObservations.append(node);return (p == 0 ? -25204:0,p == 10,p) }
  func serviceOwnerFor(_ node:Int)->(Int,Bool,Int) { let p=ownerMap[node] ?? 0;return (p == 0 ? -25204:0,p == 20,p) }
  func childrenFor(_ node:Int)->(Int,[Int]?) { if node == 3 || node == 4 { foreignAXReads += 1 };return (0,node == 1 ? [2] : node == 2 ? [3] : node == 3 ? [5] : []) }
  func parentFor(_ node:Int)->(Int,Int?) { (parentMap[node] == nil ? -25204:0,parentMap[node]) }
  let attached=try attachedForeignRoot(1,owner:10,deadline:10,now:{ 1 },equal:==,owner:ownerFor,children:childrenFor,parent:parentFor)
  try check(attached.root == 3 && attached.parent == 2 && attached.pid == 20 && attached.route == "0.0" && foreignAXReads == 0)
  let serviceTree=try boundedOwnedTree(3,owner:20,deadline:10,now:{ 1 },equal:==,owner:serviceOwnerFor,children:childrenFor,parent:parentFor)
  try check(serviceTree == [3,5] && foreignAXReads == 1)
  for fault in ["multiple","broken-edge","unknown-owner","foreign-descendant"] {
    var acted=false,refused=false
    let badOwner:(Int)->(Int,Bool,Int)={ node in
      if fault == "unknown-owner" && node == 3 { return (-25204,false,0) }
      if fault == "foreign-descendant" && node == 5 { return (0,false,99) }
      return (0,ownerMap[node] == 10,ownerMap[node] ?? 0)
    }
    let badServiceOwner:(Int)->(Int,Bool,Int)={ node in
      if fault == "foreign-descendant" && node == 5 { return (0,false,99) }
      return (0,ownerMap[node] == 20,ownerMap[node] ?? 0)
    }
    let badChildren:(Int)->(Int,[Int]?)={ node in
      if fault == "multiple" && node == 1 { return (0,[2,4]) }
      return childrenFor(node)
    }
    let badParent:(Int)->(Int,Int?)={ node in fault == "broken-edge" && node == 2 ? (0,9):parentFor(node) }
    do {
      let root=try attachedForeignRoot(1,owner:10,deadline:10,now:{ 1 },equal:==,owner:badOwner,children:badChildren,parent:badParent)
      if fault == "foreign-descendant" { _ = try boundedOwnedTree(root.root,owner:20,deadline:10,now:{ 1 },equal:==,owner:badServiceOwner,children:childrenFor,parent:parentFor) }
      try guardedElementAction({ root.pid == 20 },{ acted=true })
    } catch { refused=true }
    try check(refused && !acted)
  }
  var staleAction=false,staleRefused=false
  do { try guardedElementAction({ false },{ staleAction=true }) } catch { staleRefused=true }
  try check(staleRefused && !staleAction)
  for action in ["press","set","select"] {
    var actionsPerformed=0,refused=false
    do {
      try requireResolvedSheetMutation("sheet",action,true,false)
      actionsPerformed += 1
    } catch let error as ActionRefusal {
      refused = error.diagnostics["gotoReason"] as? String == "unresolved-transition"
    }
    try check(refused && actionsPerformed == 0)
  }
  try check(!unresolvedServiceBlocksSheetMutation("sheet","press",false,false) &&
    !unresolvedServiceBlocksSheetMutation("sheet","press",true,true) &&
    !unresolvedServiceBlocksSheetMutation("sheet","inspect",true,false) &&
    !unresolvedServiceBlocksSheetMutation("main","press",true,false))
  for fault in ["none","p38","repeat","pid","window","focus","ancestor","layout","after-action","duplicate","search","absent","timeout"] {
    var attempted = fault == "repeat", events = [String](), contexts = 0, refused = false
    let trace = LocationTrace()
    do {
      try trace.observe { try goToFolder(fault != "p38",attempted:attempted,trace:trace,consume:{ attempted = true },context:{
        contexts += 1; events.append("context")
        try locationCheck(!["pid","window","focus","ancestor","layout"].contains(fault) && !(fault == "after-action" && contexts == 2),"goto-context")
      },perform:{ events.append("ax-press") },discover:{
        events.append("discover"); try locationCheck(!["duplicate","search","absent","timeout"].contains(fault),"goto-discovery")
      }) }
    } catch let error as ActionRefusal {
      refused = true
      try check(error.diagnostics["commandPosted"] as? Bool == events.contains("ax-press"))
      if fault == "after-action" { try check(error.diagnostics["gotoPhase"] as? String == "post-context") }
    }
    try check(refused == (fault != "none"))
    try check(trace.actionPerformed == events.contains("ax-press"))
    if events.contains("ax-press") { try check(events.filter { $0 == "ax-press" }.count == 1) }
    else { try check(!events.contains("discover")) }
    if fault == "after-action" { try check(events == ["context","ax-press","context"]) }
    if fault == "none" { try check(events == ["context","ax-press","context","discover"]) }
    if fault != "p38" { try check(attempted) }
  }
  let walkCases = ["none","unsupported-leaf","novalue-leaf","node-owner-query","node-owner-mismatch","child-owner-query","child-owner-mismatch",
    "parent-query","parent-shape","parent-owner-query","parent-owner-mismatch","parent-edge","children-query","children-shape","children-limit",
    "timeout-config","standalone-window","cycle","depth-limit","node-limit","walk-deadline","final-deadline"]
  for fault in walkCases {
    var ownerReads = [Int:Int](), attributeReads = [Int](), ticks = 0, diagnostic: [String:Any]?
    do {
      let walked = try locationWalk(0,deadline:10,now:{ ticks += 1; return fault == "walk-deadline" || fault == "final-deadline" && ticks == 3 ? 10 : 0 },
        equal:==,owner:{ node in
          ownerReads[node,default:0] += 1
          let target = node == 0 && ownerReads[node] == 1 ? "node" : node == 0 && ownerReads[node] == 2 ? "parent" : "child"
          if fault == target+"-owner-query" { return (-25204,false,0) }
          return (0,fault != target+"-owner-mismatch",node == 0 ? 123 : 456)
        },isWindow:{ node in attributeReads.append(node); return fault == "standalone-window" && node == 1 },
        timeout:{ node in attributeReads.append(node); return fault == "timeout-config" ? -25204 : 0 },children:{ node in
          attributeReads.append(node)
          if fault == "unsupported-leaf" && node == 1 { return (-25205,nil) }
          if fault == "novalue-leaf" && node == 1 { return (-25212,nil) }
          if fault == "children-query" { return (-25204,nil) }
          if fault == "children-shape" { return (0,nil) }
          if fault == "children-limit" { return (0,Array(repeating:1,count:513)) }
          if fault == "node-limit" { return (0,node == 0 ? Array(1...512) : []) }
          if fault == "depth-limit" { return (0,node < 13 ? [node+1] : []) }
          if fault == "cycle" { return (0,[1-node]) }
          return (0,node == 0 ? [1] : [])
        },parent:{ node in
          attributeReads.append(node)
          if fault == "parent-query" { return (-25204,nil) }
          if fault == "parent-shape" { return (0,nil) }
          if fault == "parent-edge" { return (0,2) }
          if fault == "cycle" { return (0,1-node) }
          return (0,fault == "depth-limit" ? node-1 : 0)
        })
      try check(walked == [0,1] && ["none","unsupported-leaf","novalue-leaf"].contains(fault))
    } catch let error as ActionRefusal { diagnostic = error.diagnostics }
    if ["none","unsupported-leaf","novalue-leaf"].contains(fault) { try check(diagnostic == nil) }
    else { try check(diagnostic?["gotoReason"] as? String == (fault == "final-deadline" ? "walk-deadline" : fault)) }
    if fault.hasPrefix("node-owner-") { try check(attributeReads.isEmpty) }
    if fault.hasPrefix("child-owner-") { try check(!attributeReads.contains(1) && diagnostic?["gotoRoute"] as? String == "0") }
    if fault == "depth-limit" { try check(diagnostic?["gotoDepth"] as? Int == 13) }
    if fault == "node-limit" { try check(diagnostic?["gotoNodeCount"] as? Int == 512) }
  }
  // Reproduce the same unowned child before and after the command through the
  // production walk and sequencer, rather than manufacturing a receipt result.
  for posted in [false,true] {
    let trace = LocationTrace(); var events = [String](), refused = false
    func failingWalk() throws {
      let ownership: (Int)->(Int,Bool,Int) = { (0,$0 == 0,$0 == 0 ? 123 : 456) }
      let descendants: (Int)->(Int,[Int]?) = { (0,$0 == 0 ? [1] : []) }
      let ancestor: (Int)->(Int,Int?) = { _ in (0,0) }
      let _: [Int] = try locationWalk(0,deadline:10,now:{ 0 },equal:{ $0 == $1 },owner:ownership,
        isWindow:{ _ in false },timeout:{ _ in 0 },children:descendants,parent:ancestor)
    }
    do { try trace.observe {
      trace.phase = "pre-tree"
      if !posted { try failingWalk() }
      try goToFolder(true,attempted:false,trace:trace,consume:{},context:{},perform:{ events.append("ax-press") },discover:failingWalk)
    } } catch let error as ActionRefusal {
      refused = true
      try check(error.diagnostics["gotoReason"] as? String == "child-owner-mismatch" && error.diagnostics["gotoRoute"] as? String == "0")
      try check(error.diagnostics["commandPosted"] as? Bool == posted && error.diagnostics["gotoPhase"] as? String == (posted ? "discovery" : "pre-tree"))
    }
    try check(refused && events == (posted ? ["ax-press"] : []))
  }
  // The old walk had no private owner observation at all: this oracle is RED
  // there, while preserving the same public refusal and zero foreign AX reads.
  for control in ["foreign-child","unknown-child","owner-query-error","owned-child","foreign-modal"] {
    var provenance = [(String,Int)](), foreignReads = 0, diagnostic: [String:Any]?
    let trace = LocationTrace(); trace.phase = "pre-tree"
    do { try trace.observe {
      let _: [Int] = try locationWalk(0,deadline:10,now:{ 0 },equal:==,owner:{ node in
        if node == 0 { return (0,true,123) }
        if control == "owner-query-error" { return (-25204,false,0) }
        if control == "owned-child" { return (0,true,123) }
        return (0,false,control == "unknown-child" ? 0 : 456)
      },foreignChild:{ provenance.append(($0,$1)) },isWindow:{ node in
        if node == 1 { foreignReads += 1 }; return node == 1 && control == "foreign-modal"
      },timeout:{ _ in 0 },children:{ ($0 == 0 ? (0,[1]) : (0,[])) },parent:{ _ in (0,0) })
    } } catch let error as ActionRefusal { diagnostic = error.diagnostics }
    if control == "owned-child" { try check(diagnostic == nil && provenance.isEmpty && foreignReads == 1) }
    else {
      let queried = control != "owner-query-error"
      try check(diagnostic?["gotoReason"] as? String == (queried ? "child-owner-mismatch" : "child-owner-query"))
      try check(diagnostic?["gotoRoute"] as? String == "0" && diagnostic?["ownerMatches"] as? Bool == false &&
        diagnostic?["commandPosted"] as? Bool == false && foreignReads == 0)
      try check(provenance.count == (queried ? 1 : 0))
      if queried { try check(provenance[0].0 == "0" && provenance[0].1 == (control == "unknown-child" ? 0 : 456)) }
    }
  }
  try check(foreignOwnerPathClass(nil) == "process-path-unreadable")
  try check(foreignOwnerPathClass("/System/Library/Frameworks/AppKit.framework/XPCServices/com.apple.appkit.xpc.openAndSavePanelService.xpc/Contents/MacOS/service") == "appkit-open-save-panel-path")
  try check(foreignOwnerPathClass("/usr/libexec/example") == "system-path")
  try check(foreignOwnerPathClass("/private/other") == "other-path")
  let privateRoot = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
  try FileManager.default.createDirectory(at:privateRoot,withIntermediateDirectories:false,attributes:[.posixPermissions:0o700])
  defer { try? FileManager.default.removeItem(at:privateRoot) }
  let privateIdentity = try FileManager.default.attributesOfItem(atPath:privateRoot.path)
  recordForeignOwner(0,route:"0.1",report:privateRoot,identity:privateIdentity)
  let privateFile = privateRoot.appendingPathComponent("ax-owner-provenance.private.json")
  let privateFacts = try JSONSerialization.jsonObject(with:Data(contentsOf:privateFile)) as? [String:Any]
  let privateMode = try FileManager.default.attributesOfItem(atPath:privateFile.path)[.posixPermissions] as? NSNumber
  try check(privateFacts?["route"] as? String == "0.1" && privateFacts?["observedPid"] as? Int == 0 &&
    privateFacts?["pathClass"] as? String == "process-path-unreadable" && privateMode?.intValue == 0o600)
  try check(locationCandidateValid(true,true,true,true,1,"plain",true,true,true))
  for index in 0..<9 {
    try check(!locationCandidateValid(index != 0,index != 1,index != 2,index != 3,index == 4 ? 2 : 1,
      index == 5 ? "search" : "plain",index != 6,index != 7,index != 8))
  }
  try check(textFieldKind(.success,kAXSearchFieldSubrole) == "search")
  try check(textFieldKind(.attributeUnsupported,nil) == "plain")
  try check(textFieldKind(.cannotComplete,nil) == "unreadable")
  try check(textFieldKind(.success,"AXSecureTextField") == "other")
  for fault in ["none","search","disabled","stale-focus","confirm-support","value-settable","write","read","mismatch","confirm",
    "identity-1","identity-2","identity-3","identity-4"] {
    var events = [String](), pins = 0
    let facts: [String:Any] = ["role":"AXTextField","textFieldKind":fault == "search" ? "search" : "plain",
      "enabled":fault != "disabled","confirmQueryError":0,"confirmAdvertised":fault != "confirm-support",
      "valueSettableQueryError":0,"valueSettable":fault != "value-settable",
      "focusSettableQueryError":-25205,"focusSettable":false,"focusedQueryError":0,"focusedMatches":fault != "stale-focus"]
    var result: [String:Any]?, failure: String?
    do { result = try enterSheetText(facts,identity:{ pins += 1; events.append("identity")
      try check(fault != "identity-\(pins)") },
      write:{ events.append("write"); return fault == "write" ? -25204 : 0 },
      read:{ events.append("read"); return (fault == "read" ? -25204 : 0,fault != "mismatch") },
      confirm:{ events.append("confirm"); return fault == "confirm" ? -25204 : 0 })
    } catch let error as ActionRefusal { failure = error.diagnostics["stage"] as? String }
    if fault == "none" {
      try check(result?["valueMatches"] as? Bool == true && result?["stage"] as? String == "axtext-confirm")
      try check(events == ["identity","identity","write","identity","read","identity","confirm"])
    } else {
      let expected = fault.hasPrefix("identity") ? "axtext-identity" :
      ["search":"axtext-role","disabled":"axtext-role","stale-focus":"axtext-role","confirm-support":"axtext-confirm-support",
         "value-settable":"axtext-value-settable","write":"axtext-write","read":"axtext-readback",
         "mismatch":"axtext-readback","confirm":"axtext-confirm"][fault]
      try check(failure == expected && result == nil)
      try check(events.contains("confirm") == (fault == "confirm"))
    }
  }
  for (mode,prefix,reference) in [("--persistent","NEMO_P38_",false),("--reference-persistent","NEMO_REFERENCE_",true)] {
    let scoped = try operatorScope(mode,[prefix+"GRANT":"scoped-run",prefix+"REPORT_ROOT":"/test-only"])
    try check(scoped.reference == reference && scoped.report == "/test-only")
    let wrongPrefix = reference ? "NEMO_P38_" : "NEMO_REFERENCE_"
    var denied = false
    do { _ = try operatorScope(mode,[wrongPrefix+"GRANT":"wrong-scope",wrongPrefix+"REPORT_ROOT":"/test-only"]) }
    catch { denied = true }; try check(denied)
  }
  try check(try route("0.12.3") == [0,12,3])
  for input in ["-1", "0..1", ".0", "1.", "x", "4096", "999999999999999999999", String(repeating: "1.", count: 21)] {
    var refused = false; do { _ = try route(input) } catch { refused = true }; try check(refused)
  }
  try check(!actions.contains("key") && !actions.contains("focus") && !actions.contains("launch"))
  let cgRows:[[String:Any]]=[[kCGWindowNumber as String:UInt32(9),kCGWindowOwnerPID as String:Int32(42),kCGWindowLayer as String:0],
    [kCGWindowNumber as String:UInt32(9),kCGWindowOwnerPID as String:Int32(43),kCGWindowLayer as String:0],
    [kCGWindowNumber as String:UInt32(9),kCGWindowOwnerPID as String:Int32(42),kCGWindowLayer as String:1]]
  try check(pinnedCGWindowCount(cgRows,42,9) == 1 && pinnedCGWindowCount(nil,42,9) == nil)
  for (ownerError,ownerMatches,windowsError,windowsReadable,member,minimizedError,minimizedReadable,minimized,cgReadable,cgCount,expected) in [
    (0,false,0,true,true,0,true,false,true,1,"ax-owner"), (0,true,-25205,true,true,0,true,false,true,1,"application-window-membership"),
    (0,true,0,false,true,0,true,false,true,1,"application-window-membership"), (0,true,0,true,false,0,true,false,true,1,"application-window-membership"),
    (0,true,0,true,true,0,true,false,false,0,"pinned-cg-window-list"), (0,true,0,true,true,0,true,false,true,0,"pinned-cg-window"),
    (0,true,0,true,true,0,true,true,true,1,"minimized"), (0,true,0,true,true,-25205,true,false,true,1,"minimized"),
    (0,true,0,true,true,0,false,false,true,1,"minimized")
  ] { try check(identityFailureStage(ownerError,ownerMatches,windowsError,windowsReadable,member,minimizedError,minimizedReadable,minimized,cgReadable,cgCount) == expected) }
  try check(identityFailureStage(0,true,0,true,true,0,true,false,true,1) == nil)
  try check(retainedChildrenProbeClass(-25205,false,false,false,0) == "children-query-error")
  try check(!diagnosticSheetMatch(["candidatePidMatches":false,"candidateRoleReadable":true,"candidateRole":"AXSheet",
    "candidateParentReadable":true,"candidateParentPidMatches":true,"candidateParentIsRetainedMain":true]))
  try check(!diagnosticSheetMatch(["candidatePidMatches":true,"candidateRoleReadable":true,"candidateRole":"AXSheet",
    "candidateParentReadable":true,"candidateParentPidMatches":true,"candidateParentIsRetainedMain":false]))
  try check(diagnosticSheetMatch(["candidatePidMatches":true,"candidateRoleReadable":true,"candidateRole":"AXSheet",
    "candidateParentReadable":true,"candidateParentPidMatches":true,"candidateParentIsRetainedMain":true]))
  try check(retainedChildrenProbeClass(0,true,false,false,0) == "no-attached-sheet")
  try check(retainedChildrenProbeClass(0,true,false,false,2) == "multiple-attached-sheets")
  try check(retainedChildrenProbeClass(0,true,false,false,1) == "one-attached-sheet")
  try check(retainedChildrenProbeClass(0,true,true,false,1) == "children-truncated")
  try check(retainedChildrenProbeClass(0,true,false,true,1) == "candidate-probe-incomplete")
  let membershipFacts: [String:Any] = ["axPidMatches":true,"applicationWindowsQueryError":0,
    "applicationWindowsReadable":true,"pinnedAXWindowMember":false,"minimizedQueryError":0,
    "minimizedReadable":true,"minimized":false,"pinnedCGWindowListReadable":true,"pinnedCGWindowCount":1]
  try check(shouldProbeRetainedMainChildren("application-window-membership",membershipFacts))
  try check(!shouldProbeRetainedMainChildren("application-window-membership",["axPidMatches":true,
    "applicationWindowsQueryError":-25205,"applicationWindowsReadable":false,"pinnedAXWindowMember":false]))
  try check(!shouldProbeRetainedMainChildren("application-window-membership",["axPidMatches":false,
    "applicationWindowsQueryError":0,"applicationWindowsReadable":true,"pinnedAXWindowMember":false]))
  try check(!shouldProbeRetainedMainChildren("application-window-membership",["axPidMatches":true,
    "applicationWindowsQueryError":0,"applicationWindowsReadable":true,"pinnedAXWindowMember":true]))
  try check(!shouldProbeRetainedMainChildren("application-window-membership",["axPidMatches":true,
    "applicationWindowsQueryError":0,"applicationWindowsReadable":true,"pinnedAXWindowMember":false,
    "minimizedQueryError":0,"minimizedReadable":true,"minimized":false,"pinnedCGWindowListReadable":true,"pinnedCGWindowCount":0]))
  try check(!shouldProbeRetainedMainChildren("minimized",membershipFacts))
  let stableTransitionSample: [String:Any] = ["axPidMatches":true,"applicationWindowsQueryError":0,
    "applicationWindowsReadable":true,"pinnedAXWindowMember":false,"minimizedQueryError":0,
    "minimizedReadable":true,"minimized":false,"pinnedCGWindowListReadable":true,"pinnedCGWindowCount":1,
    "retainedMainChildrenQueryError":0,"retainedMainChildrenReadable":true]
  try check(transitionObservationStatus([stableTransitionSample]) == "inconclusive")
  try check(transitionObservationStatus([stableTransitionSample,stableTransitionSample]) == "persistent-omission")
  var restoredTransitionSample = stableTransitionSample; restoredTransitionSample["pinnedAXWindowMember"] = true
  try check(transitionObservationStatus([stableTransitionSample,restoredTransitionSample]) == "membership-returned")
  var driftTransitionSample = stableTransitionSample; driftTransitionSample["axPidMatches"] = false
  try check(transitionObservationStatus([stableTransitionSample,driftTransitionSample]) == "inconclusive")
  var errorTransitionSample = stableTransitionSample; errorTransitionSample["applicationWindowsQueryError"] = -25205
  try check(transitionObservationStatus([stableTransitionSample,errorTransitionSample]) == "inconclusive")
  try check(transitionObservationStatus([stableTransitionSample,stableTransitionSample,stableTransitionSample]) == "inconclusive")
  var fakeNow:UInt64=10,termCount=0,killCount=0
  let timely=superviseBoundedWorker(deadline:20,now:{fakeNow},waitFor:{ remaining in
    if remaining != 10 { return .failed };fakeNow=19;return .completed(Data([1]))
  },terminateAndCollect:{termCount+=1;return true},killAndCollect:{killCount+=1;return true})
  if case .completed(let data)=timely { try check(data == Data([1])) } else { throw Refusal(code:"adapter-refused") }
  fakeNow=10
  let late=superviseBoundedWorker(deadline:20,now:{fakeNow},waitFor:{ _ in
    fakeNow=21
    return .completed(Data([2]))
  },
    terminateAndCollect:{termCount+=1;return true},killAndCollect:{killCount+=1;return true})
  if case .deadline(let cleanup)=late { try check(cleanup) } else { throw Refusal(code:"adapter-refused") }
  fakeNow=10
  let timedOut=superviseBoundedWorker(deadline:20,now:{fakeNow},waitFor:{ _ in .timedOut },
    terminateAndCollect:{termCount+=1;return false},killAndCollect:{killCount+=1;return false})
  if case .deadline(let cleanup)=timedOut { try check(!cleanup) } else { throw Refusal(code:"adapter-refused") }
  try check(termCount == 1 && killCount == 1)
  var returnedAfterBudget = stableTransitionSample; returnedAfterBudget["pinnedAXWindowMember"] = true
  try check(transitionObservationStatus([stableTransitionSample,returnedAfterBudget],true) == "deadline")
  try check(try sheetProof([11],[11:123],[11:1],123,1,nil) == 11)
  try check(try sheetProof([11],[11:123],[11:1],123,1,11) == 11)
  for (members,owners,parents,retained) in [([], [11:123], [11:1], nil), ([11,12],[11:123,12:123],[11:1,12:1],nil), ([11],[11:124],[11:1],nil),
    ([12],[11:123,12:123],[11:1,12:1],11), ([],[11:123],[11:1],11), ([11],[11:123],[11:999],nil), ([11],[11:123],[:],11)] as [([Int],[Int:Int],[Int:Int],Int?)] {
    var refused = false; do { _ = try sheetProof(members,owners,parents,123,1,retained) } catch { refused = true }; try check(refused)
  }
  var reads = 0
  for owner in [124,nil] as [pid_t?] {
    let fact = localFacts("0",owner,123) { reads += 1; return ["control":"private"] }
    try check(fact["blocked"] as? String == "child-owner" && reads == 0)
  }
  _ = localFacts("0",123,123) { reads += 1; return ["control":"Open"] }; try check(reads == 1)
  try projectionLimit(5,128)
  var refusedProjection = false; do { try projectionLimit(6,128) } catch { refusedProjection = true }; try check(refusedProjection)
  refusedProjection = false; do { try projectionLimit(1,129) } catch { refusedProjection = true }; try check(refusedProjection)
  for (stage,classes) in inspectionQueryClasses { for queryClass in classes {
    try check(safeInspectionContext(stage,queryClass) != nil)
  } }
  try check(safeInspectionContext("private-path","private-value") == nil)
  try check(inspectionDiagnosticForRefusal("adapter-refused","main-children-query","attribute-query-failed") != nil)
  try check(inspectionDiagnosticForRefusal("sheet-owner","main-children-query","attribute-query-failed") == nil)
  try check(inspectionDiagnosticForRefusal("adapter-refused","private-path","private-value") == nil)
  try emit(["status":"static-model-pass", "routeNegatives":8, "sheetNegatives":7, "identityPredicateNegatives":9,
    "retainedMainProbeCases":15,"transitionObservationCases":6,"workerDeadlineCases":3,"textEntryCases":13,"textFieldKindCases":4,
    "goToFolderSequenceCases":13,"locationWalkCases":walkCases.count,"locationPhaseWalkCases":2,
    "privateOwnerCases":5,"privatePathClassCases":4,"locationCandidateCases":10,"locationAncestryCases":8,"locationContextCases":5,
    "serviceIdentityCases":5,"serviceAttachmentCases":5,"focusedFieldCases":4,"unresolvedTransitionCases":3,
    "foreignReads":0,"runtimeAX":false])
}
func run() throws {
  let args = CommandLine.arguments
  if args == [args[0], "--self-test"] { try selfTest(); return }
  #if UI32_VALIDATION
  try check(args.count > 1 && ui32ModeAllowed(args[1]))
  if args.count > 1 && args[1] == "--ui32-self-test" { try ui32SelfTest(); return }
  if args.count > 1 && args[1] == "--ui32-persistent" { try runUI32(args,ProcessInfo.processInfo.environment); return }
  throw Refusal(code:"adapter-refused")
  #else
  if args.count == 4 && args[1] == "--p38-cg-sample" { try cgWindowSampleWorker(args); return }
  #endif
  let initialWindow = try discoverOwnedInitialWindow(args,ProcessInfo.processInfo.environment)
  let scope = initialWindow.scope, pid = initialWindow.pid, pinnedProcessStart = initialWindow.start
  let number = initialWindow.number, physical = initialWindow.physical, reportIdentity = initialWindow.reportIdentity
  let application = initialWindow.application, main = initialWindow.main, cg = initialWindow.cg
  var projectionCursor: OwnedProjectionCursor<AXUIElement>? = nil
  var ordinal = 0
  var sheet: AXUIElement? = nil, sheetToken: String? = nil
  var location: AXUIElement? = nil, locationField: AXUIElement? = nil, locationToken: String? = nil
  var serviceRoot:AXUIElement? = nil, serviceParent:AXUIElement? = nil, servicePID:pid_t? = nil
  var serviceStart:ProcessStart? = nil, serviceSheetToken:String? = nil
  var locationAttempted = false
  func descendant(_ candidate: AXUIElement, _ ancestor: AXUIElement) -> Bool {
    ownedAncestor(candidate,ancestor,equal:{ CFEqual($0,$1) },owned:{ owns($0,pid) },
      isWindow:{ attr($0,kAXRoleAttribute) as? String == kAXWindowRole },
      parent:{ axElement(attr($0,kAXParentAttribute)) },children:{ attr($0,kAXChildrenAttribute) as? [AXUIElement] })
  }
  // Public SDK defines AXSheet + AXParent/AXChildren, not an AXSheets constant.
  // Derive attachment from that documented relationship, never same-PID alone.
  func members(inspectMain: Bool = false) throws -> [AXUIElement] {
    var rawChildren: CFTypeRef?
    let childrenQuery = AXUIElementCopyAttributeValue(main,kAXChildrenAttribute as CFString,&rawChildren)
    guard childrenQuery == .success else {
      if inspectMain { throw InspectionRefusal(stage:"main-children-query",queryClass:"attribute-query-failed") }
      throw Refusal(code:"adapter-refused")
    }
    guard let childRows = rawChildren as? [AXUIElement] else {
      if inspectMain { throw InspectionRefusal(stage:"main-children-value",queryClass:"attribute-value-unreadable") }
      throw Refusal(code:"adapter-refused")
    }
    var rawWindows: CFTypeRef?
    let windowsQuery = AXUIElementCopyAttributeValue(application,kAXWindowsAttribute as CFString,&rawWindows)
    guard windowsQuery == .success else {
      if inspectMain { throw InspectionRefusal(stage:"application-windows-query",queryClass:"attribute-query-failed") }
      throw Refusal(code:"adapter-refused")
    }
    guard let appRows = rawWindows as? [AXUIElement] else {
      if inspectMain { throw InspectionRefusal(stage:"application-windows-value",queryClass:"attribute-value-unreadable") }
      throw Refusal(code:"adapter-refused")
    }
    var attached = [AXUIElement]()
    for candidate in childRows + appRows {
      if !owns(candidate,pid) { throw Refusal(code:"sheet-owner") }
      if attr(candidate,kAXRoleAttribute) as? String != kAXSheetRole { continue }
      guard let rawParent = attr(candidate,kAXParentAttribute), CFGetTypeID(rawParent) == AXUIElementGetTypeID() else { throw Refusal(code:"sheet-parent") }
      let parent = rawParent as! AXUIElement
      if scope.reference && locationAttempted, let outer = sheet, !CFEqual(candidate,outer), descendant(candidate,outer) { continue }
      if !owns(parent,pid) || !CFEqual(parent,main) { throw Refusal(code:"sheet-parent") }
      if !attached.contains(where:{ CFEqual($0,candidate) }) { attached.append(candidate) }
    }
    return attached
  }
  func verifySheet() throws -> AXUIElement {
    guard let retained = sheet else { throw Refusal(code:"adapter-refused") }
    let actual = try members()
    if actual.count != 1 || !owns(actual[0],pid) || !owns(retained,pid) || !CFEqual(actual[0],retained) { throw Refusal(code:"sheet-replaced") }
    return retained
  }
  func noSheet(inspectMain: Bool = false) throws {
    if inspectMain && sheet != nil { throw InspectionRefusal(stage:"main-sheet-state",queryClass:"retained-sheet-present") }
    try check(sheet == nil)
    setInspectionContext("main-children-query","attribute-query-failed")
    let actual = try members(inspectMain:inspectMain)
    if inspectMain && !actual.isEmpty { throw InspectionRefusal(stage:"main-sheet-state",queryClass:"attached-sheet-present") }
    try check(actual.isEmpty)
  }
  func retainedMainChildrenDiagnostic() -> [String:Any] {
    // Separate, count-bounded detail phase outside the transition-sample budget.
    let limit = 64
    var rawChildren: CFTypeRef?
    let query = AXUIElementCopyAttributeValue(main,kAXChildrenAttribute as CFString,&rawChildren)
    var evidence: [String:Any] = ["retainedMainChildrenQueryError":Int(query.rawValue),
      "retainedMainChildrenReadable":false,"retainedMainChildFacts":[]]
    guard query == .success else {
      evidence["retainedMainSheetProbe"] = retainedChildrenProbeClass(Int(query.rawValue),false,false,false,0)
      return evidence
    }
    guard let values = rawChildren as? [AXUIElement] else {
      evidence["retainedMainSheetProbe"] = retainedChildrenProbeClass(0,false,false,false,0)
      return evidence
    }
    let total = values.count
    let truncated = total > limit
    let bounded = min(total,limit)
    var candidates = [[String:Any]]()
    var incomplete = false
    var attachedSheets = 0
    for index in 0..<bounded {
      let child = values[index]
      var facts: [String:Any] = ["index":index]
      let childTimeout = AXUIElementSetMessagingTimeout(child,0.03)
      facts["candidateTimeoutConfigError"] = Int(childTimeout.rawValue)
      guard childTimeout == .success else { incomplete = true; candidates.append(facts); continue }
      var candidatePID: pid_t = 0
      let pidQuery = AXUIElementGetPid(child,&candidatePID)
      facts["candidatePidQueryError"] = Int(pidQuery.rawValue)
      guard pidQuery == .success else { incomplete = true; candidates.append(facts); continue }
      facts["candidatePid"] = Int(candidatePID)
      guard candidatePID > 0 else { incomplete = true; facts["candidatePidMatches"] = false; candidates.append(facts); continue }
      let samePID = candidatePID == pid
      facts["candidatePidMatches"] = samePID
      guard samePID else { candidates.append(facts); continue }

      var rawRole: CFTypeRef?
      let roleQuery = AXUIElementCopyAttributeValue(child,kAXRoleAttribute as CFString,&rawRole)
      facts["candidateRoleQueryError"] = Int(roleQuery.rawValue)
      guard roleQuery == .success else { incomplete = true; candidates.append(facts); continue }
      guard let role = rawRole as? String else {
        facts["candidateRoleReadable"] = false; incomplete = true; candidates.append(facts); continue
      }
      facts["candidateRoleReadable"] = true
      facts["candidateRole"] = role == kAXSheetRole ? "AXSheet" : "other"
      guard role == kAXSheetRole else { candidates.append(facts); continue }

      var rawParent: CFTypeRef?
      let parentQuery = AXUIElementCopyAttributeValue(child,kAXParentAttribute as CFString,&rawParent)
      facts["candidateParentQueryError"] = Int(parentQuery.rawValue)
      guard parentQuery == .success else {
        incomplete = true; candidates.append(facts); continue
      }
      guard let parent = axElement(rawParent) else {
        facts["candidateParentReadable"] = false; incomplete = true; candidates.append(facts); continue
      }
      facts["candidateParentReadable"] = true
      let parentTimeout = AXUIElementSetMessagingTimeout(parent,0.03)
      facts["candidateParentTimeoutConfigError"] = Int(parentTimeout.rawValue)
      guard parentTimeout == .success else { incomplete = true; candidates.append(facts); continue }
      var parentPID: pid_t = 0
      let parentPIDQuery = AXUIElementGetPid(parent,&parentPID)
      facts["candidateParentPidQueryError"] = Int(parentPIDQuery.rawValue)
      guard parentPIDQuery == .success else { incomplete = true; candidates.append(facts); continue }
      let parentPIDMatches = parentPID == pid
      let retainedParent = parentPIDMatches && CFEqual(parent,main)
      facts["candidateParentPidMatches"] = parentPIDMatches
      facts["candidateParentIsRetainedMain"] = retainedParent
      if diagnosticSheetMatch(facts) { attachedSheets += 1 }
      candidates.append(facts)
    }
    evidence["retainedMainChildrenReadable"] = true
    evidence["retainedMainChildrenCount"] = min(total,4096)
    evidence["retainedMainChildrenCountCapped"] = total > 4096
    evidence["retainedMainChildrenTruncated"] = truncated
    evidence["retainedMainChildFacts"] = candidates
    evidence["retainedMainSheetProbe"] = retainedChildrenProbeClass(0,true,truncated,incomplete,attachedSheets)
    return evidence
  }
  func transitionObservationDiagnostic() -> [String:Any] {
    let started = DispatchTime.now().uptimeNanoseconds
    let deadline = started + 750_000_000
    let timeout: Float = 0.03
    var timeoutsConfigured = false
    func elapsedMs() -> Int { Int((DispatchTime.now().uptimeNanoseconds-started)/1_000_000) }
    func expired() -> Bool { DispatchTime.now().uptimeNanoseconds >= deadline }
    func result(_ suggestedStatus: String, _ samples: [[String:Any]]) -> [String:Any] {
      let completed = DispatchTime.now().uptimeNanoseconds
      let overrun = completed >= deadline
      return ["status":overrun ? "deadline" : suggestedStatus,"budgetMs":750,
        "budgetKind":"parent-supervised-one-shot","budgetScope":"transition-samples-only",
        "axTimeoutsConfigured":timeoutsConfigured,"elapsedMs":Int((completed-started)/1_000_000),"samples":samples]
    }
    // AX message timeouts precede every data query in either post-refusal probe.
    if expired() { return result("deadline",[]) }
    let appTimeout = AXUIElementSetMessagingTimeout(application,timeout)
    if expired() { return result("deadline",[]) }
    guard appTimeout == .success else { return result("timeout-config-refused",[]) }
    let mainTimeout = AXUIElementSetMessagingTimeout(main,timeout)
    timeoutsConfigured = mainTimeout == .success
    if expired() { return result("deadline",[]) }
    guard timeoutsConfigured else { return result("timeout-config-refused",[]) }
    // The CG query runs in a one-shot child so the retained owner can enforce its deadline.
    func sample(_ index: Int) -> ([String:Any],Bool) {
      var facts: [String:Any] = ["sampleIndex":index,"elapsedMs":elapsedMs()]
      func reachedDeadline() -> Bool {
        facts["elapsedMs"] = elapsedMs()
        if expired() { facts["sampleDeadlineReached"] = true; return true }
        return false
      }
      if reachedDeadline() { return (facts,true) }
      var actualPID: pid_t = 0
      let ownerError = AXUIElementGetPid(main,&actualPID)
      facts["axPidQueryError"] = Int(ownerError.rawValue)
      facts["axPidMatches"] = ownerError == .success && actualPID == pid
      if reachedDeadline() { return (facts,true) }

      var rawWindows: CFTypeRef?
      let windowsError = AXUIElementCopyAttributeValue(application,kAXWindowsAttribute as CFString,&rawWindows)
      let windows = windowsError == .success ? rawWindows as? [AXUIElement] : nil
      facts["applicationWindowsQueryError"] = Int(windowsError.rawValue)
      facts["applicationWindowsReadable"] = windows != nil
      facts["pinnedAXWindowMember"] = windows?.contains { CFEqual($0,main) } ?? false
      if reachedDeadline() { return (facts,true) }

      var rawMinimized: CFTypeRef?
      let minimizedError = AXUIElementCopyAttributeValue(main,kAXMinimizedAttribute as CFString,&rawMinimized)
      let minimizedValue = minimizedError == .success ? (rawMinimized as? Bool) : nil
      facts["minimizedQueryError"] = Int(minimizedError.rawValue)
      facts["minimizedReadable"] = minimizedValue != nil
      if let minimizedValue { facts["minimized"] = minimizedValue }
      if reachedDeadline() { return (facts,true) }

      facts.merge(boundedPinnedCGWindowSample(pid,number,deadline)) { _,new in new }
      if facts["sampleDeadlineReached"] as? Bool == true || reachedDeadline() { return (facts,true) }

      var rawChildren: CFTypeRef?
      let childrenError = AXUIElementCopyAttributeValue(main,kAXChildrenAttribute as CFString,&rawChildren)
      let childRows = childrenError == .success ? rawChildren as? [AXUIElement] : nil
      facts["retainedMainChildrenQueryError"] = Int(childrenError.rawValue)
      facts["retainedMainChildrenReadable"] = childRows != nil
      if reachedDeadline() { return (facts,true) }
      if let childRows {
        facts["retainedMainChildrenCount"] = min(childRows.count,4096)
        facts["retainedMainChildrenCountCapped"] = childRows.count > 4096
      }
      return (facts,false)
    }
    var samples = [[String:Any]]()
    for index in 0..<2 {
      if index > 0 {
        let now = DispatchTime.now().uptimeNanoseconds
        let remaining = deadline > now ? deadline-now : 0
        if remaining == 0 { break }
        Thread.sleep(forTimeInterval:min(0.15,Double(remaining)/1_000_000_000))
      }
      if expired() { break }
      let (facts,overrun) = sample(index)
      samples.append(facts)
      if overrun { break }
    }
    let overrun = expired() || samples.contains { $0["sampleDeadlineReached"] as? Bool == true }
    let status = transitionObservationStatus(samples,overrun)
    return result(status,samples)
  }
  func verify() throws { try verifyOwnedWindow(initialWindow) }
  func node(_ value: String, _ root: AXUIElement, owner expectedPID:pid_t? = nil) throws -> AXUIElement {
    var current = root
    for index in try route(value) {
      let list = children(current); try check(list.indices.contains(index)); current = list[index]
      if !owns(current,expectedPID ?? pid) { throw Refusal(code:"child-owner") }
      // Routes begin at pinned main; standalone AX windows/panels cannot enter the route.
      if attr(current,kAXRoleAttribute) as? String == kAXWindowRole { throw Refusal(code:"standalone-window") }
    }
    return current
  }
  func supports(_ node: AXUIElement, _ action: String) -> Bool {
    var list: CFArray?; return AXUIElementCopyActionNames(node,&list) == .success && (list as? [String] ?? []).contains(action)
  }
  // Bounded, complete ownership walk. An unreadable/truncated topology is not
  // absence. Every edge must be attested by both AXChildren and AXParent.
  func locationTree(_ root: AXUIElement, _ deadline: UInt64 = DispatchTime.now().uptimeNanoseconds + 1_000_000_000,
    foreignChild: (String,Int)->Void = { _,_ in }) throws -> [AXUIElement] {
    try locationWalk(root,deadline:deadline,now:{ DispatchTime.now().uptimeNanoseconds },equal:{ CFEqual($0,$1) },owner:{ node in
      var actual: pid_t = 0; let error = AXUIElementGetPid(node,&actual)
      return (Int(error.rawValue),error == .success && actual == pid,Int(actual))
    },foreignChild:foreignChild,isWindow:{ attr($0,kAXRoleAttribute) as? String == kAXWindowRole },
      timeout:{ Int(AXUIElementSetMessagingTimeout($0,0.03).rawValue) },children:{ node in
      var raw: CFTypeRef?; let error = AXUIElementCopyAttributeValue(node,kAXChildrenAttribute as CFString,&raw)
      return (Int(error.rawValue),raw as? [AXUIElement])
    },parent:{ node in
      var raw: CFTypeRef?; let error = AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
      return (Int(error.rawValue),axElement(raw))
    })
  }
  func serviceOwnerTree(_ root:AXUIElement, _ ownerPID:pid_t) throws -> [AXUIElement] {
    try boundedOwnedTree(root,owner:Int(ownerPID),deadline:DispatchTime.now().uptimeNanoseconds+1_000_000_000,
      now:{ DispatchTime.now().uptimeNanoseconds },equal:{ CFEqual($0,$1) },owner:{ node in
        var actual:pid_t=0; let error=AXUIElementGetPid(node,&actual)
        return (Int(error.rawValue),error == .success && actual == ownerPID,Int(actual))
      },children:{ node in
        let timeout=AXUIElementSetMessagingTimeout(node,0.03)
        guard timeout == .success else { return (Int(timeout.rawValue),nil) }
        var raw:CFTypeRef?;let error=AXUIElementCopyAttributeValue(node,kAXChildrenAttribute as CFString,&raw)
        if error == .attributeUnsupported || error == .noValue { return (0,[]) }
        return (Int(error.rawValue),raw as? [AXUIElement])
      },parent:{ node in
        let timeout=AXUIElementSetMessagingTimeout(node,0.03)
        guard timeout == .success else { return (Int(timeout.rawValue),nil) }
        var raw:CFTypeRef?;let error=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
        return (Int(error.rawValue),axElement(raw))
      })
  }
  func discoverServiceRoot(_ outer:AXUIElement) throws -> (AXUIElement,AXUIElement,pid_t,ProcessStart) {
    let found=try attachedForeignRoot(outer,owner:Int(pid),deadline:DispatchTime.now().uptimeNanoseconds+1_000_000_000,
      now:{ DispatchTime.now().uptimeNanoseconds },equal:{ CFEqual($0,$1) },owner:{ node in
        var actual:pid_t=0;let error=AXUIElementGetPid(node,&actual)
        return (Int(error.rawValue),error == .success && actual == pid,Int(actual))
      },children:{ node in
        let timeout=AXUIElementSetMessagingTimeout(node,0.03)
        guard timeout == .success else { return (Int(timeout.rawValue),nil) }
        var raw:CFTypeRef?;let error=AXUIElementCopyAttributeValue(node,kAXChildrenAttribute as CFString,&raw)
        if error == .attributeUnsupported || error == .noValue { return (0,[]) }
        return (Int(error.rawValue),raw as? [AXUIElement])
      },parent:{ node in
        let timeout=AXUIElementSetMessagingTimeout(node,0.03)
        guard timeout == .success else { return (Int(timeout.rawValue),nil) }
        var raw:CFTypeRef?;let error=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
        return (Int(error.rawValue),axElement(raw))
      },foreignChild:{ route,observed in recordForeignOwner(observed,route:route,report:physical,identity:reportIdentity) })
    guard found.pid > 0,let start=processStart(pid_t(found.pid)),authenticatedPanelService(pid_t(found.pid)),
      processStart(pid_t(found.pid)) == start else { throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"context-identity"]) }
    guard AXUIElementSetMessagingTimeout(found.root,0.03) == .success else {
      throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"timeout-config"])
    }
    var rawParent:CFTypeRef?
    let parentError=AXUIElementCopyAttributeValue(found.root,kAXParentAttribute as CFString,&rawParent)
    guard parentError == .success,let actualParent=axElement(rawParent),
      attachedServiceParentValid(CFEqual(actualParent,found.parent),owns(actualParent,pid) ? pid : -1,pid,descendant(actualParent,outer)) else {
      throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"retained-ancestry"])
    }
    return (found.root,actualParent,pid_t(found.pid),start)
  }
  func verifyService() throws -> [AXUIElement] {
    try verify()
    let outer=try verifySheet()
    guard let expectedRoot=serviceRoot,let expectedParent=serviceParent,
      let expectedPID=servicePID,let expectedStart=serviceStart,let expectedSheetToken=serviceSheetToken,
      sheetToken == expectedSheetToken else { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"]) }
    let current=try discoverServiceRoot(outer)
    guard CFEqual(current.0,expectedRoot),CFEqual(current.1,expectedParent),current.2 == expectedPID,
      current.3 == expectedStart,authenticatedPanelService(expectedPID),processStart(expectedPID) == expectedStart else {
      throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"])
    }
    let tree=try serviceOwnerTree(expectedRoot,expectedPID)
    try verify(); _ = try verifySheet()
    guard processStart(expectedPID) == expectedStart,authenticatedPanelService(expectedPID) else {
      throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"])
    }
    return tree
  }
  func modal(_ candidate: AXUIElement) -> Bool {
    let role = attr(candidate,kAXRoleAttribute) as? String
    return role == kAXSheetRole || (role == kAXGroupRole && attr(candidate,kAXSubroleAttribute) as? String == kAXDialogSubrole)
  }
  func locationContext(_ focusRoot: AXUIElement? = nil) throws {
    let tree=try verifyService()
    if let focusRoot { try locationCheck(tree.contains { CFEqual($0,focusRoot) },"context-identity") }
  }
  func verifyLocation() throws -> AXUIElement {
    let tree=try verifyService()
    guard let retained = location, let field = locationField,let root=serviceRoot,let ownerPID=servicePID else { throw ActionRefusal(diagnostics:["stage":"goto-state"]) }
    let modals = tree.filter { !CFEqual($0,root) && modal($0) }
    try locationPredicate(modals.count == 1,"retained-modal-count",["gotoCandidateCount":modals.count])
    try locationPredicate(CFEqual(modals[0],retained),"retained-modal-identity")
    let focusedFields=tree.filter { attr($0,kAXRoleAttribute) as? String == kAXTextFieldRole && attr($0,kAXFocusedAttribute) as? Bool == true }
    try locationPredicate(uniqueFocusedField(focusedFields.count,focusedFields.first.map { CFEqual($0,field) } ?? false),"retained-field")
    let modalTree=try serviceOwnerTree(retained,ownerPID)
    try locationPredicate(modalTree.contains { CFEqual($0,field) },"retained-ancestry")
    var subrole: CFTypeRef?, writable = DarwinBoolean(false)
    let query = AXUIElementCopyAttributeValue(field,kAXSubroleAttribute as CFString,&subrole)
    try locationPredicate(attr(field,kAXFocusedAttribute) as? Bool == true && attr(field,kAXRoleAttribute) as? String == kAXTextFieldRole && textFieldKind(query,subrole as? String) == "plain" &&
      attr(field,kAXEnabledAttribute) as? Bool == true && AXUIElementIsAttributeSettable(field,kAXValueAttribute as CFString,&writable) == .success &&
      writable.boolValue && supports(field,kAXConfirmAction),"retained-field")
    return retained
  }
  try emit(["status":"pinned", "pid":pid, "windowId":number, "matchedAX":true])
  while let line = readLine() {
    try check(line.utf8.count <= 8192)
    guard let raw = line.data(using:.utf8), let request = try JSONSerialization.jsonObject(with:raw) as? [String:Any],
          let action = request["type"] as? String, actions.contains(action), request["pid"] as? Int32 == pid,
          request["windowId"] as? UInt32 == number else { throw Refusal(code:"adapter-refused") }
    try locationCheck(scope.reference || !["go-to-folder","location-dismissed"].contains(action) && request["locationToken"] == nil,"goto-state")
    // The driver's mandatory identity checks do not consume the 64 UI-action budget.
    if action != "check" { try check(ordinal < (scope.reference ? 128 : 64)); ordinal += 1 }
    if action == "quit" { try verify(); try emit(["status":"quit", "pid":pid,"windowId":number]); return }
    let requestedPath = request["element"] as? String
    func pressFailure(_ stage: String, _ extra: [String:Any] = [:]) -> ActionRefusal {
      var diagnostics: [String:Any] = ["stage":stage]
      if let requestedPath, requestedPath.range(of:"^\\d+(?:\\.\\d+)*$",options:.regularExpression) != nil,
         requestedPath.utf8.count <= 128 { diagnostics["elementPath"] = requestedPath }
      diagnostics.merge(extra) { _, new in new }
      return ActionRefusal(diagnostics:diagnostics)
    }
    activeInspectionContext = action == "inspect" && (request["target"] as? String ?? "main") == "main" ?
      InspectionContext(stage:"identity-precondition",queryClass:"identity-check-failed") : nil
    do { try verify() }
    catch let error as IdentityRefusal {
      if action == "press" || action == "set" {
        throw pressFailure(error.stage == "process-start" ? "window-identity" : "window-identity-"+error.stage,error.facts)
      }
      throw error
    }
    catch { if action == "press" || action == "set" { throw pressFailure("window-identity") }; throw error }
    if activeInspectionContext != nil { setInspectionContext("request-precondition","request-refused") }
    var result: [String:Any] = ["status":"ok", "pid":pid,"windowId":number,"type":action,"ordinal":ordinal,"matchedAX":true]
    let target = request["target"] as? String ?? "main"
    try check(target == "main" || target == "sheet")
    var selectedRoot = main
    if target == "sheet" {
      try check(request["sheetToken"] as? String == sheetToken && sheetToken != nil)
      selectedRoot = try verifySheet(); result["target"] = "sheet"; result["sheetToken"] = sheetToken
    }
    if let token = locationToken, action != "check" {
      try locationCheck(target == "sheet" && request["locationToken"] as? String == token &&
        ["inspect","set","press","location-dismissed"].contains(action),"goto-state")
      if action != "location-dismissed" { selectedRoot = try verifyLocation() }
      result["locationToken"] = token
    } else { try locationCheck(request["locationToken"] == nil,"goto-state") }
    try requireResolvedSheetMutation(target,action,serviceRoot != nil,locationToken != nil)
    switch action {
    case "go-to-folder":
      let trace = LocationTrace()
      try trace.observe {
        try locationCheck(scope.reference && target == "sheet" && !locationAttempted && location == nil,"goto-state")
        try locationCheck(Set(request.keys).isSubset(of:["type","target","pid","windowId","sheetToken"]) &&
          AXUIElementSetMessagingTimeout(application,0.03) == .success && AXUIElementSetMessagingTimeout(main,0.03) == .success,"goto-state")
        trace.phase = "pre-context"
        let attached = try discoverServiceRoot(selectedRoot)
        serviceRoot=attached.0;serviceParent=attached.1;servicePID=attached.2
        serviceStart=attached.3;serviceSheetToken=sheetToken
        let before = try verifyService()
        let labels:Set<String>=["Go to Folder…","Go to Folder...","Go to Folder","Go to the Folder…","Go to the folder…","Go to the folder:"]
        let openers=before.filter { item in
          let role=attr(item,kAXRoleAttribute) as? String
          let title=attr(item,kAXTitleAttribute) as? String ?? ""
          let description=attr(item,kAXDescriptionAttribute) as? String ?? ""
          return [kAXButtonRole,kAXMenuItemRole].contains(role ?? "") && labels.contains(title.isEmpty ? description:title) &&
            attr(item,kAXEnabledAttribute) as? Bool == true && supports(item,kAXPressAction)
        }
        try locationPredicate(openers.count == 1,"candidate-count",["gotoCandidateCount":openers.count])
        let opener=openers[0]
        try goToFolder(scope.reference,attempted:locationAttempted,trace:trace,consume:{ locationAttempted = true },context:{
          try locationContext()
        },perform:{
          try guardedElementAction({
            let current=try verifyService()
            let role=attr(opener,kAXRoleAttribute) as? String
            let title=attr(opener,kAXTitleAttribute) as? String ?? ""
            let description=attr(opener,kAXDescriptionAttribute) as? String ?? ""
            return current.contains { CFEqual($0,opener) } && [kAXButtonRole,kAXMenuItemRole].contains(role ?? "") &&
              labels.contains(title.isEmpty ? description:title) && attr(opener,kAXEnabledAttribute) as? Bool == true && supports(opener,kAXPressAction)
          },{
            guard AXUIElementPerformAction(opener,kAXPressAction as CFString) == .success else {
              throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"candidate-proof"])
            }
          })
        },discover:{
          let deadline = DispatchTime.now().uptimeNanoseconds + 2_000_000_000
          while DispatchTime.now().uptimeNanoseconds < deadline {
            let tree=try verifyService()
            guard let root=serviceRoot,let ownerPID=servicePID else { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"]) }
            let candidates = tree.filter { !CFEqual($0,root) && modal($0) }
            if candidates.isEmpty { Thread.sleep(forTimeInterval:0.05); continue }
            try locationPredicate(candidates.count == 1,"candidate-count",["gotoCandidateCount":candidates.count])
            let candidate=candidates[0],contents=try serviceOwnerTree(candidate,ownerPID)
            let fields = contents.filter { attr($0,kAXRoleAttribute) as? String == kAXTextFieldRole && attr($0,kAXFocusedAttribute) as? Bool == true }
            let allFocused=tree.filter { attr($0,kAXRoleAttribute) as? String == kAXTextFieldRole && attr($0,kAXFocusedAttribute) as? Bool == true }
            let focusedMatches=allFocused.count == 1 && fields.count == 1 && CFEqual(allFocused[0],fields[0])
            try locationPredicate(uniqueFocusedField(allFocused.count,focusedMatches),"field-count",["gotoFieldCount":allFocused.count])
            let labels: Set<String> = ["Go to the folder:","Go to folder:","Go to Folder","Go to the folder","Go to Folder…"]
            let semantic = contents.contains { item in
              [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute].contains { labels.contains(attr(item,$0) as? String ?? "") }
            }
            var subrole: CFTypeRef?, writable = DarwinBoolean(false)
            try locationPredicate(fields.count == 1,"field-count",["gotoFieldCount":fields.count])
            let field = fields[0], query = AXUIElementCopyAttributeValue(field,kAXSubroleAttribute as CFString,&subrole)
            let writeQuery = AXUIElementIsAttributeSettable(field,kAXValueAttribute as CFString,&writable)
            let fresh = !before.contains { CFEqual($0,candidate) || CFEqual($0,field) }, owned = tree.contains { CFEqual($0,candidate) }
            let kind = textFieldKind(query,subrole as? String), enabled = attr(field,kAXEnabledAttribute) as? Bool == true
            let settable = writeQuery == .success && writable.boolValue, confirm = supports(field,kAXConfirmAction)
            try locationPredicate(locationCandidateValid(fresh,owned,modal(candidate),semantic,fields.count,kind,enabled,settable,confirm),
              "candidate-proof",["candidateFresh":fresh,"candidateOwned":owned,"semanticMatched":semantic,
              "gotoFieldCount":fields.count,"textFieldKind":kind,"enabled":enabled,"valueSettable":settable,"confirmAdvertised":confirm,
              "subroleQueryError":Int(query.rawValue),"valueSettableQueryError":Int(writeQuery.rawValue),"focused":attr(field,kAXFocusedAttribute) as? Bool == true])
            location = candidate; locationField = field; locationToken = UUID().uuidString.lowercased()
            trace.phase = "retained-verification"
            _ = try verifyLocation(); result["locationToken"] = locationToken; result["locationReady"] = true; result["commandPosted"] = trace.actionPerformed
            trace.phase = "complete"; result["actionDiagnostics"] = trace.facts(["stage":"goto-ready"])
            return
          }
          throw ActionRefusal(diagnostics:["stage":"goto-deadline","gotoReason":"candidate-deadline","gotoCandidateCount":0])
        })
      }
    case "location-dismissed":
      guard let retained=location,let root=serviceRoot else { throw ActionRefusal(diagnostics:["stage":"goto-state"]) }
      let tree=try verifyService()
      try locationCheck(!tree.contains { CFEqual($0,retained) || !CFEqual($0,root) && modal($0) },"goto-dismissal")
      location = nil; locationField = nil; locationToken = nil; serviceRoot=nil;serviceParent=nil;servicePID=nil
      serviceStart=nil;serviceSheetToken=nil;result["locationDismissed"] = true
    case "pin-sheet":
      try check(target == "main")
      let actual = try members(); try check(actual.count == 1 && owns(actual[0],pid))
      if let retained = sheet { try check(CFEqual(actual[0],retained)); _ = try verifySheet() }
      else { sheet = actual[0]; sheetToken = UUID().uuidString.lowercased(); locationAttempted = false }
      result["target"] = "sheet"; result["sheetToken"] = sheetToken
    case "sheet-dismissed":
      try check(target == "main" && sheet != nil && sheetToken != nil && request["sheetToken"] as? String == sheetToken)
      try check(try members().isEmpty); sheet = nil; sheetToken = nil; result["dismissed"] = true
    case "raise":
      try check(target == "main"); try noSheet()
      try check(AXUIElementPerformAction(main,kAXRaiseAction as CFString) == .success)
    case "press", "set", "select":
      guard let value = requestedPath, !value.isEmpty else {
        if action == "press" || action == "set" { throw pressFailure("route-resolution",["routeRefusal":"adapter-refused"]) }
        throw Refusal(code:"adapter-refused")
      }
      if target == "main" {
        do { try noSheet() }
        catch { if action == "press" || action == "set" { throw pressFailure("sheet-state") }; throw error }
      }
      let selected: AXUIElement
      let selectedOwner=location == nil ? pid : (servicePID ?? -1)
      do { selected = try node(value,selectedRoot,owner:selectedOwner) }
      catch {
        if action == "press" || action == "set" {
          let code = (error as? Refusal)?.code ?? "adapter-refused"
          throw pressFailure("route-resolution",["routeRefusal":allowedReasons.contains(code) ? code : "adapter-refused"])
        }
        throw error
      }
      if action == "press" {
        if location != nil { try locationCheck(attr(selected,kAXTitleAttribute) as? String == "Cancel" &&
          attr(selected,kAXRoleAttribute) as? String == kAXButtonRole && attr(selected,kAXEnabledAttribute) as? Bool == true,"goto-state") }
        var actionNames: CFArray?
        let query = AXUIElementCopyActionNames(selected,&actionNames)
        let advertised = query == .success && (actionNames as? [String] ?? []).contains(kAXPressAction as String)
        var roles: Set<String> = [kAXButtonRole,kAXCheckBoxRole,kAXMenuItemRole,kAXRowRole,kAXTextFieldRole]
        if scope.reference { roles.insert(kAXPopUpButtonRole) }
        let rawRole = attr(selected,kAXRoleAttribute) as? String ?? ""
        var facts: [String:Any] = ["axPressQueryError":Int(query.rawValue),"axPressAdvertised":advertised,
          "role":roles.contains(rawRole) ? rawRole : "other"]
        let title = attr(selected,kAXTitleAttribute) as? String ?? "", value = attr(selected,kAXValueAttribute) as? String ?? ""
        if let control = ownedControl(scope.reference,title,value,{ attr(selected,kAXDescriptionAttribute) as? String }) { facts["control"] = control }
        guard query == .success && advertised else { throw pressFailure("axpress-support",facts) }
        if args[1] == "--png-persistent" || request["expectedControl"] != nil || request["expectedRole"] != nil {
          guard let control = request["expectedControl"] as? String, let role = request["expectedRole"] as? String else { throw Refusal(code:"adapter-refused") }
          try check(rawRole == role && facts["control"] as? String == control)
        }
        if location != nil {
          do {
            guard let ownerPID=servicePID else { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"]) }
            let exactModal=try verifyLocation(), descendants=try serviceOwnerTree(exactModal,ownerPID)
            try locationCheck(descendants.contains { CFEqual($0,selected) },"retained-ancestry")
            try locationCheck(attr(selected,kAXEnabledAttribute) as? Bool == true,"candidate-proof")
          } catch { throw pressFailure("goto-context") }
        }
        let perform = AXUIElementPerformAction(selected,kAXPressAction as CFString)
        facts["performActionError"] = Int(perform.rawValue)
        guard perform == .success else { throw pressFailure("axpress-perform",facts) }
      }
      if action == "set" {
        guard target == "sheet", let text = request["value"] as? String, !text.isEmpty,
          text.utf8.count <= 4096 else { throw pressFailure("axtext-request") }
        func pinnedField() throws {
          try verify(); let retained = try verifySheet()
          let root = location == nil ? retained : try verifyLocation()
          try check(owns(selected,selectedOwner) && CFEqual(try node(value,root,owner:selectedOwner),selected))
          if let field = locationField { try check(CFEqual(selected,field)) }
        }
        do { try pinnedField() } catch { throw pressFailure("axtext-identity") }
        var subrole: CFTypeRef?, focused: CFTypeRef?, actionNames: CFArray?
        let subroleQuery = AXUIElementCopyAttributeValue(selected,kAXSubroleAttribute as CFString,&subrole)
        var focusable = DarwinBoolean(false), writable = DarwinBoolean(false)
        let focusQuery = AXUIElementIsAttributeSettable(selected,kAXFocusedAttribute as CFString,&focusable)
        let focusedQuery = AXUIElementCopyAttributeValue(selected,kAXFocusedAttribute as CFString,&focused)
        let valueQuery = AXUIElementIsAttributeSettable(selected,kAXValueAttribute as CFString,&writable)
        let confirmQuery = AXUIElementCopyActionNames(selected,&actionNames)
        let facts: [String:Any] = ["role":attr(selected,kAXRoleAttribute) as? String == kAXTextFieldRole ? "AXTextField" : "other",
          "enabled":attr(selected,kAXEnabledAttribute) as? Bool == true,
          "textFieldKind":textFieldKind(subroleQuery,subrole as? String),"subroleQueryError":Int(subroleQuery.rawValue),
          "focusSettableQueryError":Int(focusQuery.rawValue),"focusSettable":focusQuery == .success && focusable.boolValue,
          "focusedQueryError":Int(focusedQuery.rawValue),"focusedMatches":focusedQuery == .success && (focused as? Bool) == true,
          "valueSettableQueryError":Int(valueQuery.rawValue),"valueSettable":valueQuery == .success && writable.boolValue,
          "confirmQueryError":Int(confirmQuery.rawValue),"confirmAdvertised":confirmQuery == .success && (actionNames as? [String] ?? []).contains(kAXConfirmAction as String)]
        do {
          var evidence = try enterSheetText(facts,identity:pinnedField,
            write:{ Int(AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,text as CFString).rawValue) },
            read:{ var readback: CFTypeRef?; let error = AXUIElementCopyAttributeValue(selected,kAXValueAttribute as CFString,&readback)
              return (Int(error.rawValue),error == .success && (readback as? String) == text) },
            confirm:{ Int(AXUIElementPerformAction(selected,kAXConfirmAction as CFString).rawValue) })
          evidence["elementPath"] = value; result["actionDiagnostics"] = evidence
        } catch let error as ActionRefusal { throw pressFailure(error.diagnostics["stage"] as? String ?? "axtext-request",error.diagnostics) }
      }
      if action == "select" {
        try check(target == "sheet" && attr(selected,kAXRoleAttribute) as? String == kAXRowRole)
        func axError(_ error: AXError) -> Int { Int(error.rawValue) }
        var d: [String:Any] = ["outlineFound":false,"outlineSelectedRowsReadable":false,
          "outlineSelectedRowMember":false,"outlineSelectedRowsOwned":false,
          "outlineRowsReadable":false,"outlineRowMember":false,"outlineRowsOwned":false]
        var rowValue: CFTypeRef?
        let rowQuery = AXUIElementCopyAttributeValue(selected,kAXSelectedAttribute as CFString,&rowValue)
        d["rowQueryError"] = axError(rowQuery); if rowQuery == .success, let b = rowValue as? Bool { d["rowSelected"] = b }
        var rowSettable = DarwinBoolean(false)
        let rowCapability = AXUIElementIsAttributeSettable(selected,kAXSelectedAttribute as CFString,&rowSettable)
        d["rowSettableQueryError"] = axError(rowCapability); d["rowSettable"] = rowCapability == .success && rowSettable.boolValue
        // Query the nearest same-process owning AXOutline, but never write AXSelectedRows.
        var parent = axElement(attr(selected,kAXParentAttribute))
        for _ in 0..<20 {
          guard let current = parent, owns(current,pid) else { break }
          if attr(current,kAXRoleAttribute) as? String == "AXOutline" {
            d["outlineFound"] = true
            var selectedRowsValue: CFTypeRef?
            let selectedRowsError = AXUIElementCopyAttributeValue(current,"AXSelectedRows" as CFString,&selectedRowsValue)
            d["outlineSelectedRowsQueryError"] = axError(selectedRowsError)
            var outlineSettable = DarwinBoolean(false)
            let capability = AXUIElementIsAttributeSettable(current,"AXSelectedRows" as CFString,&outlineSettable)
            d["outlineSelectedRowsSettableQueryError"] = axError(capability)
            d["outlineSelectedRowsSettable"] = capability == .success && outlineSettable.boolValue
            if selectedRowsError == .success, let rows = selectedRowsValue as? [AXUIElement], rows.count <= 128 {
              d["outlineSelectedRowsReadable"] = true
              let owned = rows.allSatisfy { owns($0,pid) }
              d["outlineSelectedRowsOwned"] = owned
              d["outlineSelectedRowMember"] = owned && rows.contains { CFEqual($0,selected) }
            }
            var outlineRowsValue: CFTypeRef?
            let outlineRowsError = AXUIElementCopyAttributeValue(current,kAXRowsAttribute as CFString,&outlineRowsValue)
            d["outlineRowsQueryError"] = axError(outlineRowsError)
            if outlineRowsError == .success, let rows = outlineRowsValue as? [AXUIElement], rows.count <= 128 {
              d["outlineRowsReadable"] = true
              let owned = rows.allSatisfy { owns($0,pid) }
              d["outlineRowsOwned"] = owned
              d["outlineRowMember"] = owned && rows.contains { CFEqual($0,selected) }
            }
            break
          }
          parent = axElement(attr(current,kAXParentAttribute))
        }
        guard rowQuery == .success, rowCapability == .success, rowSettable.boolValue else {
          throw SelectionRefusal(diagnostics:d.merging(["stage":"row-capability"]){$1})
        }
        let setError = AXUIElementSetAttributeValue(selected,kAXSelectedAttribute as CFString,kCFBooleanTrue)
        d["rowSetError"] = axError(setError)
        guard setError == .success else { throw SelectionRefusal(diagnostics:d.merging(["stage":"row-write"]){$1}) }
        var readback: CFTypeRef?
        let readError = AXUIElementCopyAttributeValue(selected,kAXSelectedAttribute as CFString,&readback)
        d["rowReadbackError"] = axError(readError); if readError == .success, let b = readback as? Bool { d["rowSelectedAfter"] = b }
        guard readError == .success, (readback as? Bool) == true else {
          throw SelectionRefusal(diagnostics:d.merging(["stage":"row-readback"]){$1})
        }
        d["stage"] = "row-readback"; result["selectionDiagnostics"] = d
        result["selected"] = true
      }
      do { try verify() }
      catch let error as IdentityRefusal {
        if action == "press" {
          var facts = error.facts
          if shouldProbeRetainedMainChildren(error.stage,error.facts) {
            let observation = transitionObservationDiagnostic()
            facts["transitionObservation"] = observation
            let status = observation["status"] as? String
            let configured = observation["axTimeoutsConfigured"] as? Bool == true
            if configured && status != "deadline" && status != "timeout-config-refused" {
              facts.merge(retainedMainChildrenDiagnostic()) { _, new in new }
            }
          }
          throw pressFailure("post-action-"+error.stage,facts)
        }
        if action == "set" { throw pressFailure("post-action-"+error.stage,error.facts) }
        throw error
      }
      catch { if action == "press" || action == "set" { throw pressFailure("post-action-identity") }; throw error }
      if target == "sheet" {
        if try members().isEmpty { result["dismissalPendingVerification"] = true }
        else { _ = try verifySheet() }
      }
    case "inspect":
      if target == "main" {
        setInspectionContext("main-sheet-state","retained-sheet-present")
        try noSheet(inspectMain:true)
      }
      let base = request["element"] as? String ?? ""
      setInspectionContext("root-resolution","route-refused")
      let inspectOwner = locationToken == nil ? pid : (servicePID ?? 0); if locationToken != nil { _ = try verifyLocation() }
      let selected = locationToken == nil ? try node(base,selectedRoot) : try node(base,selectedRoot,owner:inspectOwner)
      var roles: Set<String> = ["AXWindow","AXSheet","AXButton","AXTextField","AXStaticText","AXGroup","AXWebArea","AXCheckBox","AXMenu","AXMenuItem","AXRow","AXOutline","AXScrollArea"]
      if scope.reference { roles.insert(kAXPopUpButtonRole) }
      func fact(_ current: AXUIElement, _ path: String) -> [String:Any] {
        // No title/value/children reads from a foreign child; expose blocked route only.
        var owner: pid_t = 0
        let resolved: pid_t? = AXUIElementGetPid(current,&owner) == .success ? owner : nil
        return localFacts(path,resolved,inspectOwner) {
        let role = attr(current,kAXRoleAttribute) as? String ?? ""
        let title = attr(current,kAXTitleAttribute) as? String ?? "", value = attr(current,kAXValueAttribute) as? String ?? ""
        var facts: [String:Any] = ["path":path,"role":roles.contains(role) ? role : "other","press":supports(current,kAXPressAction),"confirm":supports(current,kAXConfirmAction)]
        if let selected = attr(current,kAXSelectedAttribute) as? Bool { facts["selected"] = selected }
        if let enabled = attr(current,kAXEnabledAttribute) as? Bool { facts["enabled"] = enabled }
        if role == kAXTextFieldRole { var subrole: CFTypeRef?
          let query = AXUIElementCopyAttributeValue(current,kAXSubroleAttribute as CFString,&subrole)
          facts["textFieldKind"] = textFieldKind(query,subrole as? String)
        }
        if let control = ownedControl(scope.reference,title,value,{ attr(current,kAXDescriptionAttribute) as? String },inspection:true) { facts["control"] = control }
        let alerts = ["Switching project tabs is unavailable in this build.":"switch-unavailable","Closing the active project tab is unavailable in this build.":"active-close-unavailable"]
        if let alert = alerts[value] ?? alerts[title] { facts["alert"] = alert }
        return facts
        }
      }
      let depth = request["depth"] as? Int ?? 1, maxNodes = request["maxNodes"] as? Int ?? 128
      setInspectionContext("projection-budget","budget-refused")
      try projectionLimit(depth,maxNodes)
      if request["childOffset"] != nil { try check(target == "main" && locationToken == nil) }
      let projection = try ownedProjection(selected,base:base,depth:depth,maxNodes:maxNodes,
        expectedOwner:Int(inspectOwner),request:request,cursor:&projectionCursor,equal:{ CFEqual($0,$1) },hashKey:{ CFHash($0) },
        owner:{ value in var actual:pid_t = 0; return AXUIElementGetPid(value,&actual) == .success ? Int(actual) : nil },
        children:{ ownedAXChildren($0,expectedOwner:Int(inspectOwner)) },
        isWindow:{ attr($0,kAXRoleAttribute) as? String == kAXWindowRole },fact:fact,phase:setInspectionContext)
      result.merge(projection) { _,new in new }
      if request["projectTabs"] != nil {
        try check(!scope.reference && target == "main" && locationToken == nil)
        result["projectTabs"] = try retainedProjectTabs(selected,path:base,owner:Int(inspectOwner),control:fact(selected,base)["control"] as? String,
          request:request,projection:projection,cursor:&projectionCursor,equal:{ CFEqual($0,$1) })
      }
      try verify()
      if locationToken != nil { _ = try verifyLocation() } else if target == "sheet" { _ = try verifySheet() }
      activeInspectionContext = nil
    case "capture":
      try check(target == "main")
      if sheet != nil { _ = try verifySheet() } else { try noSheet() }
      guard let name = request["capture"] as? String, name.range(of:"^capture-[1-9][0-9]{0,2}\\.png$",options:.regularExpression) != nil else { throw Refusal(code:"adapter-refused") }
      let now = try FileManager.default.attributesOfItem(atPath:physical.path)
      try check(now[.systemNumber] as? NSNumber == reportIdentity[.systemNumber] as? NSNumber && now[.systemFileNumber] as? NSNumber == reportIdentity[.systemFileNumber] as? NSNumber)
      let target = physical.appendingPathComponent(name)
      try check(!FileManager.default.fileExists(atPath:target.path))
      let process = Process(); process.executableURL = URL(fileURLWithPath:"/usr/sbin/screencapture")
      process.arguments = ["-x","-o","-l"+String(number),target.path]
      try process.run(); process.waitUntilExit()
      do { try check(process.terminationStatus == 0); try verify() }
      catch { try? FileManager.default.removeItem(at:target); throw error }
      result["capture"] = name
    default: break
    }
    try emit(result)
  }
}
@main struct P38OwnedWindow {
  static func main() {
    do { try run() }
    catch let error as OwnedDiscoveryRefusal { try? emit(["status":"refused","code":"adapter-refused","discoveryDiagnostics":error.diagnostics]); exit(1) }
    catch let error as SelectionRefusal { try? emit(["status":"refused","code":"adapter-refused","selectionDiagnostics":error.diagnostics]); exit(1) }
    catch let error as ActionRefusal { try? emit(["status":"refused","code":"adapter-refused","actionDiagnostics":error.diagnostics]); exit(1) }
    catch let error as InspectionRefusal {
      var result: [String:Any] = ["status":"refused","code":"adapter-refused"]
      if let diagnostic = safeInspectionContext(error.stage,error.queryClass) { result["inspectionDiagnostics"] = diagnostic }
      try? emit(result); exit(1)
    }
    catch let error as Refusal {
      let code = allowedReasons.contains(error.code) ? error.code : "adapter-refused"
      var result: [String:Any] = ["status":"refused","code":code]
      if let context = activeInspectionContext,
         let diagnostic = inspectionDiagnosticForRefusal(code,context.stage,context.queryClass) { result["inspectionDiagnostics"] = diagnostic }
      try? emit(result); exit(1)
    }
    catch {
      var result: [String:Any] = ["status":"refused","code":"adapter-refused"]
      if let context = activeInspectionContext, let diagnostic = safeInspectionContext(context.stage,context.queryClass) { result["inspectionDiagnostics"] = diagnostic }
      try? emit(result); exit(1)
    }
  }
}
