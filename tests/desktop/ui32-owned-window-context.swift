// macOS adds sharing-session chrome to AXWindows when a window is observed.
// This is not product content. Admit only its exact owned, nonmodal leaf shape;
// the pinned main window and its complete content proof remain mandatory.
import Foundation
import ApplicationServices

func ui32ContentWindows<Node>(_ windows:[Node], main:Node,
  equal:(Node,Node)->Bool, owned:(Node)->Bool,
  window:(Node)->(String?,String?,String?,Bool?),
  button:(Node)->(String?,String?), children:(Node)->[Node]?,
  parent:(Node)->Node?) throws -> Int {
  func require(_ ok:Bool) throws {if !ok {throw Refusal(code:"sheet-active")}}
  try require((1...2).contains(windows.count))
  try require(windows.filter {equal($0,main)}.count == 1)
  for node in windows {try require(owned(node))}
  let extras=windows.filter {!equal($0,main)}
  for node in extras {
    let (role,subrole,title,modal)=window(node)
    try require(role == "AXWindow" && subrole == "AXDialog" && title == "Window" && modal == false)
    guard let members=children(node),members.count == 1 else {throw Refusal(code:"sheet-active")}
    let control=members[0];try require(owned(control) && !equal(control,node) && !equal(control,main))
    let (controlRole,controlTitle)=button(control)
    try require(controlRole == "AXButton" && controlTitle == "WindowSharingSessionButton")
    guard let ancestor=parent(control) else {throw Refusal(code:"sheet-active")}
    try require(owned(ancestor) && equal(ancestor,node))
    guard let descendants=children(control) else {throw Refusal(code:"sheet-active")}
    try require(descendants.isEmpty)
  }
  return extras.count
}

func ui32ContentWindowContext(_ pinned:OwnedInitialWindow) throws -> Int {
  guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement] else {
    throw Refusal(code:"sheet-active")
  }
  return try ui32ContentWindows(windows,main:pinned.main,equal:{CFEqual($0,$1)},
    owned:{owns($0,pinned.pid)},window:{node in
      (attr(node,kAXRoleAttribute) as? String,attr(node,kAXSubroleAttribute) as? String,
       attr(node,kAXTitleAttribute) as? String,attr(node,kAXModalAttribute) as? Bool)
    },button:{node in (attr(node,kAXRoleAttribute) as? String,attr(node,kAXTitleAttribute) as? String)},
    children:{node in let (query,rows)=ui32Children(node,pinned.pid);return query == 0 ? rows : nil},
    parent:{axElement(attr($0,kAXParentAttribute))})
}

func ui32ContentWindowsSelfTest() throws {
  let faults=["none","main-only","reverse","missing-main","duplicate-main","duplicate-chrome",
    "foreign-window","foreign-control","foreign-parent","window-role","subrole","title","modal",
    "modal-unreadable","children-unreadable","children-extra","control-role","control-title",
    "control-parent","parent-unreadable","descendants","descendants-unreadable"]
  for fault in faults {
    let windows = fault == "main-only" ? [0] : fault == "reverse" ? [1,0] :
      fault == "missing-main" ? [1] : fault == "duplicate-main" ? [0,0] :
      fault == "duplicate-chrome" ? [0,1,1] : [0,1]
    var refused=false,foreignReads=0
    let foreign = fault == "foreign-window" ? 1 : fault == "foreign-control" ? 2 : fault == "foreign-parent" ? 3 : -1
    func read(_ node:Int) {if node == foreign {foreignReads += 1}}
    do {
      let count=try ui32ContentWindows(windows,main:0,equal:{$0 == $1},owned:{$0 != foreign},
        window:{node in read(node);return (fault == "window-role" ? "AXSheet" : "AXWindow",
          fault == "subrole" ? "AXStandardWindow" : "AXDialog",fault == "title" ? "Save" : "Window",
          fault == "modal-unreadable" ? nil : fault == "modal")},
        button:{node in read(node);return (fault == "control-role" ? "AXGroup" : "AXButton",
          fault == "control-title" ? "Open" : "WindowSharingSessionButton")},
        children:{node in read(node)
          if node == 1 {return fault == "children-unreadable" ? nil : fault == "children-extra" ? [2,4] : [2]}
          return fault == "descendants-unreadable" ? nil : fault == "descendants" ? [4] : []
        },parent:{node in read(node);return fault == "parent-unreadable" ? nil :
          ["foreign-parent","control-parent"].contains(fault) ? 3 : 1})
      try check(count == (fault == "main-only" ? 0 : 1))
    } catch {refused=true}
    try check(refused == !["none","main-only","reverse"].contains(fault) && foreignReads == 0)
  }
}
