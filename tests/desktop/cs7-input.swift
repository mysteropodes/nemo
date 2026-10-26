// Shared input mechanics. Semantic targets and owner proofs remain in each driver.
// Acknowledged focus is not fresh focus; a posted event is not product acceptance.
import Foundation
import ApplicationServices
import CoreGraphics

struct CS7InputFailure: Error {
  let cause:Error
  let downPosted:Bool, upPosted:Bool, cleanupAttempted:Bool, cleanupCompleted:Bool
  var diagnostics:[String:Any] {
    ["downPosted":downPosted,"upPosted":upPosted,"cleanupAttempted":cleanupAttempted,
      "cleanupCompleted":cleanupCompleted,"retry":false]
  }
}

// Poll observations only. Never repeat the focus request or swallow identity/read errors.
func cs7AwaitFocus(identity:() throws -> Void, read:() throws -> Bool,
  now:()->UInt64={DispatchTime.now().uptimeNanoseconds},
  pause:()->Void={Thread.sleep(forTimeInterval:0.01)}) throws {
  let start=now()
  try check(start <= UInt64.max-750_000_000)
  let deadline=start+750_000_000
  for _ in 0..<75 {
    try check(now() < deadline);try identity();try check(now() < deadline)
    let focused=try read()
    try identity();try check(now() < deadline)
    if focused {return}
    pause()
  }
  throw Refusal(code:"adapter-refused")
}

// On failure release only our own held input, addressed to the original process.
// The cleanup proof deliberately checks identity, not the focus that just changed.
// A replaced/exited owner refuses cleanup. No down/move is replayed; the run fails.
func cs7InputSequence(context:() throws -> Void, releaseIdentity:() throws -> Void,
  down:()->Void, moves:[()->Void]=[], up:()->Void) throws {
  var held=false, postedDown=false, postedUp=false, cleanupAttempted=false, cleanupCompleted=false
  do {
    try context();down();held=true;postedDown=true
    for move in moves {try context();move()}
    try context();up();held=false;postedUp=true
    try context()
  } catch {
    if held {
      cleanupAttempted=true
      do {try releaseIdentity();up();held=false;postedUp=true;cleanupCompleted=true}
      catch {cleanupCompleted=false}
    }
    throw CS7InputFailure(cause:error,downPosted:postedDown,upPosted:postedUp,
      cleanupAttempted:cleanupAttempted,cleanupCompleted:cleanupCompleted)
  }
}

func cs7InputSelfTest() throws -> Int {
  try cs7StartKeysSelfTest()
  var cases=0
  for readyAt in [0,1,4] {
    var clock:UInt64=0,reads=0,identities=0
    try cs7AwaitFocus(identity:{identities += 1},read:{reads += 1;return reads>readyAt},
      now:{clock},pause:{clock += 10_000_000})
    try check(reads == readyAt+1 && identities == reads*2);cases += 1
  }
  for fault in ["identity","read","deadline","never"] {
    var clock:UInt64=0,reads=0,refused=false
    do {
      try cs7AwaitFocus(identity:{if fault == "identity" {throw Refusal(code:"adapter-refused")}},
        read:{reads += 1;if fault == "read" {throw Refusal(code:"adapter-refused")};return false},
        now:{clock},pause:{clock += fault == "deadline" ? 750_000_000 : 10_000_000})
    } catch {refused=true}
    try check(refused && reads <= 75 && (fault != "identity" || reads == 0));cases += 1
  }
  // Fault every fence in a drag, including after up. Exercise real sequence code.
  for failure in 0...5 {
    for ownerAlive in [true,false] {
      var fences=0,down=0,move=0,up=0,releases=0,refused=false
      do {
        try cs7InputSequence(context:{fences += 1;if fences == failure {throw Refusal(code:"adapter-refused")}},
          releaseIdentity:{releases += 1;try check(ownerAlive)},down:{down += 1},
          moves:[{move += 1},{move += 1}],up:{up += 1})
      } catch let error as CS7InputFailure {
        refused=true
        try check(error.downPosted == (failure != 1))
        try check(error.cleanupAttempted == (2...4).contains(failure))
        try check(error.cleanupCompleted == ((2...4).contains(failure) && ownerAlive))
      }
      try check(refused == (failure != 0) && down <= 1 && up <= 1 && move <= 2)
      if (2...4).contains(failure) {try check(releases == 1 && up == (ownerAlive ? 1 : 0))}
      else {try check(releases == 0)}
      cases += 1
    }
  }
  return cases
}

// Mandatory shared route: production and static controls use the same three fences.
func ui32MotionKeyPair(_ focus:() throws -> Void, down:()->Void, up:()->Void,
  releaseIdentity:(() throws -> Void)?=nil) throws {
  if let releaseIdentity {
    try cs7InputSequence(context:focus,releaseIdentity:releaseIdentity,down:down,up:up)
  } else {
    try cs7InputSequence(context:focus,releaseIdentity:focus,down:down,up:up)
  }
}

func cs7MotionFocus(_ pinned:OwnedInitialWindow, _ node:AXUIElement, _ search:Bool, _ rule:UI32Rule,
  resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void,
  contentScope:(AXUIElement) throws -> Void) throws {
    try cs7AwaitFocus(identity:{
      try verifyOwnedWindow(pinned);try check(CFEqual(try resolve(rule),node));try visible(node)
    },read:{
      guard let window=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),owns(window,pinned.pid),
        let focused=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(focused,pinned.pid) else {
        throw Refusal(code:"adapter-refused")
      }
      return CFEqual(window,pinned.main) && CFEqual(focused,node)
    })
    try ui32MotionFocus(pinned,node,search,contentScope:contentScope)
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
    try cs7InputSequence(context:context,releaseIdentity:{try verifyOwnedWindow(pinned)},
      down:{down.postToPid(pinned.pid);downPosted=true},up:{up.postToPid(pinned.pid);upPosted=true})
    return ["buttonDownPosted":downPosted,"buttonUpPosted":upPosted,"note":"Named F06-B additive selection event pair only; verify actual native selection separately."]
  } catch {
    var facts:[String:Any]=["purpose":"UI32","command":"selection.toggle.B","buttonDownPosted":downPosted,"buttonUpPosted":upPosted,"retry":false]
    if let failure=error as? CS7InputFailure {facts["inputCleanup"]=failure.diagnostics}
    throw ActionRefusal(diagnostics:facts)
  }
}

