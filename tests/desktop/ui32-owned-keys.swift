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
func ui32MotionFocus(_ pinned:OwnedInitialWindow, _ expected:AXUIElement?, _ text:Bool) throws {
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
    if CFEqual(current,pinned.main) { try check(inMotion);return }
    guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pinned.pid),
      let rows=ownedAXChildren(parent,expectedOwner:Int(pinned.pid)).values,rows.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"child-owner") }
    current=parent
  }
  throw Refusal(code:"adapter-refused")
}
func runUI32Keys(_ command:String, _ option:String?, _ pinned:OwnedInitialWindow,
  resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void) throws -> [String:Any] {
  try verifyOwnedWindow(pinned)
  if command == "motion.focus" {
    try check(option == "search" || option == "toolbar")
    let search=option == "search", rule=try ui32Rule(search ? "motion.search" : "motion.reveal",search ? nil : "All")
    let node=try resolve(rule);try visible(node);var writable=DarwinBoolean(false)
    try check(AXUIElementIsAttributeSettable(node,kAXFocusedAttribute as CFString,&writable) == .success && writable.boolValue)
    try verifyOwnedWindow(pinned);try check(CFEqual(try resolve(rule),node))
    try check(AXUIElementSetAttributeValue(node,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
    try ui32MotionFocus(pinned,node,search)
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
      try ui32MotionFocus(pinned,search,typing);press.postToPid(pinned.pid);down=true
      try ui32MotionFocus(pinned,search,typing);release.postToPid(pinned.pid);up=true;postedPairs += 1
      try ui32MotionFocus(pinned,search,typing)
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
  let first=try ui32Key("P"), shifted=try ui32Key("shift+T")
  try check(first.code == 35 && shifted.shift)
  for denied in ["Enter","z","super+s","shift+U","click","", "PP"] {
    var refused=false;do { _ = try ui32Key(denied) } catch { refused=true };try check(refused)
  }
}
