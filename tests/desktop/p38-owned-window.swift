// P38V2 test-only persistent main/sheet control; no global input or rediscovery.
import Foundation
import ApplicationServices
import CoreGraphics

struct Refusal: Error { let code: String }
struct SelectionRefusal: Error { let diagnostics: [String:Any] }
let allowedReasons: Set<String> = ["adapter-refused","sheet-owner","sheet-parent","sheet-replaced","child-owner","confirm-unsupported","children-unavailable","sheet-ambiguous","sheet-active","standalone-window"]
func check(_ ok: Bool) throws { if !ok { throw Refusal(code:"adapter-refused") } }
let actions: Set<String> = ["inspect", "press", "set", "select", "raise", "capture", "check", "pin-sheet", "sheet-dismissed", "quit"]
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
func selfTest() throws {
  try check(try route("0.12.3") == [0,12,3])
  for input in ["-1", "0..1", ".0", "1.", "x", "4096", "999999999999999999999", String(repeating: "1.", count: 21)] {
    var refused = false; do { _ = try route(input) } catch { refused = true }; try check(refused)
  }
  try check(!actions.contains("key") && !actions.contains("focus") && !actions.contains("launch"))
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
  try emit(["status":"static-model-pass", "routeNegatives":8, "sheetNegatives":7, "foreignReads":0, "runtimeAX":false])
}
func run() throws {
  let args = CommandLine.arguments
  if args == [args[0], "--self-test"] { try selfTest(); return }
  try check(args.count == 3 && args[1] == "--persistent" && ProcessInfo.processInfo.environment["NEMO_P38_GRANT"]?.isEmpty == false)
  guard let pid = Int32(args[2]), pid > 0, let report = ProcessInfo.processInfo.environment["NEMO_P38_REPORT_ROOT"] else { throw Refusal(code:"adapter-refused") }
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
  func cg(_ all: Bool) -> [[String:Any]] {
    let flags: CGWindowListOption = all ? [.optionAll,.excludeDesktopElements] : [.optionOnScreenOnly,.excludeDesktopElements]
    return (CGWindowListCopyWindowInfo(flags,kCGNullWindowID) as? [[String:Any]] ?? []).filter {
      ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
    }
  }
  let initial = cg(false); try check(initial.count == 1)
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
  func verify() throws {
    try check(owns(main,pid) && (attr(application,kAXWindowsAttribute) as? [AXUIElement] ?? []).contains { CFEqual($0,main) })
    let minimized = attr(main,kAXMinimizedAttribute) as? Bool ?? false
    let identities = cg(true).filter { $0[kCGWindowNumber as String] as? UInt32 == number }
    // Retain the exact main AX object across picker presentation; no rediscovery.
    try check(identities.count == 1 && !minimized)
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
    if action != "check" { try check(ordinal < 64); ordinal += 1 }
    if action == "quit" { try verify(); try emit(["status":"quit", "pid":pid,"windowId":number]); return }
    try verify()
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
      guard let value = request["element"] as? String, !value.isEmpty else { throw Refusal(code:"adapter-refused") }
      if target == "main" { try noSheet() }
      let selected = try node(value,selectedRoot), operation = action == "press" ? kAXPressAction : kAXConfirmAction
      if action != "select" && !supports(selected,operation) { throw Refusal(code:action == "set" ? "confirm-unsupported" : "adapter-refused") }
      if action == "set" {
        try check(target == "sheet")
        guard let text = request["value"] as? String, text.utf8.count <= 4096 else { throw Refusal(code:"adapter-refused") }
        try check(attr(selected,kAXRoleAttribute) as? String == kAXTextFieldRole)
        var focusable = DarwinBoolean(false)
        try check(AXUIElementIsAttributeSettable(selected,kAXFocusedAttribute as CFString,&focusable) == .success && focusable.boolValue)
        try check(AXUIElementSetAttributeValue(selected,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
        guard let focused = attr(application,kAXFocusedUIElementAttribute) else { throw Refusal(code:"adapter-refused") }
        try check(CFEqual(focused,selected))
        try check(AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,text as CFString) == .success)
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
      } else { try check(AXUIElementPerformAction(selected,operation as CFString) == .success) }
      try verify()
      if target == "sheet" {
        if try members().isEmpty { result["dismissalPendingVerification"] = true }
        else { _ = try verifySheet() }
      }
    case "inspect":
      if target == "main" { try noSheet() }
      let base = request["element"] as? String ?? ""
      let selected = try node(base,selectedRoot)
      let roles: Set<String> = ["AXWindow","AXSheet","AXButton","AXTextField","AXStaticText","AXGroup","AXWebArea","AXCheckBox","AXMenu","AXMenuItem","AXRow","AXOutline","AXScrollArea"]
      let labels: Set<String> = ["Open","Open Project…","Cancel","A","B","A.json","B.json","Motion","Opacity","R08 rectangle"]
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
catch let error as Refusal { try? emit(["status":"refused","code":allowedReasons.contains(error.code) ? error.code : "adapter-refused"]); exit(1) }
catch { try? emit(["status":"refused","code":"adapter-refused"]); exit(1) }
