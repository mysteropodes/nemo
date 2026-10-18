// Closed Motion-only keyboard cases. Events target one retained PID, never a tap.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

func ui32Key(_ option:String) throws -> (code:CGKeyCode,text:String,shift:Bool) {
  let shifted=option.hasPrefix("shift+"), label=shifted ? String(option.dropFirst(6)) : option
  let keys:[String:CGKeyCode]=["P":35,"A":0,"R":15,"S":1,"T":17,"U":32,"E":14,"M":46]
  guard let code=keys[label], !shifted || Set(["P","A","R","S","T"]).contains(label) else { throw Refusal(code:"adapter-refused") }
  return (code,shifted ? label : label.lowercased(),shifted)
}
// Mandatory shared route: production and static controls use the same three fences.
func ui32MotionKeyPair(_ focus:() throws -> Void, down:()->Void, up:()->Void) throws {
  try focus();down();try focus();up();try focus()
}
func ui32MotionContent<Node>(_ certificate:UI32ContentCertificate<Node>, _ focus:Node,
  equal:(Node,Node)->Bool, identifier:(Node) throws -> String?) throws {
  guard let target=certificate.nodes.firstIndex(where:{equal($0,focus)}),certificate.inside.contains(target) else {
    throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"motion-focus-outside-content"])
  }
  var inside=[Int](),outside=[Int]()
  for i in certificate.nodes.indices {
    if try identifier(certificate.nodes[i]) == "motion-props-body" {
      if certificate.inside.contains(i) {inside.append(i)} else {outside.append(i)}
    }
  }
  let context=try ui32ScopedCandidate(inside,outside)
  var current:Int?=target
  while let index=current {
    if index == context {return};current=certificate.declared[index]
  }
  throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"motion-focus-wrong-context"])
}
func ui32MotionFocus(_ pinned:OwnedInitialWindow, _ expected:AXUIElement?, _ text:Bool,
  contentScope:(AXUIElement) throws -> Void) throws {
  try verifyOwnedWindow(pinned)
  guard let window=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),owns(window,pinned.pid),CFEqual(window,pinned.main),
    let node=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(node,pinned.pid) else { throw Refusal(code:"adapter-refused") }
  if let expected { try check(CFEqual(node,expected)) }
  let role=attr(node,kAXRoleAttribute) as? String ?? ""
  try check(text ? role == kAXTextFieldRole : ![kAXTextFieldRole,"AXTextArea","AXComboBox"].contains(role))
  var current=node,visited=[AXUIElement](), inMotion=false
  for _ in 0..<20 {
    try check(owns(current,pinned.pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
    if attr(current,kAXIdentifierAttribute) as? String == "motion-props-body" { inMotion=true }
    if CFEqual(current,pinned.main) { try check(inMotion);try contentScope(node);return }
    guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pinned.pid),
      let rows=ownedAXChildren(parent,expectedOwner:Int(pinned.pid)).values,rows.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"child-owner") }
    current=parent
  }
  throw Refusal(code:"adapter-refused")
}
func runUI32Keys(_ command:String, _ option:String?, _ pinned:OwnedInitialWindow,
  resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void,
  contentScope:(AXUIElement) throws -> Void) throws -> [String:Any] {
  try verifyOwnedWindow(pinned)
  if command == "motion.focus" {
    try check(option == "search" || option == "toolbar")
    let search=option == "search", rule=try ui32Rule(search ? "motion.search" : "motion.reveal",search ? nil : "All")
    let node=try resolve(rule);try visible(node);var writable=DarwinBoolean(false)
    try check(AXUIElementIsAttributeSettable(node,kAXFocusedAttribute as CFString,&writable) == .success && writable.boolValue)
    try verifyOwnedWindow(pinned);try check(CFEqual(try resolve(rule),node))
    try check(AXUIElementSetAttributeValue(node,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
    try ui32MotionFocus(pinned,node,search,contentScope:contentScope)
    return ["focusAcknowledged":true,"note":"Focus proof only; no feature acceptance."]
  }
  try check(command == "motion.shortcut" || command == "motion.guard-typing")
  let typing=command == "motion.guard-typing"
  if typing { try check(option == nil) }
  let options=typing ? ["P","A","R","S","T"] : [option ?? ""]
  let search:AXUIElement?
  if typing { search=try resolve(ui32Rule("motion.search",nil)) } else { search=nil }
  var down=false,up=false,postedPairs=0
  do {
    for option in options {
      let key=try ui32Key(option)
      guard let source=CGEventSource(stateID:.privateState),
        let press=CGEvent(keyboardEventSource:source,virtualKey:key.code,keyDown:true),
        let release=CGEvent(keyboardEventSource:source,virtualKey:key.code,keyDown:false) else { throw Refusal(code:"adapter-refused") }
      let characters=Array(key.text.utf16)
      for event in [press,release] {
        event.flags=key.shift ? .maskShift : []
        characters.withUnsafeBufferPointer { buffer in event.keyboardSetUnicodeString(stringLength:characters.count,unicodeString:buffer.baseAddress!) }
      }
      down=false;up=false
      try ui32MotionKeyPair({ try ui32MotionFocus(pinned,search,typing,contentScope:contentScope) },
        down:{press.postToPid(pinned.pid);down=true},up:{release.postToPid(pinned.pid);up=true;postedPairs += 1})
    }
    return ["postedPairs":postedPairs,"note":"Exact PID event posting, not application acknowledgement or acceptance."]
  } catch { throw ActionRefusal(diagnostics:["purpose":"UI32","command":command,"postedPairs":postedPairs,"lastDownPosted":down,"lastUpPosted":up,"retry":false]) }
}
// One named additive selection gesture, on the verified F06-B BUTTON only.
func ui32ToggleB(_ pinned:OwnedInitialWindow, resolve:(UI32Rule) throws -> AXUIElement,
  visible:(AXUIElement) throws -> Void) throws -> [String:Any] {
  let rule=try ui32Rule("selection.B",nil), node=try resolve(rule)
  func context() throws {
    try verifyOwnedWindow(pinned)
    guard let focused=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),owns(focused,pinned.pid),CFEqual(focused,pinned.main) else { throw Refusal(code:"adapter-refused") }
    try check(CFEqual(try resolve(rule),node));try visible(node)
  }
  try context()
  guard let position=xy(node),let size=wh(node),let source=CGEventSource(stateID:.privateState) else { throw Refusal(code:"adapter-refused") }
  let point=CGPoint(x:position.x+size.width/2,y:position.y+size.height/2)
  guard let down=CGEvent(mouseEventSource:source,mouseType:.leftMouseDown,mouseCursorPosition:point,mouseButton:.left),
    let up=CGEvent(mouseEventSource:source,mouseType:.leftMouseUp,mouseCursorPosition:point,mouseButton:.left) else { throw Refusal(code:"adapter-refused") }
  down.flags = .maskCommand;up.flags = .maskCommand
  var downPosted=false,upPosted=false
  do {
    try context();down.postToPid(pinned.pid);downPosted=true
    try context();up.postToPid(pinned.pid);upPosted=true
    try verifyOwnedWindow(pinned)
    return ["buttonDownPosted":downPosted,"buttonUpPosted":upPosted,"note":"Named F06-B additive selection event pair only; verify actual native selection separately."]
  } catch { throw ActionRefusal(diagnostics:["purpose":"UI32","command":"selection.toggle.B","buttonDownPosted":downPosted,"buttonUpPosted":upPosted,"retry":false]) }
}
func ui32KeysSelfTest() throws {
  try ui32MotionScopeSelfTest()
  let first=try ui32Key("P"), shifted=try ui32Key("shift+T")
  try check(first.code == 35 && shifted.shift)
  for denied in ["Enter","z","super+s","shift+U","click","", "PP"] {
    var refused=false;do { _ = try ui32Key(denied) } catch { refused=true };try check(refused)
  }
}

// Actual certificate + production key-pair route; no event objects/native AX.
func ui32MotionScopeSelfTest() throws {
  for fault in ["none","outside-focus","outside-context","duplicate-context","missing-context",
    "wrong-context","move-after-down","move-after-up","unreadable-context"] {
    let graph=[0:[1,4],1:[2],2:[3],3:[],4:[]],parents=[1:0,2:1,3:2,4:0]
    var focus=fault == "outside-focus" ? 4 : fault == "wrong-context" ? 1 : 3
    var down=0,up=0,calls=0,refused=false
    let owner:(Int)->(Int,Bool,Int)={_ in (0,true,7)}
    let children:(Int)->(Int,[Int]?)={(0,graph[$0] ?? [])}
    let parent:(Int)->(Int,Int?)={(0,parents[$0])}
    do {
      try ui32MotionKeyPair({
        calls += 1
        let certificate=try ui32ContentCertificate(0,pid:7,retained:1,deadline:10,now:{0},equal:==,
          owner:owner,prepare:{_ in true},traits:{($0 == 0 ? "AXWindow" : $0 == 1 ? "AXWebArea" : "AXGroup",false)},
          children:children,parent:parent,strict:{web,deadline in
            try boundedOwnedTree(web,owner:7,deadline:deadline,now:{0},equal:==,owner:owner,children:children,parent:parent)
          })
        try ui32MotionContent(certificate,focus,equal:==,identifier:{ node in
          if fault == "unreadable-context" && node == 4 {throw Refusal(code:"adapter-refused")}
          if fault == "missing-context" {return nil}
          return node == 2 || (fault == "outside-context" && node == 4) ||
            (fault == "duplicate-context" && node == 3) ? "motion-props-body" : nil
        })
      },down:{down += 1;if fault == "move-after-down" {focus=4}},
        up:{up += 1;if fault == "move-after-up" {focus=4}})
    } catch {refused=true}
    try check(refused == (fault != "none"))
    if fault == "none" {try check(down == 1 && up == 1 && calls == 3)}
    else if fault == "move-after-down" {try check(down == 1 && up == 0 && calls == 2)}
    else if fault == "move-after-up" {try check(down == 1 && up == 1 && calls == 3)}
    else {try check(down == 0 && up == 0 && calls == 1)}
  }
}
