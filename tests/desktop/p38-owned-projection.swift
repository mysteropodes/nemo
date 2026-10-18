// Owned AX projection and retained direct-child pages; no global input or state.
import Foundation
import CoreFoundation
import ApplicationServices
import CoreGraphics
import Darwin

enum OwnedDiscoveryStage: String {
  case arguments, scope, pid, processStart = "process-start", cgFirst = "cg-first"
  case cgID = "cg-window-id", reportPath = "report-path", reportAttributes = "report-attributes"
  case reportKind = "report-kind", cgSecondQuery = "cg-second-query", cgSecondCount = "cg-second-count"
  case cgBounds = "cg-bounds", axTrust = "ax-trust", axMatch = "ax-match"
}
struct OwnedDiscoveryRefusal: Error { let diagnostics: [String:Any] }
struct OwnedDiscoveryTrace {
  var stage: OwnedDiscoveryStage = .arguments
  var facts: [String:Any] = [:]
  mutating func at(_ next: OwnedDiscoveryStage) { stage = next; facts = [:] }
  func refusal() -> OwnedDiscoveryRefusal {
    var value = facts; value["schema"] = "nemo.p38-discovery/1"; value["stage"] = stage.rawValue
    return OwnedDiscoveryRefusal(diagnostics:value)
  }
  mutating func require(_ ok: Bool, _ witness: [String:Any]) throws {
    facts = witness; if !ok { throw refusal() }
  }
}
struct OwnedInitialWindow {
  let scope: (reference:Bool,report:String)
  let pid: pid_t
  let start: ProcessStart
  let number: UInt32
  let physical: URL
  let reportIdentity: [FileAttributeKey:Any]
  let application: AXUIElement
  let main: AXUIElement
  let cg: (Bool)->[[String:Any]]?
}
func discoveryQueryClass(_ error: AXError) -> String {
  switch error {
  case .success: return "success"
  case .attributeUnsupported: return "unsupported"
  case .noValue: return "no-value"
  case .cannotComplete: return "cannot-complete"
  default: return "other-error"
  }
}

// The original one-shot predicate sequence, moved intact from run(). No retries.
func discoverOwnedInitialWindow(_ args: [String], _ environment: [String:String],
  scopeCheck: (String,[String:String]) throws -> (reference:Bool,report:String) = operatorScope) throws -> OwnedInitialWindow {
  var trace = OwnedDiscoveryTrace()
  do {
    try trace.require(args.count == 3,["valid":false])
    trace.at(.scope)
    let scope = try scopeCheck(args[1],environment)
    trace.at(.pid); trace.facts = ["available":false]
    guard let pid = Int32(args[2]), pid > 0 else { throw trace.refusal() }
    trace.at(.processStart); trace.facts = ["available":false]
    guard let pinnedProcessStart = processStart(pid) else { throw trace.refusal() }
    trace.at(.cgFirst)
    let first = CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]]
    let initialRows = (first ?? []).filter {
      ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
    }
    try trace.require(initialRows.count == 1,["available":first != nil,"countBucket":min(initialRows.count,2)])
    trace.at(.cgID); trace.facts = ["available":false]
    guard let number = initialRows[0][kCGWindowNumber as String] as? UInt32 else { throw trace.refusal() }
    let reportURL = URL(fileURLWithPath:scope.report).standardizedFileURL
    let physical = reportURL.resolvingSymlinksInPath()
    trace.at(.reportPath)
    try trace.require(physical.path == reportURL.path && FileManager.default.fileExists(atPath:physical.path),["valid":false])
    trace.at(.reportAttributes)
    let reportIdentity = try FileManager.default.attributesOfItem(atPath:physical.path)
    trace.at(.reportKind)
    try trace.require(reportIdentity[.type] as? FileAttributeType == .typeDirectory,["valid":false])
    func cg(_ all: Bool) -> [[String:Any]]? {
      let flags: CGWindowListOption = all ? [.optionAll,.excludeDesktopElements] : [.optionOnScreenOnly,.excludeDesktopElements]
      guard let rows = CGWindowListCopyWindowInfo(flags,kCGNullWindowID) as? [[String:Any]] else { return nil }
      return rows.filter {
        ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
      }
    }
    trace.at(.cgSecondQuery); trace.facts = ["available":false]
    guard let initial = cg(false) else { throw trace.refusal() }
    trace.at(.cgSecondCount)
    try trace.require(initial.count == 1,["countBucket":min(initial.count,2)])
    trace.at(.cgBounds); trace.facts = ["valid":false]
    guard let row = initial.first, row[kCGWindowNumber as String] as? UInt32 == number,
      let bounds = row[kCGWindowBounds as String] as? [String:CGFloat] else { throw trace.refusal() }
    trace.at(.axTrust)
    try trace.require(AXIsProcessTrusted(),["trusted":false])
    let application = AXUIElementCreateApplication(pid)
    trace.at(.axMatch)
    // Exactly the former attr() call: a failed query still becomes nil/[],
    // and only the existing unique-match predicate refuses it.
    var raw: CFTypeRef?
    let query = AXUIElementCopyAttributeValue(application,kAXWindowsAttribute as CFString,&raw)
    let value: CFTypeRef? = query == .success ? raw : nil
    let windowValue = value as? [AXUIElement], windows = windowValue ?? []
    let matched = windows.filter {
      guard owns($0,pid), let p = xy($0), let s = wh($0) else { return false }
      return abs(p.x-(bounds["X"] ?? -10000)) < 3 && abs(p.y-(bounds["Y"] ?? -10000)) < 3 &&
        abs(s.width-(bounds["Width"] ?? -10000)) < 3 && abs(s.height-(bounds["Height"] ?? -10000)) < 3
    }
    try trace.require(matched.count == 1,["queryClass":discoveryQueryClass(query),
      "valueClass":windowValue != nil ? "array" : value == nil ? "missing" : "wrong-type",
      "candidates":min(windows.count,2),"matches":min(matched.count,2)])
    return OwnedInitialWindow(scope:scope,pid:pid,start:pinnedProcessStart,number:number,physical:physical,
      reportIdentity:reportIdentity,application:application,main:matched[0],cg:cg)
  } catch let error as OwnedDiscoveryRefusal { throw error }
  catch { throw trace.refusal() }
}

