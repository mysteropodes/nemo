// Only the already admitted Initial-text fixture; no caller-selected file or route.
import Foundation
import ApplicationServices
import CryptoKit

let ui32FixtureHash="642bc4c681373c9a4e7ab3c1795f024ee1c7f277ac255298c5fdf8e1f57c8831"
func ui32FixtureURLMatches(_ observed:URL?, _ expected:URL) -> Bool {
  observed?.isFileURL == true && observed?.standardizedFileURL == expected
}
final class UI32Fixture {
  let pinned:OwnedInitialWindow
  let fixture:URL
  var attempted=false, completed=false, confirmed=false
  let cancelOnly:Bool
  var sheet:AXUIElement?, root:AXUIElement?, parent:AXUIElement?, owner:pid_t?, start:ProcessStart?
  var selected:AXUIElement?
  init(_ pinned:OwnedInitialWindow, _ environment:[String:String], cancelOnly:Bool=false) throws {
    self.pinned=pinned;self.cancelOnly=cancelOnly
    guard let path=environment["NEMO_UI32_FIXTURE"], path.hasPrefix("/") else { throw Refusal(code:"adapter-refused") }
    fixture=URL(fileURLWithPath:path).standardizedFileURL
    try fileProof()
  }
  func fileProof() throws {
    try check(fixture.resolvingSymlinksInPath() == fixture && fixture.lastPathComponent == "Initial-text.json")
    let attributes=try FileManager.default.attributesOfItem(atPath:fixture.path)
    try check(attributes[.type] as? FileAttributeType == .typeRegular && (attributes[.size] as? NSNumber)?.intValue == 7791)
    let bytes=try Data(contentsOf:fixture); try check(SHA256.hash(data:bytes).map {String(format:"%02x",$0)}.joined() == ui32FixtureHash)
  }
  func ownedTree(_ root:AXUIElement, _ pid:pid_t) throws -> [AXUIElement] {
    try boundedOwnedTree(root,owner:Int(pid),deadline:DispatchTime.now().uptimeNanoseconds+1_000_000_000,
      now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:{ node in
        var actual:pid_t=0;let q=AXUIElementGetPid(node,&actual)
        return (Int(q.rawValue),q == .success && actual == pid,Int(actual))
      },children:{ node in
        ui32Children(node,pid)
      },parent:{ node in
        guard AXUIElementSetMessagingTimeout(node,0.03) == .success else { return (-1,nil) }
        var raw:CFTypeRef?;let q=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
        return (Int(q.rawValue),axElement(raw))
      })
  }
  func attachedSheet() throws -> AXUIElement {
    try verifyOwnedWindow(pinned)
    guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement],
      let children=attr(pinned.main,kAXChildrenAttribute) as? [AXUIElement], windows.count <= 8, children.count <= 512 else { throw Refusal(code:"sheet-ambiguous") }
    var found=[AXUIElement]()
    for node in windows+children {
      try check(owns(node,pinned.pid))
      if attr(node,kAXRoleAttribute) as? String != kAXSheetRole { continue }
      guard let ancestor=axElement(attr(node,kAXParentAttribute)), owns(ancestor,pinned.pid), CFEqual(ancestor,pinned.main) else { throw Refusal(code:"sheet-parent") }
      if !found.contains(where:{CFEqual($0,node)}) { found.append(node) }
    }
    try check(found.count == 1)
    if let sheet { try check(CFEqual(sheet,found[0])) } else { sheet=found[0] }
    return found[0]
  }
  func panel() throws -> ([AXUIElement],pid_t) {
    let outer=try attachedSheet();var foreign=[Int]()
    var currentRoot=outer, currentParent=pinned.main, currentOwner=pinned.pid, currentStart=pinned.start
    do {
      let found=try attachedForeignRoot(outer,owner:Int(pinned.pid),deadline:DispatchTime.now().uptimeNanoseconds+1_000_000_000,
        now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:{ node in
          var actual:pid_t=0;let q=AXUIElementGetPid(node,&actual)
          return (Int(q.rawValue),q == .success && actual == self.pinned.pid,Int(actual))
        },children:{ node in
          ui32Children(node,self.pinned.pid)
        },parent:{ node in
          guard AXUIElementSetMessagingTimeout(node,0.03) == .success else { return (-1,nil) }
          var raw:CFTypeRef?;let q=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
          return (Int(q.rawValue),axElement(raw))
        },foreignChild:{ _,pid in foreign.append(pid) })
      let pid=pid_t(found.pid)
      guard let stamp=processStart(pid), authenticatedPanelService(pid), processStart(pid) == stamp else { throw Refusal(code:"sheet-owner") }
      try check(AXUIElementSetMessagingTimeout(found.root,0.03) == .success)
      guard let observedParent=axElement(attr(found.root,kAXParentAttribute)), owns(observedParent,pinned.pid) else { throw Refusal(code:"sheet-parent") }
      let reaches=ownedAncestor(observedParent,outer,equal:{CFEqual($0,$1)},owned:{owns($0,self.pinned.pid)},
        isWindow:{attr($0,kAXRoleAttribute) as? String == kAXWindowRole},parent:{axElement(attr($0,kAXParentAttribute))},children:{attr($0,kAXChildrenAttribute) as? [AXUIElement]})
      try check(attachedServiceParentValid(CFEqual(observedParent,found.parent),pinned.pid,pinned.pid,reaches))
      currentRoot=found.root;currentParent=observedParent;currentOwner=pid;currentStart=stamp
    } catch let error as ActionRefusal {
      // Only a completed walk proving ZERO foreign roots may use same-PID AX.
      guard foreign.isEmpty && error.diagnostics["stage"] as? String == "candidate-count" else { throw error }
    }
    if let root, let parent, let owner, let start {
      try check(CFEqual(root,currentRoot) && CFEqual(parent,currentParent) && owner == currentOwner && start == currentStart)
    } else { root=currentRoot;parent=currentParent;owner=currentOwner;start=currentStart }
    let rows=try ownedTree(currentRoot,currentOwner)
    try verifyOwnedWindow(pinned);_ = try attachedSheet()
    try check(processStart(currentOwner) == currentStart && (currentOwner == pinned.pid || authenticatedPanelService(currentOwner)))
    return (rows,currentOwner)
  }
  func exactRow(_ rows:[AXUIElement], _ pid:pid_t) throws -> AXUIElement {
    let matches=rows.filter { node in
      guard owns(node,pid) else { return false }
      return ui32FixtureURLMatches(attr(node,kAXURLAttribute) as? URL,fixture)
    }
    guard matches.count == 1 else { throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"fixture-url-match","countBucket":min(matches.count,2)]) }
    var current=matches[0],visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
      if attr(current,kAXRoleAttribute) as? String == kAXRowRole { return current }
      guard let parent=axElement(attr(current,kAXParentAttribute)), owns(parent,pid),
        let children=attr(parent,kAXChildrenAttribute) as? [AXUIElement],children.count <= 512,
        children.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"sheet-parent") }
      current=parent
    }
    throw Refusal(code:"adapter-refused")
  }
  func singleSelectedFile(_ rows:[AXUIElement], _ pid:pid_t, _ expected:AXUIElement) throws {
    try check(CFEqual(try exactRow(rows,pid),expected) && rows.contains {CFEqual($0,expected)})
    var current=expected,outline:AXUIElement?,visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
      if attr(current,kAXRoleAttribute) as? String == kAXOutlineRole { outline=current;break }
      guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pid),
        let children=ownedAXChildren(parent,expectedOwner:Int(pid)).values,children.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"sheet-parent") }
      current=parent
    }
    guard let outline,rows.contains(where:{CFEqual($0,outline)}) else { throw Refusal(code:"adapter-refused") }
    var rawRows:CFTypeRef?,rawSelection:CFTypeRef?
    try check(AXUIElementCopyAttributeValue(outline,kAXRowsAttribute as CFString,&rawRows) == .success)
    try check(AXUIElementCopyAttributeValue(outline,"AXSelectedRows" as CFString,&rawSelection) == .success)
    guard let files=rawRows as? [AXUIElement], let selection=rawSelection as? [AXUIElement] else { throw Refusal(code:"sheet-ambiguous") }
    try ui32SingleSelection(files,selection,expected,equal:{CFEqual($0,$1)},owned:{ node in owns(node,pid) && rows.contains {CFEqual($0,node)} },selected:{ file in
      var raw:CFTypeRef?;try check(AXUIElementCopyAttributeValue(file,kAXSelectedAttribute as CFString,&raw) == .success)
      guard let selected=raw as? Bool else { throw Refusal(code:"adapter-refused") };return selected
    })
  }
  func perform(_ requested:String) throws -> [String:Any] {
    if cancelOnly { try check(["sync.panel-observe","sync.cancel-only","sync.complete"].contains(requested)) }
    else { try check(requested.hasPrefix("fixture.")) }
    let command=requested == "sync.cancel-only" ? "fixture.cancel" : requested == "sync.complete" ? "fixture.complete" : requested == "sync.panel-observe" ? "fixture.observe" : requested
    try check(attempted && !completed);try fileProof();try verifyOwnedWindow(pinned)
    if command == "fixture.complete" {
      guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement], windows.count == 1,
        owns(windows[0],pinned.pid),CFEqual(windows[0],pinned.main) else { throw Refusal(code:"sheet-active") }
      let finalTree=try ownedTree(pinned.main,pinned.pid)
      try check(!finalTree.contains { attr($0,kAXRoleAttribute) as? String == kAXSheetRole });completed=true
      return ["complete":true,"fixtureConfirmed":confirmed,"note":"Requires actual native SDK fixture baseline; not acceptance."]
    }
    let (rows,pid)=try panel()
    if command == "fixture.observe" {
      return ["panelOwner":pid,"nodes":rows.map { node -> [String:Any] in
        ["role":attr(node,kAXRoleAttribute) as? String ?? "", "title":String((attr(node,kAXTitleAttribute) as? String ?? "").prefix(120)),
         "value":String((attr(node,kAXValueAttribute) as? String ?? "").prefix(120))]
      }]
    }
    var attemptedAction=false
    do {
      if command == "fixture.choose" {
        try check(selected == nil)
        let before=try exactRow(rows,pid)
        let (latest,latestPID)=try panel();try check(latestPID == pid)
        let row=try exactRow(latest,pid);try check(CFEqual(row,before))
        var writable=DarwinBoolean(false)
        try check(AXUIElementIsAttributeSettable(row,kAXSelectedAttribute as CFString,&writable) == .success && writable.boolValue)
        try fileProof();attemptedAction=true
        try check(AXUIElementSetAttributeValue(row,kAXSelectedAttribute as CFString,kCFBooleanTrue) == .success)
        selected=row;let (fresh,freshPID)=try panel();try check(freshPID == pid)
        try singleSelectedFile(fresh,pid,row)
      } else {
        try check(command == "fixture.confirm" || command == "fixture.cancel")
        if command == "fixture.confirm" {
          guard let selected else { throw Refusal(code:"adapter-refused") }
          try check(CFEqual(try exactRow(rows,pid),selected) && attr(selected,kAXSelectedAttribute) as? Bool == true)
        }
        let labels:Set<String> = command == "fixture.confirm" ? ["Open","Ouvrir","開く","Abrir"] : ["Cancel","Annuler","キャンセル","Cancelar"]
        let buttons=rows.filter { attr($0,kAXRoleAttribute) as? String == kAXButtonRole && labels.contains(attr($0,kAXTitleAttribute) as? String ?? "") && attr($0,kAXEnabledAttribute) as? Bool == true }
        try check(buttons.count == 1);var actions:CFArray?
        try check(AXUIElementCopyActionNames(buttons[0],&actions) == .success && (actions as? [String] ?? []).contains(kAXPressAction as String))
        let (latest,latestPID)=try panel();try check(latestPID == pid)
        let freshButtons=latest.filter { attr($0,kAXRoleAttribute) as? String == kAXButtonRole && labels.contains(attr($0,kAXTitleAttribute) as? String ?? "") && attr($0,kAXEnabledAttribute) as? Bool == true }
        try check(freshButtons.count == 1 && CFEqual(freshButtons[0],buttons[0]))
        var freshActions:CFArray?
        try check(AXUIElementCopyActionNames(freshButtons[0],&freshActions) == .success && (freshActions as? [String] ?? []).contains(kAXPressAction as String))
        if command == "fixture.confirm" {
          guard let selected else { throw Refusal(code:"adapter-refused") }
          try singleSelectedFile(latest,pid,selected)
        }
        try fileProof();attemptedAction=true
        try check(AXUIElementPerformAction(freshButtons[0],kAXPressAction as CFString) == .success)
        if command == "fixture.confirm" { confirmed=true }
        try verifyOwnedWindow(pinned)
      }
      return ["actionAttempted":attemptedAction,"actionPosted":true,"panelOwner":pid]
    } catch { throw ActionRefusal(diagnostics:["purpose":"UI32","command":command,"actionAttempted":attemptedAction,"stage":"fixture-guard-or-action","retry":false]) }
  }
}

