// P38 owned-window adapter. No global keyboard/mouse events or dialog-window fallback.
import Foundation
import ApplicationServices
import CoreGraphics

func require(_ value: Bool, _ message: String) throws {
  if !value { throw NSError(domain: "P38", code: 1, userInfo: [NSLocalizedDescriptionKey: message]) }
}
func attr(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
  var value: CFTypeRef?
  return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
}
func children(_ element: AXUIElement) -> [AXUIElement] {
  return attr(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
}
func point(_ element: AXUIElement) -> CGPoint? {
  guard let raw = attr(element, kAXPositionAttribute) else { return nil }
  var value = CGPoint.zero
  return AXValueGetValue(raw as! AXValue, .cgPoint, &value) ? value : nil
}
func size(_ element: AXUIElement) -> CGSize? {
  guard let raw = attr(element, kAXSizeAttribute) else { return nil }
  var value = CGSize.zero
  return AXValueGetValue(raw as! AXValue, .cgSize, &value) ? value : nil
}
func emit(_ value: [String: Any]) throws {
  let data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
  print(String(data: data, encoding: .utf8)!)
}
func run() throws {
  let args = CommandLine.arguments
  try require(ProcessInfo.processInfo.environment["NEMO_P38_GRANT"]?.isEmpty == false, "grant required")
  try require(args.count >= 3, "owned PID required")
  guard let pid = Int32(args[1]), pid > 0 else { throw NSError(domain: "P38", code: 2) }
  try require(AXIsProcessTrusted(), "Accessibility unavailable")
  let owned = (CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []).filter {
    ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowLayer as String] as? Int) == 0
  }
  let discover = args[2] == "discover"
  if discover { try require(owned.count == 1, "one initial owned window required") }
  let selected = discover ? owned : owned.filter { ($0[kCGWindowNumber as String] as? UInt32) == UInt32(args[2]) }
  try require(selected.count == 1, "owned window identity changed")
  guard let info = selected.first, let number = info[kCGWindowNumber as String] as? UInt32,
        let bounds = info[kCGWindowBounds as String] as? [String: CGFloat] else { throw NSError(domain: "P38", code: 3) }
  if !discover { try require(UInt32(args[2]) == number && args.count >= 4, "window identity changed") }
  let application = AXUIElementCreateApplication(pid)
  let axWindows = attr(application, kAXWindowsAttribute) as? [AXUIElement] ?? []
  try require(axWindows.count == 1, "separate AX dialog/window forbidden")
  let candidates = axWindows.filter {
    guard let p = point($0), let s = size($0) else { return false }
    return abs(p.x - (bounds["X"] ?? -10000)) < 3 && abs(p.y - (bounds["Y"] ?? -10000)) < 3 &&
      abs(s.width - (bounds["Width"] ?? -10000)) < 3 && abs(s.height - (bounds["Height"] ?? -10000)) < 3
  }
  try require(candidates.count == 1, "ambiguous AX/window match")
  let window = candidates[0]
  var result: [String: Any] = ["pid": pid, "windowId": number, "matchedAX": true]
  if discover { try emit(result); return }
  let action = args[3]
  func node(_ route: String) throws -> AXUIElement {
    var current = window
    let parts = route.split(separator: ".")
    try require(!parts.isEmpty && parts.count <= 20, "bounded element path required")
    for part in parts {
      guard let index = Int(part), children(current).indices.contains(index) else { throw NSError(domain: "P38", code: 4) }
      current = children(current)[index]
    }
    var actual: pid_t = 0
    try require(AXUIElementGetPid(current, &actual) == .success && actual == pid, "foreign AX element")
    return current
  }
  switch action {
  case "check": break
  case "raise": try require(AXUIElementPerformAction(window, kAXRaiseAction as CFString) == .success, "raise refused")
  case "press":
    try require(args.count == 5, "element required")
    let element = try node(args[4]); var actions: CFArray?
    try require(AXUIElementCopyActionNames(element, &actions) == .success &&
      (actions as? [String] ?? []).contains(kAXPressAction), "AXPress unsupported; no fallback")
    try require(AXUIElementPerformAction(element, kAXPressAction as CFString) == .success, "press refused")
  case "set":
    try require(args.count == 6, "element/value required")
    try require(AXUIElementSetAttributeValue(try node(args[4]), kAXValueAttribute as CFString, args[5] as CFString) == .success, "set refused")
  case "capture":
    try require(args.count == 5 && args[4].hasPrefix("/"), "capture destination required")
    guard let reportRoot = ProcessInfo.processInfo.environment["NEMO_P38_REPORT_ROOT"] else { throw NSError(domain: "P38", code: 6) }
    let parent = URL(fileURLWithPath: args[4]).deletingLastPathComponent().resolvingSymlinksInPath().path
    try require(parent == reportRoot && URL(fileURLWithPath: reportRoot).resolvingSymlinksInPath().path == reportRoot, "capture containment refused")
    try require((try? FileManager.default.attributesOfItem(atPath: args[4])) == nil, "capture destination exists")
    let capture = Process(); capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
    capture.arguments = ["-x", "-o", "-l" + String(number), args[4]]
    try capture.run(); capture.waitUntilExit(); try require(capture.terminationStatus == 0, "capture failed")
  case "inspect":
    var nodes = [[String: Any]]()
    func visit(_ element: AXUIElement, _ route: String, _ depth: Int) throws {
      try require(nodes.count < 4096, "AX tree budget")
      if depth > 15 { return }
      var actual: pid_t = 0
      try require(AXUIElementGetPid(element, &actual) == .success && actual == pid, "foreign subtree")
      let role = attr(element, kAXRoleAttribute) as? String ?? ""
      let roles = ["AXWindow", "AXSheet", "AXButton", "AXTextField", "AXStaticText", "AXGroup", "AXWebArea", "AXCheckBox", "AXMenu", "AXMenuItem"]
      var facts: [String: Any] = ["path": route, "role": roles.contains(role) ? role : "other"]
      let title = attr(element, kAXTitleAttribute) as? String ?? ""
      let value = attr(element, kAXValueAttribute) as? String ?? ""
      let labels = ["Open", "Cancel", "A", "B", "Motion", "Opacity", "R08 rectangle"]
      if labels.contains(title) { facts["control"] = title }
      let alerts = ["Switching project tabs is unavailable in this build.": "switch-unavailable",
        "Closing the active project tab is unavailable in this build.": "active-close-unavailable"]
      if let alert = alerts[value] ?? alerts[title] { facts["alert"] = alert }
      // No arbitrary AX title/value, file paths, document text or owner token is persisted.
      nodes.append(facts)
      for (index, child) in children(element).enumerated() { try visit(child, route.isEmpty ? String(index) : route + "." + String(index), depth + 1) }
    }
    try visit(window, "", 0); result["nodes"] = nodes
  default: throw NSError(domain: "P38", code: 5)
  }
  let remainsOwned = (CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] ?? []).contains {
    ($0[kCGWindowOwnerPID as String] as? Int32) == pid && ($0[kCGWindowNumber as String] as? UInt32) == number
  }
  if !remainsOwned && action == "capture" { try? FileManager.default.removeItem(atPath: args[4]) }
  try require(remainsOwned, "owned window disappeared during operation")
  try emit(result)
}
do { try run() } catch { fputs("P38 owned-window operation refused; no fallback.\n", stderr); exit(1) }