// Ordered public controls; the semantic region marker is inspection-only.
func ownedControl(_ reference: Bool, _ title: String, _ value: String, _ description: ()->String?, inspection: Bool = false) -> String? {
  var labels: Set<String> = ["Open","Open Project…","Cancel","A","B","A.json","B.json","Motion","Opacity","R08 rectangle"]
  if inspection { labels.insert("Project tabs") }
  if reference { labels.formUnion(["File","Edit","Save","Save As…","Save As","Save as…","Open…","Undo","Redo",
    "Reference","Reference…","None","No reference","Visible","Remove","Import Reference Image",
    "ReferenceA.png","ReferenceB.png","Saved.json","frame_0001.png","Export","Export…","Export PNG","PNG","PNG Sequence",
    "SVG Sequence","Work area","Whole timeline","1x","Choose","Select","Export Image","Replace",
    "Go","Go to the folder:","Go to folder:","Go to Folder","Go to the folder"]) }
  if labels.contains(title) { return title }
  if labels.contains(value) { return value }
  if let text = description(), labels.contains(text) { return text }
  return nil
}

struct OwnedProjectionCursor<Node> {
  let token: String
  let base: String
  let root: Node
  let children: [Node]
  let owner: Int
  var next: Int
  var foreign = false
  var unreadable = false
  var depthClipped = false
  var region: OwnedProjectRegion<Node>? = nil
}

struct OwnedProjectRegion<Node> { let token: String; let path: String; let node: Node; let owner: Int }

enum OwnedOwnerQuery: String {
  case matched, mismatch, failed = "query-failed"
  init(_ actual: Int?, _ expected: Int) {
    self = actual == nil ? .failed : actual == expected ? .matched : .mismatch
  }
}

enum OwnedChildrenQuery: String {
  case success, unsupported, noValue = "no-value", failed, wrongType = "wrong-type"
}
enum OwnedChildrenCountQuery: String {
  case zero = "success-zero", positive = "success-positive", unsupported, noValue = "no-value", failed, invalid
}

// No-value is empty only with an independently owned zero-count witness for
// this observation. Unsupported/failed/inconsistent queries stay unreadable.
struct OwnedChildren<Node> {
  let query: OwnedChildrenQuery
  let values: [Node]?
  let countQuery: OwnedChildrenCountQuery?
  let ownerRefusal: OwnedOwnerQuery?
  init(query: OwnedChildrenQuery, values: [Node]?, countQuery: OwnedChildrenCountQuery? = nil,
    ownerRefusal: OwnedOwnerQuery? = nil) {
    self.query = query == .success && values == nil ? .wrongType : query
    self.countQuery = countQuery; self.ownerRefusal = ownerRefusal
    let empty = self.query == .noValue && values == nil && countQuery == .zero && ownerRefusal == nil
    self.values = ownerRefusal != nil ? nil : self.query == .success ? values : empty ? [] : nil
  }
  var diagnostic: String { query == .success && values?.isEmpty == true ? "success-empty" : query.rawValue }
  var witness: String? { query == .noValue && countQuery == .zero && values?.isEmpty == true && ownerRefusal == nil ? "empty-count-confirmed" : nil }
}