// Production selected-set gate, exercised with foreign/extra/unreadable controls.
func ui32SingleSelection<Node>(_ rows:[Node], _ selection:[Node], _ expected:Node,
  equal:(Node,Node)->Bool, owned:(Node)->Bool, selected:(Node) throws -> Bool) throws {
  try check(!rows.isEmpty && rows.count <= 512 && selection.count == 1 && owned(selection[0]) && equal(selection[0],expected))
  var count=0
  for row in rows {
    try check(owned(row))
    if try selected(row) { count += 1;try check(equal(row,expected)) }
  }
  try check(count == 1)
}
func ui32FixtureSelfTest() throws {
  for fault in ["none","extra","wrong","foreign","unreadable","missing"] {
    var refused=false
    do { try ui32SingleSelection([1,2],fault == "extra" ? [1,2] : fault == "wrong" ? [2] : [1],1,equal:==,
      owned:{fault != "foreign" || $0 != 2},selected:{ row in
        if fault == "unreadable" && row == 2 { throw Refusal(code:"adapter-refused") }
        return row == 1 && fault != "missing"
      }) } catch { refused=true }
    try check(refused == (fault != "none"))
  }
  let expected=URL(fileURLWithPath:"/private/fixture/Initial-text.json")
  try check(ui32FixtureURLMatches(expected,expected))
  try check(!ui32FixtureURLMatches(nil,expected) && !ui32FixtureURLMatches(URL(string:"https://example.invalid/Initial-text.json"),expected))
  try check(!ui32FixtureURLMatches(URL(fileURLWithPath:"/private/other/Initial-text.json"),expected))
}
