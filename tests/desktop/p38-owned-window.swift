// P38V2 test-only persistent main/sheet control; no global input or rediscovery.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin
import Carbon

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
// This records calls to postToPid, never key delivery or native UI acceptance.
final class LocationTrace {
  var phase = "request"
  private(set) var pairPosted = false
  func postedPair() { pairPosted = true; phase = "posted-pair" }
  func facts(_ original: [String:Any]) -> [String:Any] {
    var result = original
    result["gotoPhase"] = phase; result["commandPosted"] = pairPosted
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
  equal: (Node,Node)->Bool, owner: (Node)->(Int,Bool), isWindow: (Node)->Bool,
  timeout: (Node)->Int, children: (Node)->(Int,[Node]?), parent: (Node)->(Int,Node?)) throws -> [Node] {
  var found = [Node]()
  func walk(_ current: Node, _ route: String, _ depth: Int) throws {
    let facts: [String:Any] = ["gotoRoute":route,"gotoDepth":depth,"gotoNodeCount":found.count]
    func require(_ ok: Bool, _ reason: String, _ extra: [String:Any] = [:]) throws {
      var observed = facts; observed["gotoNodeCount"] = found.count
      try locationPredicate(ok,reason,observed.merging(extra) { _,new in new })
    }
    func owned(_ node: Node, _ kind: String) throws {
      let (error,matches) = owner(node)
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
      let (childError,childOwned) = owner(child)
      try edgeRequire(childError == 0,"child-owner-query",["gotoQueryError":childError,"ownerMatches":false])
      try edgeRequire(childOwned,"child-owner-mismatch",["gotoQueryError":childError,"ownerMatches":childOwned])
      let (parentError,ancestor) = parent(child)
      try edgeRequire(parentError == 0,"parent-query",["gotoQueryError":parentError])
      try edgeRequire(ancestor != nil,"parent-shape",["gotoQueryError":parentError])
      let (ownerError,parentOwned) = owner(ancestor!)
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
// Production and fault controls share the one-shot event ordering. A posted
// process event is not proof that the original sheet received the command.
func goToFolder(_ reference: Bool, attempted: Bool, trace: LocationTrace, consume: () -> Void, context: () throws -> Void,
  down: () -> Void, up: () -> Void, discover: () throws -> Void) throws {
  try locationCheck(reference && !attempted,"goto-state"); consume()
  trace.phase = "pre-context"
  try context()
  // No fallible AX query or waiting between the exact-PID down/up pair.
  down(); up(); trace.postedPair()
  trace.phase = "post-context"
  try context()
  trace.phase = "discovery"
  try discover()
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
    d["enabled"] as? Bool == true, "axtext-role")
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
let allowedReasons: Set<String> = ["adapter-refused","sheet-owner","sheet-parent","sheet-replaced","child-owner","confirm-unsupported","children-unavailable","sheet-ambiguous","sheet-active","standalone-window"]
func check(_ ok: Bool) throws { if !ok { throw Refusal(code:"adapter-refused") } }
let actions: Set<String> = ["inspect", "press", "set", "select", "raise", "capture", "check", "pin-sheet", "sheet-dismissed", "go-to-folder", "location-dismissed", "quit"]
func operatorScope(_ mode: String, _ environment: [String:String]) throws -> (reference:Bool, report:String) {
  try check(mode == "--persistent" || mode == "--reference-persistent")
  let reference = mode == "--reference-persistent"
  let prefix = reference ? "NEMO_REFERENCE_" : "NEMO_P38_"
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
func localFacts(_ path: String, _ owner: pid_t?, _ pid: pid_t, _ read: () -> [String:Any]) -> [String:Any] {
  guard owner == pid else { return ["path":path,"role":"other","blocked":"child-owner"] }
  return read()
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
  for fault in ["none","p38","repeat","pid","window","focus","ancestor","layout","after-down","duplicate","search","absent","timeout"] {
    var attempted = fault == "repeat", events = [String](), contexts = 0, refused = false
    let trace = LocationTrace()
    do {
      try trace.observe { try goToFolder(fault != "p38",attempted:attempted,trace:trace,consume:{ attempted = true },context:{
        contexts += 1; events.append("context")
        try locationCheck(!["pid","window","focus","ancestor","layout"].contains(fault) && !(fault == "after-down" && contexts == 2),"goto-context")
      },down:{ events.append("down") },up:{ events.append("up") },discover:{
        events.append("discover"); try locationCheck(!["duplicate","search","absent","timeout"].contains(fault),"goto-discovery")
      }) }
    } catch let error as ActionRefusal {
      refused = true
      try check(error.diagnostics["commandPosted"] as? Bool == events.contains("down"))
      if fault == "after-down" { try check(error.diagnostics["gotoPhase"] as? String == "post-context") }
    }
    try check(refused == (fault != "none"))
    try check(trace.pairPosted == events.contains("down"))
    if events.contains("down") { try check(events.filter { $0 == "up" }.count == 1 && events.firstIndex(of:"up")! == events.firstIndex(of:"down")!+1) }
    else { try check(!events.contains("up") && !events.contains("discover")) }
    if fault == "after-down" { try check(events == ["context","down","up","context"]) }
    if fault == "none" { try check(events == ["context","down","up","context","discover"]) }
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
          if fault == target+"-owner-query" { return (-25204,false) }
          return (0,fault != target+"-owner-mismatch")
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
      let ownership: (Int)->(Int,Bool) = { (0,$0 == 0) }
      let descendants: (Int)->(Int,[Int]?) = { (0,$0 == 0 ? [1] : []) }
      let ancestor: (Int)->(Int,Int?) = { _ in (0,0) }
      let _: [Int] = try locationWalk(0,deadline:10,now:{ 0 },equal:{ $0 == $1 },owner:ownership,
        isWindow:{ _ in false },timeout:{ _ in 0 },children:descendants,parent:ancestor)
    }
    do { try trace.observe {
      trace.phase = "pre-tree"
      if !posted { try failingWalk() }
      try goToFolder(true,attempted:false,trace:trace,consume:{},context:{},down:{ events.append("down") },up:{ events.append("up") },discover:failingWalk)
    } } catch let error as ActionRefusal {
      refused = true
      try check(error.diagnostics["gotoReason"] as? String == "child-owner-mismatch" && error.diagnostics["gotoRoute"] as? String == "0")
      try check(error.diagnostics["commandPosted"] as? Bool == posted && error.diagnostics["gotoPhase"] as? String == (posted ? "discovery" : "pre-tree"))
    }
    try check(refused && events == (posted ? ["down","up"] : []))
  }
  try check(locationCandidateValid(true,true,true,true,1,"plain",true,true,true))
  for index in 0..<9 {
    try check(!locationCandidateValid(index != 0,index != 1,index != 2,index != 3,index == 4 ? 2 : 1,
      index == 5 ? "search" : "plain",index != 6,index != 7,index != 8))
  }
  try check(textFieldKind(.success,kAXSearchFieldSubrole) == "search")
  try check(textFieldKind(.attributeUnsupported,nil) == "plain")
  try check(textFieldKind(.cannotComplete,nil) == "unreadable")
  try check(textFieldKind(.success,"AXSecureTextField") == "other")
  for fault in ["none","search","disabled","confirm-support","value-settable","write","read","mismatch","confirm",
    "identity-1","identity-2","identity-3","identity-4"] {
    var events = [String](), pins = 0
    let facts: [String:Any] = ["role":"AXTextField","textFieldKind":fault == "search" ? "search" : "plain",
      "enabled":fault != "disabled","confirmQueryError":0,"confirmAdvertised":fault != "confirm-support",
      "valueSettableQueryError":0,"valueSettable":fault != "value-settable",
      "focusSettableQueryError":-25205,"focusSettable":false,"focusedQueryError":0,"focusedMatches":false]
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
        ["search":"axtext-role","disabled":"axtext-role","confirm-support":"axtext-confirm-support",
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
  try emit(["status":"static-model-pass", "routeNegatives":8, "sheetNegatives":7, "identityPredicateNegatives":9,
    "retainedMainProbeCases":15,"transitionObservationCases":6,"workerDeadlineCases":3,"textEntryCases":13,"textFieldKindCases":4,
    "goToFolderSequenceCases":13,"locationWalkCases":walkCases.count,"locationPhaseWalkCases":2,"locationCandidateCases":10,"locationAncestryCases":8,"locationContextCases":5,"foreignReads":0, "runtimeAX":false])
}
func run() throws {
  let args = CommandLine.arguments
  if args == [args[0], "--self-test"] { try selfTest(); return }
  if args.count == 4 && args[1] == "--p38-cg-sample" { try cgWindowSampleWorker(args); return }
  try check(args.count == 3)
  let scope = try operatorScope(args[1],ProcessInfo.processInfo.environment)
  let report = scope.report
  guard let pid = Int32(args[2]), pid > 0 else { throw Refusal(code:"adapter-refused") }
  let initialRows = (CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]] ?? []).filter {
    ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
  }
  try check(initialRows.count == 1)
  guard let number = initialRows[0][kCGWindowNumber as String] as? UInt32 else { throw Refusal(code:"adapter-refused") }
  let reportURL = URL(fileURLWithPath: report).standardizedFileURL
  let physical = reportURL.resolvingSymlinksInPath()
  try check(physical.path == reportURL.path && FileManager.default.fileExists(atPath: physical.path))
  let reportIdentity = try FileManager.default.attributesOfItem(atPath: physical.path)
  try check(reportIdentity[.type] as? FileAttributeType == .typeDirectory)
  func cg(_ all: Bool) -> [[String:Any]]? {
    let flags: CGWindowListOption = all ? [.optionAll,.excludeDesktopElements] : [.optionOnScreenOnly,.excludeDesktopElements]
    guard let rows = CGWindowListCopyWindowInfo(flags,kCGNullWindowID) as? [[String:Any]] else { return nil }
    return rows.filter {
      ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
    }
  }
  guard let initial = cg(false) else { throw Refusal(code:"adapter-refused") }
  try check(initial.count == 1)
  guard let row = initial.first, row[kCGWindowNumber as String] as? UInt32 == number,
        let bounds = row[kCGWindowBounds as String] as? [String:CGFloat] else { throw Refusal(code:"adapter-refused") }
  try check(AXIsProcessTrusted())
  let application = AXUIElementCreateApplication(pid)
  let windows = attr(application,kAXWindowsAttribute) as? [AXUIElement] ?? []
  let matched = windows.filter {
    guard owns($0,pid), let p = xy($0), let s = wh($0) else { return false }
    return abs(p.x-(bounds["X"] ?? -10000)) < 3 && abs(p.y-(bounds["Y"] ?? -10000)) < 3 &&
      abs(s.width-(bounds["Width"] ?? -10000)) < 3 && abs(s.height-(bounds["Height"] ?? -10000)) < 3
  }
  try check(matched.count == 1); let main = matched[0]
  var ordinal = 0
  var sheet: AXUIElement? = nil, sheetToken: String? = nil
  var location: AXUIElement? = nil, locationField: AXUIElement? = nil, locationToken: String? = nil
  var locationAttempted = false
  func descendant(_ candidate: AXUIElement, _ ancestor: AXUIElement) -> Bool {
    ownedAncestor(candidate,ancestor,equal:{ CFEqual($0,$1) },owned:{ owns($0,pid) },
      isWindow:{ attr($0,kAXRoleAttribute) as? String == kAXWindowRole },
      parent:{ axElement(attr($0,kAXParentAttribute)) },children:{ attr($0,kAXChildrenAttribute) as? [AXUIElement] })
  }
  // Public SDK defines AXSheet + AXParent/AXChildren, not an AXSheets constant.
  // Derive attachment from that documented relationship, never same-PID alone.
  func members() throws -> [AXUIElement] {
    guard let childRows = attr(main,kAXChildrenAttribute) as? [AXUIElement],
          let appRows = attr(application,kAXWindowsAttribute) as? [AXUIElement] else { throw Refusal(code:"adapter-refused") }
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
  func noSheet() throws { try check(sheet == nil && (try members()).isEmpty) }
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
  func verify() throws {
    var actualPID: pid_t = 0
    let ownerError = AXUIElementGetPid(main,&actualPID)
    var rawWindows: CFTypeRef?
    let windowsError = AXUIElementCopyAttributeValue(application,kAXWindowsAttribute as CFString,&rawWindows)
    let windows = windowsError == .success ? rawWindows as? [AXUIElement] : nil
    let pinnedAXMember = windows?.contains { CFEqual($0,main) } ?? false
    var rawMinimized: CFTypeRef?
    let minimizedError = AXUIElementCopyAttributeValue(main,kAXMinimizedAttribute as CFString,&rawMinimized)
    let minimizedValue = minimizedError == .success ? (rawMinimized as? Bool) : nil
    let minimizedReadable = minimizedValue != nil, minimized = minimizedValue ?? false
    let cgRows = cg(true), identities = cgRows?.filter { $0[kCGWindowNumber as String] as? UInt32 == number } ?? []
    let stage = identityFailureStage(Int(ownerError.rawValue),ownerError == .success && actualPID == pid,
      Int(windowsError.rawValue),windows != nil,pinnedAXMember,Int(minimizedError.rawValue),minimizedReadable,minimized,cgRows != nil,identities.count)
    guard let stage else { return }
    // Keep the pinned AX object and original CGWindowID; record facts without rediscovery.
    var facts: [String:Any] = ["axPidQueryError":Int(ownerError.rawValue),"axPidMatches":ownerError == .success && actualPID == pid,
      "applicationWindowsQueryError":Int(windowsError.rawValue),"applicationWindowsReadable":windows != nil,
      "pinnedAXWindowMember":pinnedAXMember,"minimizedQueryError":Int(minimizedError.rawValue),
      "minimizedReadable":minimizedReadable,"minimized":minimized,"pinnedCGWindowListReadable":cgRows != nil]
    if cgRows != nil { facts["pinnedCGWindowCount"] = identities.count }
    throw IdentityRefusal(stage:stage,facts:facts)
  }
  func node(_ value: String, _ root: AXUIElement) throws -> AXUIElement {
    var current = root
    for index in try route(value) {
      let list = children(current); try check(list.indices.contains(index)); current = list[index]
      if !owns(current,pid) { throw Refusal(code:"child-owner") }
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
  func locationTree(_ root: AXUIElement, _ deadline: UInt64 = DispatchTime.now().uptimeNanoseconds + 1_000_000_000) throws -> [AXUIElement] {
    try locationWalk(root,deadline:deadline,now:{ DispatchTime.now().uptimeNanoseconds },equal:{ CFEqual($0,$1) },owner:{ node in
      var actual: pid_t = 0; let error = AXUIElementGetPid(node,&actual)
      return (Int(error.rawValue),error == .success && actual == pid)
    },isWindow:{ attr($0,kAXRoleAttribute) as? String == kAXWindowRole },
      timeout:{ Int(AXUIElementSetMessagingTimeout($0,0.03).rawValue) },children:{ node in
      var raw: CFTypeRef?; let error = AXUIElementCopyAttributeValue(node,kAXChildrenAttribute as CFString,&raw)
      return (Int(error.rawValue),raw as? [AXUIElement])
    },parent:{ node in
      var raw: CFTypeRef?; let error = AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
      return (Int(error.rawValue),axElement(raw))
    })
  }
  func modal(_ candidate: AXUIElement) -> Bool {
    let role = attr(candidate,kAXRoleAttribute) as? String
    return role == kAXSheetRole || (role == kAXGroupRole && attr(candidate,kAXSubroleAttribute) as? String == kAXDialogSubrole)
  }
  func locationContext(_ focusRoot: AXUIElement? = nil) throws {
    do { try verify(); _ = try verifySheet() } catch { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-identity"]) }
    guard let outer = sheet, let rows = attr(application,kAXWindowsAttribute) as? [AXUIElement],
      let focusedWindow = axElement(attr(application,kAXFocusedWindowAttribute)),
      let focused = axElement(attr(application,kAXFocusedUIElementAttribute)) else { throw ActionRefusal(diagnostics:["stage":"goto-context","gotoReason":"context-shape"]) }
    let reason = locationContextFailure(rows.filter { CFEqual($0,main) }.count,
      rows.allSatisfy { owns($0,pid) && (CFEqual($0,main) || descendant($0,outer)) },
      owns(focusedWindow,pid) && (CFEqual(focusedWindow,main) || descendant(focusedWindow,outer)),
      descendant(focused,focusRoot ?? location ?? outer))
    try locationCheck(reason == nil,"goto-context",reason)
  }
  func verifyLocation() throws -> AXUIElement {
    try locationContext()
    guard let retained = location, let field = locationField, let outer = sheet else { throw ActionRefusal(diagnostics:["stage":"goto-state"]) }
    let tree = try locationTree(outer), modals = tree.filter { !CFEqual($0,outer) && modal($0) }
    try locationPredicate(modals.count == 1,"retained-modal-count",["gotoCandidateCount":modals.count])
    try locationPredicate(CFEqual(modals[0],retained),"retained-modal-identity")
    try locationPredicate(descendant(retained,outer) && descendant(field,retained),"retained-ancestry")
    var subrole: CFTypeRef?, writable = DarwinBoolean(false)
    let query = AXUIElementCopyAttributeValue(field,kAXSubroleAttribute as CFString,&subrole)
    try locationPredicate(attr(field,kAXRoleAttribute) as? String == kAXTextFieldRole && textFieldKind(query,subrole as? String) == "plain" &&
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
    do { try verify() }
    catch let error as IdentityRefusal { if action == "press" || action == "set" { throw pressFailure("window-identity-"+error.stage,error.facts) }; throw error }
    catch { if action == "press" || action == "set" { throw pressFailure("window-identity") }; throw error }
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
    switch action {
    case "go-to-folder":
      let trace = LocationTrace()
      try trace.observe {
        try locationCheck(scope.reference && target == "sheet" && !locationAttempted && location == nil,"goto-state")
        try locationCheck(Set(request.keys).isSubset(of:["type","target","pid","windowId","sheetToken"]) &&
          AXUIElementSetMessagingTimeout(application,0.03) == .success && AXUIElementSetMessagingTimeout(main,0.03) == .success,"goto-state")
        trace.phase = "pre-tree"
        let before = try locationTree(selectedRoot)
        let priorModals = before.filter { !CFEqual($0,selectedRoot) && modal($0) }
        try locationPredicate(priorModals.isEmpty,"preexisting-modal",["gotoCandidateCount":priorModals.count])
        try locationCheck(before.contains { attr($0,kAXRoleAttribute) as? String == kAXButtonRole &&
          attr($0,kAXTitleAttribute) as? String == "Open" && supports($0,kAXPressAction) },"goto-context","outer-open-control")
        let source = CGEventSource(stateID:.privateState)
        guard let down = CGEvent(keyboardEventSource:source,virtualKey:5,keyDown:true),
          let up = CGEvent(keyboardEventSource:source,virtualKey:5,keyDown:false) else { throw ActionRefusal(diagnostics:["stage":"goto-state","gotoReason":"event-allocation"]) }
        down.flags = [.maskCommand,.maskShift]; up.flags = [.maskCommand,.maskShift]
        try goToFolder(scope.reference,attempted:locationAttempted,trace:trace,consume:{ locationAttempted = true },context:{
          try locationContext()
          // The fixed physical G key is admitted only for these known layouts.
          guard let input = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue(),
            let rawID = TISGetInputSourceProperty(input,kTISPropertyInputSourceID) else { throw ActionRefusal(diagnostics:["stage":"goto-layout","gotoReason":"layout-query"]) }
          let inputID = Unmanaged<CFString>.fromOpaque(rawID).takeUnretainedValue() as String
          try locationCheck(["com.apple.keylayout.US","com.apple.keylayout.ABC"].contains(inputID),"goto-layout","layout-unsupported")
        },down:{ down.postToPid(pid) },up:{ up.postToPid(pid) },discover:{
          let deadline = DispatchTime.now().uptimeNanoseconds + 2_000_000_000
          while DispatchTime.now().uptimeNanoseconds < deadline {
            try locationContext()
            let tree = try locationTree(selectedRoot,deadline)
            let candidates = tree.filter { !CFEqual($0,selectedRoot) && modal($0) }
            if candidates.isEmpty { Thread.sleep(forTimeInterval:0.05); continue }
            try locationPredicate(candidates.count == 1,"candidate-count",["gotoCandidateCount":candidates.count])
            let candidate = candidates[0], contents = try locationTree(candidate,deadline)
            let fields = contents.filter { attr($0,kAXRoleAttribute) as? String == kAXTextFieldRole }
            let labels: Set<String> = ["Go to the folder:","Go to folder:","Go to Folder","Go to the folder"]
            let semantic = contents.contains { item in
              [kAXTitleAttribute,kAXDescriptionAttribute,kAXValueAttribute].contains { labels.contains(attr(item,$0) as? String ?? "") }
            }
            var subrole: CFTypeRef?, writable = DarwinBoolean(false)
            try locationPredicate(fields.count == 1,"field-count",["gotoFieldCount":fields.count])
            let field = fields[0], query = AXUIElementCopyAttributeValue(field,kAXSubroleAttribute as CFString,&subrole)
            let writeQuery = AXUIElementIsAttributeSettable(field,kAXValueAttribute as CFString,&writable)
            let fresh = !before.contains { CFEqual($0,candidate) || CFEqual($0,field) }, owned = descendant(candidate,selectedRoot)
            let kind = textFieldKind(query,subrole as? String), enabled = attr(field,kAXEnabledAttribute) as? Bool == true
            let settable = writeQuery == .success && writable.boolValue, confirm = supports(field,kAXConfirmAction)
            try locationPredicate(locationCandidateValid(fresh,owned,modal(candidate),semantic,fields.count,kind,enabled,settable,confirm),
              "candidate-proof",["candidateFresh":fresh,"candidateOwned":owned,"semanticMatched":semantic,
              "gotoFieldCount":fields.count,"textFieldKind":kind,"enabled":enabled,"valueSettable":settable,"confirmAdvertised":confirm,
              "subroleQueryError":Int(query.rawValue),"valueSettableQueryError":Int(writeQuery.rawValue)])
            location = candidate; locationField = field; locationToken = UUID().uuidString.lowercased()
            trace.phase = "retained-verification"
            _ = try verifyLocation(); result["locationToken"] = locationToken; result["locationReady"] = true; result["commandPosted"] = trace.pairPosted
            trace.phase = "complete"; result["actionDiagnostics"] = trace.facts(["stage":"goto-ready"])
            return
          }
          throw ActionRefusal(diagnostics:["stage":"goto-deadline","gotoReason":"candidate-deadline","gotoCandidateCount":0])
        })
      }
    case "location-dismissed":
      guard let retained = location, let outer = sheet else { throw ActionRefusal(diagnostics:["stage":"goto-state"]) }
      try locationContext(outer)
      let tree = try locationTree(outer)
      try locationCheck(!tree.contains { CFEqual($0,retained) || !CFEqual($0,outer) && modal($0) },"goto-dismissal")
      location = nil; locationField = nil; locationToken = nil; result["locationDismissed"] = true
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
      do { selected = try node(value,selectedRoot) }
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
        let labels = ["Open","Open Project…","Cancel","A","B","A.json","B.json","Motion","Opacity","R08 rectangle"]
        let title = attr(selected,kAXTitleAttribute) as? String ?? "", value = attr(selected,kAXValueAttribute) as? String ?? ""
        if labels.contains(title) { facts["control"] = title }
        else if labels.contains(value) { facts["control"] = value }
        guard query == .success && advertised else { throw pressFailure("axpress-support",facts) }
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
          try check(owns(selected,pid) && CFEqual(try node(value,root),selected))
          if let field = locationField { try check(CFEqual(selected,field)) }
        }
        do { try pinnedField() } catch { throw pressFailure("axtext-identity") }
        var subrole: CFTypeRef?, focused: CFTypeRef?, actionNames: CFArray?
        let subroleQuery = AXUIElementCopyAttributeValue(selected,kAXSubroleAttribute as CFString,&subrole)
        var focusable = DarwinBoolean(false), writable = DarwinBoolean(false)
        let focusQuery = AXUIElementIsAttributeSettable(selected,kAXFocusedAttribute as CFString,&focusable)
        let focusedQuery = AXUIElementCopyAttributeValue(application,kAXFocusedUIElementAttribute as CFString,&focused)
        let valueQuery = AXUIElementIsAttributeSettable(selected,kAXValueAttribute as CFString,&writable)
        let confirmQuery = AXUIElementCopyActionNames(selected,&actionNames)
        let facts: [String:Any] = ["role":attr(selected,kAXRoleAttribute) as? String == kAXTextFieldRole ? "AXTextField" : "other",
          "enabled":attr(selected,kAXEnabledAttribute) as? Bool == true,
          "textFieldKind":textFieldKind(subroleQuery,subrole as? String),"subroleQueryError":Int(subroleQuery.rawValue),
          "focusSettableQueryError":Int(focusQuery.rawValue),"focusSettable":focusQuery == .success && focusable.boolValue,
          "focusedQueryError":Int(focusedQuery.rawValue),"focusedMatches":focusedQuery == .success && focused.map { CFEqual($0,selected) } == true,
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
      if target == "main" { try noSheet() }
      let base = request["element"] as? String ?? ""
      let selected = try node(base,selectedRoot)
      var roles: Set<String> = ["AXWindow","AXSheet","AXButton","AXTextField","AXStaticText","AXGroup","AXWebArea","AXCheckBox","AXMenu","AXMenuItem","AXRow","AXOutline","AXScrollArea"]
      if scope.reference { roles.insert(kAXPopUpButtonRole) }
      var labels: Set<String> = ["Open","Open Project…","Cancel","A","B","A.json","B.json","Motion","Opacity","R08 rectangle"]
      if scope.reference { labels.formUnion(["File","Edit","Save","Save As…","Save As","Save as…","Open…","Undo","Redo",
        "Reference","Reference…","None","No reference","Visible","Remove","Import Reference Image",
        "ReferenceA.png","ReferenceB.png","Saved.json","frame_0001.png","Export","Export…","Export PNG","PNG","PNG Sequence",
        "SVG Sequence","Work area","Whole timeline","1x","Choose","Select","Export Image","Replace",
        "Go","Go to the folder:","Go to folder:","Go to Folder","Go to the folder"]) }
      func fact(_ current: AXUIElement, _ path: String) -> [String:Any] {
        // No title/value/children reads from a foreign child; expose blocked route only.
        var owner: pid_t = 0
        let resolved: pid_t? = AXUIElementGetPid(current,&owner) == .success ? owner : nil
        return localFacts(path,resolved,pid) {
        let role = attr(current,kAXRoleAttribute) as? String ?? ""
        let title = attr(current,kAXTitleAttribute) as? String ?? "", value = attr(current,kAXValueAttribute) as? String ?? ""
        var facts: [String:Any] = ["path":path,"role":roles.contains(role) ? role : "other","press":supports(current,kAXPressAction),"confirm":supports(current,kAXConfirmAction)]
        if let selected = attr(current,kAXSelectedAttribute) as? Bool { facts["selected"] = selected }
        if let enabled = attr(current,kAXEnabledAttribute) as? Bool { facts["enabled"] = enabled }
        if role == kAXTextFieldRole { var subrole: CFTypeRef?
          let query = AXUIElementCopyAttributeValue(current,kAXSubroleAttribute as CFString,&subrole)
          facts["textFieldKind"] = textFieldKind(query,subrole as? String)
        }
        if labels.contains(title) { facts["control"] = title }
        else if labels.contains(value) { facts["control"] = value }
        let alerts = ["Switching project tabs is unavailable in this build.":"switch-unavailable","Closing the active project tab is unavailable in this build.":"active-close-unavailable"]
        if let alert = alerts[value] ?? alerts[title] { facts["alert"] = alert }
        return facts
        }
      }
      let depth = request["depth"] as? Int ?? 1, maxNodes = request["maxNodes"] as? Int ?? 128
      try projectionLimit(depth,maxNodes)
      var nodes = [[String:Any]]()
      var truncated = false
      func project(_ current: AXUIElement, _ path: String, _ level: Int) throws {
        var owner: pid_t = 0
        guard AXUIElementGetPid(current,&owner) == .success, owner == pid else { nodes.append(["path":path,"role":"other","blocked":"child-owner"]); return }
        if level > 0 && attr(current,kAXRoleAttribute) as? String == kAXWindowRole { throw Refusal(code:"standalone-window") }
        nodes.append(fact(current,path))
        guard level < depth else { return }
        let rawChildren = attr(current,kAXChildrenAttribute) as? [AXUIElement]
        guard let list = rawChildren else { nodes[nodes.count-1]["blocked"] = "children-unavailable"; return }
        try check(list.count < 4096)
        for (index,child) in list.enumerated() {
          if nodes.count >= maxNodes { truncated = true; return }
          let childPath = path.isEmpty ? String(index) : path+"."+String(index)
          var childOwner: pid_t = 0
          guard AXUIElementGetPid(child,&childOwner) == .success, childOwner == pid else { nodes.append(["path":childPath,"role":"other","blocked":"child-owner"]); continue }
          try project(child,childPath,level+1)
        }
      }
      try project(selected,base,0)
      result["nodes"] = nodes
      result["truncated"] = truncated
      if target == "sheet" { _ = try verifySheet() }
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
do { try run() }
catch let error as SelectionRefusal { try? emit(["status":"refused","code":"adapter-refused","selectionDiagnostics":error.diagnostics]); exit(1) }
catch let error as ActionRefusal { try? emit(["status":"refused","code":"adapter-refused","actionDiagnostics":error.diagnostics]); exit(1) }
catch let error as Refusal { try? emit(["status":"refused","code":allowedReasons.contains(error.code) ? error.code : "adapter-refused"]); exit(1) }
catch { try? emit(["status":"refused","code":"adapter-refused"]); exit(1) }