// One retained semantic object; a changed route/label/owner is never re-pinned.
func retainedProjectTabs<Node>(_ current: Node, path: String, owner: Int, control: String?,
  request: [String:Any], projection: [String:Any], cursor: inout OwnedProjectionCursor<Node>?,
  equal: (Node,Node)->Bool) throws -> [String:Any] {
  try check(request["projectTabs"] as? Bool == true && request["childOffset"] == nil &&
    control == "Project tabs" && projection["complete"] as? Bool == true)
  try check(["truncated","depthClipped","childrenUnreadable","foreignChildren"].allSatisfy { projection[$0] as? Bool == false })
  let retained: OwnedProjectRegion<Node>
  if let previous = cursor?.region {
    try check(request["projectTabsToken"] as? String == previous.token && previous.path == path &&
      previous.owner == owner && equal(previous.node,current))
    retained = previous
  } else {
    try check(request["projectTabsToken"] == nil)
    retained = OwnedProjectRegion(token:UUID().uuidString.lowercased(),path:path,node:current,owner:owner)
  }
  var holder = cursor ?? OwnedProjectionCursor(token:retained.token,base:path,root:current,children:[],owner:owner,next:0)
  holder.region = retained; cursor = holder
  return ["path":retained.path,"token":retained.token]
}