// Fixed CS4 diagnostic from #2343. Never accepts caller-supplied keys or modifiers.
func cs7StartAdmission(_ command:String, entry:Bool, fixture:Bool, sync:Bool, f03:Bool,
  used:Bool, option:Any?, value:Any?) throws {
  try check(command == "cs4.start-keys" && !entry && !fixture && !sync && !f03 && !used && option == nil && value == nil)
}
func cs7StartSurface(_ classes:Set<String>) throws {
  try check(!classes.contains("hid") && !classes.contains("hidden"))
}
func cs7StartKeys(_ pinned:OwnedInitialWindow, resolve:(UI32Rule) throws -> AXUIElement,
  visible:(AXUIElement) throws -> Void) throws -> [String:Any] {
  let rule=UI32Rule(identifier:"start-open",region:"start-screen",labels:[],roles:["AXButton"],operation:"focus",values:[])
  let node=try resolve(rule);try visible(node)
  var cursor=node,screen:AXUIElement?,seen=[AXUIElement]()
  for _ in 0..<20 {
    try check(owns(cursor,pinned.pid) && !seen.contains {CFEqual($0,cursor)});seen.append(cursor)
    if try ui32DOMIdentifiers(cursor,pinned.pid).resolved == "start-screen" {screen=cursor;break}
    guard let parent=axElement(attr(cursor,kAXParentAttribute)),owns(parent,pinned.pid),
      let children=attr(parent,kAXChildrenAttribute) as? [AXUIElement],children.contains(where:{CFEqual($0,cursor)}) else {throw Refusal(code:"child-owner")}
    cursor=parent
  }
  guard let screen else {throw Refusal(code:"adapter-refused")}
  func owner() throws {
    try verifyOwnedWindow(pinned);try check(owns(node,pinned.pid) && owns(screen,pinned.pid))
    try cs7StartSurface(ui32EntryDOMClasses(screen,pinned.pid));try visible(node)
  }
  func focused() throws -> Bool {
    guard let window=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),owns(window,pinned.pid),
      let focus=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(focus,pinned.pid) else {return false}
    return CFEqual(window,pinned.main) && CFEqual(focus,node)
  }
  try owner();var writable=DarwinBoolean(false)
  try check(AXUIElementIsAttributeSettable(node,kAXFocusedAttribute as CFString,&writable) == .success && writable.boolValue)
  try check(AXUIElementSetAttributeValue(node,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
  try cs7AwaitFocus(identity:owner,read:focused)
  let keys:[(String,CGKeyCode,CGEventFlags)]=[("Right",124,[]),("Left",123,[]),
    ("Cmd+Z",6,.maskCommand),("Shift+Cmd+Z",6,[.maskCommand,.maskShift]),
    ("V",9,[]),("B",11,[]),("P",35,[]),("R",15,[]),("Y",16,[]),("E",14,[])]
  var pairs=[String]()
  do {
    for (name,code,flags) in keys {
      guard let source=CGEventSource(stateID:.privateState),
        let down=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:true),
        let up=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:false) else {throw Refusal(code:"adapter-refused")}
      down.flags=flags;up.flags=[]
      down.setIntegerValueField(.keyboardEventAutorepeat,value:0);up.setIntegerValueField(.keyboardEventAutorepeat,value:0)
      try cs7InputSequence(context:{try owner();try check(try focused())},
        releaseIdentity:{try verifyOwnedWindow(pinned)},down:{down.postToPid(pinned.pid)},
        up:{up.postToPid(pinned.pid)})
      pairs.append(name)
    }
    try check(CFEqual(try resolve(rule),node))
    return ["postedPairs":pairs,"focusAcknowledged":true,"ownedReleasesPosted":true,
      "note":"Fixed PID key down/up posting and owner/focus proofs; not application acknowledgement or feature acceptance."]
  } catch {
    var facts:[String:Any]=["purpose":"UI32","command":"cs4.start-keys","postedPairs":pairs,"retry":false]
    if let failure=error as? CS7InputFailure {facts["inputCleanup"]=failure.diagnostics}
    throw ActionRefusal(diagnostics:facts)
  }
}
func cs7StartKeysSelfTest() throws {
  try cs7StartAdmission("cs4.start-keys",entry:false,fixture:false,sync:false,f03:false,used:false,option:nil,value:nil)
  for fault in ["command","entry","fixture","sync","f03","used","option","value"] {
    var refused=false
    do {try cs7StartAdmission(fault == "command" ? "cs4.arbitrary" : "cs4.start-keys",entry:fault == "entry",
      fixture:fault == "fixture",sync:fault == "sync",f03:fault == "f03",used:fault == "used",
      option:fault == "option" ? "Left" : nil,value:fault == "value" ? "Right" : nil)} catch {refused=true}
    try check(refused)
  }
  try cs7StartSurface([])
  for classes in [Set(["hid"]),Set(["hidden"]),Set(["hid","visible"])] {
    var refused=false;do {try cs7StartSurface(classes)} catch {refused=true};try check(refused)
  }
}
