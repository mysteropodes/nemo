// Finite F03 input only. No document API, arbitrary setter, pointer or retry.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

enum F03Input: String, CaseIterable {
  case opacity40="f03.opacity.40", opacity60="f03.opacity.60"
  case undo="f03.history.undo", redo="f03.history.redo"
  case frame0="f03.frame.0", frame10="f03.frame.10", frame20="f03.frame.20"
  case menu="f03.file.menu", save="f03.file.save", saveAs="f03.file.save-as", reopen="f03.file.open"
  var value:String? {
    switch self {
    case .opacity40:return "40"
    case .opacity60:return "60"
    case .frame0:return "1"
    case .frame10:return "11"
    case .frame20:return "21"
    default:return nil
    }
  }
  var opacity:Bool { self == .opacity40 || self == .opacity60 }
  var field:Bool { value != nil }
  var staticStage:Bool { opacity || self == .undo || self == .redo }
  var menuEntry:Bool { self == .save || self == .saveAs || self == .reopen }
  var picker:Bool { self == .save || self == .saveAs || self == .reopen }
}
func f03Input(_ command:String, _ option:String?, _ value:String?) throws -> F03Input {
  guard option == nil, value == nil, let action=F03Input(rawValue:command) else {throw Refusal(code:"adapter-refused")}
  return action
}
// Static input is admitted only for the confirmed INITIAL fixture lineage,
// at zero-based frame0 (the real UI displays1), never the keyed terminal.
// This proves input scope, not SDK save/reopen/history or native semantics.
func f03StaticInputProof(_ input:F03Input, originalKind:String, confirmedKind:String?, displayedFrame:String) throws {
  guard input.staticStage else {return}
  try check(originalKind == "f03-initial" && confirmedKind == "f03-initial" && displayedFrame == "1")
}
// The same effect kernel is invoked by the real AX route and fault controls.
// Any first failure is terminal. Catch never probes AX or emits a release/retry.
final class F03InputKernel {
  private(set) var failed=false
  private(set) var attempted=[String](), acknowledged=[String]()
  func effect(_ name:String, identity:() throws -> Void, before:() throws -> Void,
    write:() throws -> Void, after:() throws -> Void) throws {
    try check(!failed)
    do {
      try ui32GuardedAction(identity,before,{
        self.attempted.append(name);try write()
      },postScope:after,posted:{self.acknowledged.append(name)})
    } catch {
      failed=true
      throw ActionRefusal(diagnostics:["purpose":"F03","stage":name,"attempted":attempted,
        "acknowledged":acknowledged,"underlying":ui32ActionCause(error),"retry":false])
    }
  }
  func stop(){failed=true}
}
func f03Rule(_ input:F03Input) -> UI32Rule {
  if input.opacity {return UI32Rule(identifier:nil,region:"motion-props-body",labels:["opacity component 1"],
    roles:["AXTextField"],operation:"value",values:[])}
  if input.field {return UI32Rule(identifier:"tl-cf",region:nil,labels:[],roles:["AXTextField"],operation:"value",values:[])}
  if input == .menu {return UI32Rule(identifier:"app-menu-btn",region:nil,labels:[],roles:["AXButton"],operation:"press",values:[])}
  let labels:Set<String>
  switch input {
  case .save:labels=["Save","Enregistrer","保存","Guardar"]
  case .saveAs:labels=["Save as…","Enregistrer sous…","名前を付けて保存…","Guardar como…"]
  case .reopen:labels=["Open…","Ouvrir…","開く…","Abrir…"]
  default:return UI32Rule(identifier:"btn-ff",region:nil,labels:[],roles:["AXButton"],operation:"focus",values:[])
  }
  return UI32Rule(identifier:nil,region:nil,labels:labels,roles:["AXMenuItem","AXGroup","AXStaticText"],operation:"press",values:[])
}
// Bind the selected menu entry to the actual container carrying the
// Settings witness AND export/render-manager sibling IDs. The exact same
// bounded reciprocal path predicate is exercised by the pure controls.
func f03MenuBinding<Node>(_ selected:Node, _ witness:Node,
  equal:(Node,Node)->Bool, owned:(Node)->Bool,
  parent:(Node) throws -> Node, children:(Node) throws -> [Node],
  identifier:(Node) throws -> String?) throws -> Node {
  func edge(_ current:Node, _ seen:[Node]) throws -> (Node,[Node]) {
    try check(owned(current));let ancestor=try parent(current)
    try check(owned(ancestor) && !seen.contains {equal($0,ancestor)})
    let rows=try children(ancestor)
    try check(rows.count <= 64 && rows.filter {equal($0,current)}.count == 1)
    for row in rows {try check(owned(row))}
    for i in rows.indices {try check(!rows.prefix(i).contains {equal($0,rows[i])})}
    return (ancestor,rows)
  }
  var current=witness,seen=[witness],container:Node?
  for _ in 0..<4 {
    let (ancestor,rows)=try edge(current,seen)
    var exports=0,managers=0
    for row in rows {
      try check(owned(row));let id=try identifier(row)
      if id == "ctx-export" {exports += 1}
      if id == "ctx-render-manager" {managers += 1}
    }
    if exports == 1 && managers == 1 {container=ancestor;break}
    seen.append(ancestor);current=ancestor
  }
  guard let menu=container else {throw Refusal(code:"adapter-refused")}
  try check(owned(selected));current=selected;seen=[selected]
  for _ in 0..<4 {
    let (ancestor,_)=try edge(current,seen)
    if equal(ancestor,menu) {return menu}
    seen.append(ancestor);current=ancestor
  }
  throw Refusal(code:"adapter-refused")
}
// Ownership/content resolver is the existing strict UI32 resolver. The extra
// focus membership port must use its current ordered content certificate.
final class F03OwnedOpacity {
  let pinned:OwnedInitialWindow
  let kernel=F03InputKernel()
  let fixtureKind:String
  private var menuOpened=false
  init(_ pinned:OwnedInitialWindow, _ fixtureKind:String?) throws {
    try check(fixtureKind == "f03-initial" || fixtureKind == "f03-original")
    self.pinned=pinned;self.fixtureKind=fixtureKind!
  }
  func perform(_ command:String, option:String?, value:String?, confirmedFixtureKind:String?,
    resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void,
    contentMember:(AXUIElement) throws -> Void, beforeMenuPress:() -> Void = {}) throws -> [String:Any] {
    try check(!kernel.failed)
    do {
      let input=try f03Input(command,option,value)
      // The current confirmed admission comes from Root's retained fixture
      // state, never from an option/value or merely the startup environment.
      if input.staticStage {
        try check(fixtureKind == "f03-initial" && confirmedFixtureKind == "f03-initial")
      }
      try check(!menuOpened || input.menuEntry)
      try check(!input.menuEntry || menuOpened)
      let rule=f03Rule(input), selected=try resolve(rule)
      let start=DispatchTime.now().uptimeNanoseconds
      try check(start <= UInt64.max-3_000_000_000)
      let deadline=start+3_000_000_000
      func identity() throws {
        try check(DispatchTime.now().uptimeNanoseconds < deadline)
        try verifyOwnedWindow(pinned)
        try check(DispatchTime.now().uptimeNanoseconds < deadline)
      }
      func read(_ node:AXUIElement, _ key:String) throws -> CFTypeRef? {
        try identity();try check(owns(node,pinned.pid))
        try check(AXUIElementSetMessagingTimeout(node,0.03) == .success)
        var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
        try check(status == .success);try check(owns(node,pinned.pid));try identity();return raw
      }
      func staticScope() throws {
        guard input.staticStage else {return}
        let frame=try resolve(f03Rule(.frame0));try visible(frame)
        let raw=try read(frame,kAXValueAttribute)
        guard let displayed=(raw as? String) ?? (raw as? NSNumber)?.stringValue else {throw Refusal(code:"adapter-refused")}
        try f03StaticInputProof(input,originalKind:fixtureKind,confirmedKind:confirmedFixtureKind,displayedFrame:displayed)
      }
      func current() throws {
        try identity();try check(owns(selected,pinned.pid))
        try check(CFEqual(try resolve(rule),selected));try visible(selected)
        try staticScope()
        guard let window=axElement(try read(pinned.application,kAXFocusedWindowAttribute)),
          owns(window,pinned.pid),CFEqual(window,pinned.main) else {throw Refusal(code:"adapter-refused")}
        if input.menuEntry {
          // Reuse the existing sibling-signature proof for the same app menu.
          let witness=try resolve(ui32Rule("settings.menu.entry",nil));try visible(witness)
          let menu=try f03MenuBinding(selected,witness,equal:{CFEqual($0,$1)},owned:{owns($0,pinned.pid)},
            parent:{node in
              guard let parent=axElement(try read(node,kAXParentAttribute)) else {throw Refusal(code:"adapter-refused")}
              return parent
            },children:{node in
              guard let rows=try read(node,kAXChildrenAttribute) as? [AXUIElement] else {throw Refusal(code:"adapter-refused")}
              return rows
            },identifier:{try ui32DOMIdentifiers($0,pinned.pid).resolved})
          try contentMember(menu);try check(CFEqual(try resolve(rule),selected))
        }
      }
      func focused(_ same:Bool) throws {
        try identity()
        guard let window=axElement(try read(pinned.application,kAXFocusedWindowAttribute)),
          owns(window,pinned.pid),CFEqual(window,pinned.main),
          let field=axElement(try read(pinned.application,kAXFocusedUIElementAttribute)),owns(field,pinned.pid)
        else {throw Refusal(code:"adapter-refused")}
        if same {try check(CFEqual(field,selected))}
        try contentMember(field);try staticScope();try identity()
      }
      func writable(_ key:String) throws {
        try current();var settable=DarwinBoolean(false)
        let status=AXUIElementIsAttributeSettable(selected,key as CFString,&settable)
        try check(status == .success && settable.boolValue);try check(owns(selected,pinned.pid))
      }
      func effect(_ name:String, before:() throws -> Void, write:() throws -> Void,
        after:() throws -> Void) throws {
        try kernel.effect(name,identity:identity,before:before,write:write,after:after)
      }
      func focus() throws {
        try effect("focus",before:{try writable(kAXFocusedAttribute)},write:{
          try check(AXUIElementSetAttributeValue(selected,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
        },after:{try current();try focused(true)})
      }
      func keys(_ code:CGKeyCode, _ flags:CGEventFlags, _ text:String,
        before:() throws -> Void, after:() throws -> Void) throws {
        guard let source=CGEventSource(stateID:.privateState),
          let down=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:true),
          let up=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:false)
        else {throw Refusal(code:"adapter-refused")}
        let units=Array(text.utf16)
        for event in [down,up] {
          event.flags=flags
          if !units.isEmpty {units.withUnsafeBufferPointer {event.keyboardSetUnicodeString(stringLength:units.count,unicodeString:$0.baseAddress!)}}
        }
        try effect("key-down",before:before,write:{down.postToPid(pinned.pid)},after:after)
        try effect("key-up",before:after,write:{up.postToPid(pinned.pid)},after:after)
      }
      if input.field {
        // Current number input and label are proved by resolve; AXValue does
        // not by itself establish DOM change or a published native revision.
        try current();let old=try read(selected,kAXValueAttribute)
        guard let prior=(old as? String) ?? (old as? NSNumber)?.stringValue else {throw Refusal(code:"adapter-refused")}
        if input == .opacity40 {try check(prior == "25")}
        if input == .opacity60 {try check(prior == "40")}
        try focus()
        let wanted=input.value!
        try effect("value",before:{
          try current();try focused(true);try writable(kAXValueAttribute)
          let raw=try read(selected,kAXValueAttribute)
          try check(((raw as? String) ?? (raw as? NSNumber)?.stringValue) == prior)
        },write:{
          try check(AXUIElementSetAttributeValue(selected,kAXValueAttribute as CFString,wanted as CFString) == .success)
        },after:{
          try current();try focused(true)
          let raw=try read(selected,kAXValueAttribute)
          try check(((raw as? String) ?? (raw as? NSNumber)?.stringValue) == wanted)
        })
        // Enter's ordinary onKeyDown blurs INPUT, dispatching its change.
        // After down, the former field may be retired: only current content
        // and focused-window proofs apply, never a stale-field re-resolution.
        try keys(36,[],"\r",before:{try current();try focused(true)},after:{try focused(false)})
      } else if input == .undo || input == .redo {
        // Focus a real navigation button WITHOUT pressing/changing frame.
        // Otherwise Cmd-Z inside INPUT would undo browser text only.
        try focus()
        try keys(6,input == .redo ? [.maskCommand,.maskShift] : .maskCommand,"z",
          before:{try current();try focused(true)},after:{try current();try focused(true)})
      } else {
        // Open is the actual menu entry; a shortcut label does not prove a
        // keyboard handler. Menu entries require the preceding menu action.
        try effect("press",before:{
          try current();var names:CFArray?
          try check(AXUIElementCopyActionNames(selected,&names) == .success)
          try check((names as? [String] ?? []).contains(kAXPressAction as String))
        },write:{beforeMenuPress();try check(AXUIElementPerformAction(selected,kAXPressAction as CFString) == .success)},
          after:{try verifyOwnedWindow(pinned)})
        menuOpened=input == .menu
      }
      return ["command":command,"inputAttempted":true,"effectAttempts":kernel.attempted,"effectAcknowledgements":kernel.acknowledged,
        "nativePickerMayOpen":input.picker,"retry":false,
        "note":"Input route only; native receipt, DOM change, save/open completion and pixels require independent observations."]
    } catch {
      kernel.stop()
      throw ActionRefusal(diagnostics:["purpose":"F03","command":command,"attempted":kernel.attempted,
        "acknowledged":kernel.acknowledged,"underlying":ui32ActionCause(error),"retry":false])
    }
  }
}