func ownedProjection<Node>(_ root: Node, base: String, depth: Int, maxNodes: Int,
  expectedOwner: Int, request: [String:Any], cursor: inout OwnedProjectionCursor<Node>?,
  equal: (Node,Node)->Bool, hashKey: (Node)->UInt, owner: (Node)->Int?, children: (Node)->OwnedChildren<Node>,
  isWindow: (Node)->Bool, fact: (Node,String)->[String:Any],
  phase: (String,String)->Void) throws -> [String:Any] {
  try check(depth >= 1 && depth <= 5 && maxNodes >= 1 && maxNodes <= 128)
  let paged = request["childOffset"] != nil
  var offset = 0, token: String? = nil
  if paged {
    guard let raw = request["childOffset"] as? NSNumber,
      CFGetTypeID(raw) != CFBooleanGetTypeID(),
      let value = request["childOffset"] as? Int else { throw Refusal(code:"adapter-refused") }
    try check(raw.doubleValue == Double(value) && value >= 0 && value < 4096 && depth == 1 && maxNodes >= 2)
    offset = value
    if let rawToken = request["pageToken"] {
      guard let supplied = rawToken as? String, UUID(uuidString:supplied) != nil else { throw Refusal(code:"adapter-refused") }
      token = supplied
    }
  } else {
    try check(request["pageToken"] == nil)
    if cursor?.region == nil { cursor = nil }
  }
  var nodes = [[String:Any]](), truncated = false, depthClipped = false, unreadable = false, foreign = false
  func ownership(_ node: Node) -> OwnedOwnerQuery {
    phase("ax-owner-query","ownership-refused")
    return OwnedOwnerQuery(owner(node),expectedOwner)
  }
  func blocked(_ path: String, _ query: OwnedOwnerQuery) -> [String:Any] {
    ["path":path,"role":"other","blocked":"child-owner","ownerQuery":query.rawValue]
  }
  guard ownership(root) == .matched else { cursor = nil; throw Refusal(code:"child-owner") }
  phase("ax-children-query","attribute-value-unreadable")
  let rootObservation = children(root), rootChildren = rootObservation.values
  guard rootObservation.ownerRefusal == nil && ownership(root) == .matched else { cursor = nil; throw Refusal(code:"child-owner") }
  if let list = rootChildren { try check(list.count < 4096) }
  if paged {
    guard let list = rootChildren else { cursor = nil; throw Refusal(code:"children-unavailable") }
    try check(offset <= list.count)
    if offset == 0 {
      try check(token == nil)
      var seen = [UInt:[Node]]()
      for child in list {
        let key = hashKey(child)
        try check(!(seen[key] ?? []).contains { equal($0,child) })
        seen[key,default:[]].append(child)
      }
      cursor = OwnedProjectionCursor(token:UUID().uuidString.lowercased(),base:base,root:root,children:list,owner:expectedOwner,next:0,region:cursor?.region)
    } else {
      guard let pinned = cursor, token == pinned.token, pinned.next == offset,
        pinned.base == base, pinned.owner == expectedOwner, equal(pinned.root,root),
        pinned.children.count == list.count,
        zip(pinned.children,list).allSatisfy({ equal($0.0,$0.1) }) else {
        cursor = nil; throw Refusal(code:"adapter-refused")
      }
    }
  }
  func project(_ current: Node, _ path: String, _ level: Int) throws {
    let before = ownership(current)
    guard before == .matched else {
      foreign = true; nodes.append(blocked(path,before)); return
    }
    if level > 0 && isWindow(current) { throw Refusal(code:"standalone-window") }
    phase("ax-attribute-query","attribute-query-failed")
    nodes.append(fact(current,path))
    // The fact adapter rechecks ownership independently. A transient failure
    // there cannot be erased by a later successful PID query.
    if nodes.last?["blocked"] as? String == "child-owner" { foreign = true; return }
    let afterFact = ownership(current)
    guard afterFact == .matched else {
      foreign = true; nodes[nodes.count-1] = blocked(path,afterFact); return
    }
    phase("ax-children-query","attribute-value-unreadable")
    let observation = level == 0 ? rootObservation : children(current)
    if let failure = observation.ownerRefusal {
      foreign = true; nodes[nodes.count-1] = blocked(path,failure); return
    }
    let afterChildren = ownership(current)
    guard afterChildren == .matched else {
      foreign = true; nodes[nodes.count-1] = blocked(path,afterChildren); return
    }
    nodes[nodes.count-1]["ownerQuery"] = afterChildren.rawValue
    nodes[nodes.count-1]["childrenQuery"] = observation.diagnostic
    if let countQuery = observation.countQuery { nodes[nodes.count-1]["childrenCountQuery"] = countQuery.rawValue }
    if let witness = observation.witness { nodes[nodes.count-1]["childrenWitness"] = witness }
    guard let list = observation.values else {
      unreadable = true
      if level < depth { nodes[nodes.count-1]["blocked"] = "children-unavailable" }
      else { depthClipped = true }
      return
    }
    phase("ax-projection","topology-refused")
    try check(list.count < 4096)
    nodes[nodes.count-1]["childCount"] = list.count
    guard level < depth else { if !list.isEmpty { depthClipped = true }; return }
    for (index,child) in list.enumerated().dropFirst(level == 0 ? offset : 0) {
      if nodes.count >= maxNodes { truncated = true; return }
      let childPath = path.isEmpty ? String(index) : path+"."+String(index)
      try project(child,childPath,level+1)
    }
  }
  try project(root,base,0)
  guard ownership(root) == .matched else { cursor = nil; throw Refusal(code:"child-owner") }
  if paged {
    guard let pinned = cursor, let now = children(root).values,
      now.count == pinned.children.count,
      zip(pinned.children,now).allSatisfy({ equal($0.0,$0.1) }), ownership(root) == .matched else {
      cursor = nil; throw Refusal(code:"adapter-refused")
    }
  }
  var result: [String:Any] = ["nodes":nodes,"truncated":truncated,"depthClipped":depthClipped,
    "childrenUnreadable":unreadable,"foreignChildren":foreign,
    "complete":!truncated && !depthClipped && !unreadable && !foreign]
  if paged, var pinned = cursor {
    pinned.foreign = pinned.foreign || foreign
    pinned.unreadable = pinned.unreadable || unreadable
    pinned.depthClipped = pinned.depthClipped || depthClipped
    result["foreignChildren"] = pinned.foreign
    result["childrenUnreadable"] = pinned.unreadable
    result["depthClipped"] = pinned.depthClipped
    pinned.next = min(pinned.children.count,offset+nodes.count-1)
    result["pageToken"] = pinned.token
    result["childOffset"] = offset
    result["nextChildOffset"] = pinned.next
    result["childCount"] = pinned.children.count
    result["childrenComplete"] = pinned.next == pinned.children.count && !pinned.foreign
    result["complete"] = pinned.next == pinned.children.count && !pinned.foreign && !pinned.unreadable && !pinned.depthClipped
    cursor = pinned.next == pinned.children.count && pinned.region == nil ? nil : pinned
  }
  return result
}

// Shared unchanged retained-window predicate sequence; no rediscovery.
func verifyOwnedWindow(_ initial: OwnedInitialWindow) throws {
    let pid=initial.pid, pinnedProcessStart=initial.start, number=initial.number
    let main=initial.main, application=initial.application, cg=initial.cg
    guard processStart(pid) == pinnedProcessStart else {
      throw IdentityRefusal(stage:"process-start",facts:["processStartMatches":false])
    }
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
