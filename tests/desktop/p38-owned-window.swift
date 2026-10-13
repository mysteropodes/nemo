// P38V2 test-only persistent main/sheet control; no global input or rediscovery.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

struct Refusal: Error { let code: String }
struct SelectionRefusal: Error { let diagnostics: [String:Any] }
struct ActionRefusal: Error { let diagnostics: [String:Any] }
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
let actions: Set<String> = ["inspect", "press", "set", "select", "raise", "capture", "check", "pin-sheet", "sheet-dismissed", "quit"]
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
    "retainedMainProbeCases":15,"transitionObservationCases":6,"workerDeadlineCases":3,"textEntryCases":13,"textFieldKindCases":4,"foreignReads":0, "runtimeAX":false])
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
  try emit(["status":"pinned", "pid":pid, "windowId":number, "matchedAX":true])
  while let line = readLine() {
    try check(line.utf8.count <= 8192)
    guard let raw = line.data(using:.utf8), let request = try JSONSerialization.jsonObject(with:raw) as? [String:Any],
          let action = request["type"] as? String, actions.contains(action), request["pid"] as? Int32 == pid,
          request["windowId"] as? UInt32 == number else { throw Refusal(code:"adapter-refused") }
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
    switch action {
    case "pin-sheet":
      try check(target == "main")
      let actual = try members(); try check(actual.count == 1 && owns(actual[0],pid))
      if let retained = sheet { try check(CFEqual(actual[0],retained)); _ = try verifySheet() }
      else { sheet = actual[0]; sheetToken = UUID().uuidString.lowercased() }
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
          try check(owns(selected,pid) && CFEqual(try node(value,retained),selected))
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
        "SVG Sequence","Work area","Whole timeline","1x","Choose","Select","Export Image","Replace"]) }
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
