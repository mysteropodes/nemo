// Closed profile test-input focus. No typing, generic setter, fallback or retry.
import Foundation
import ApplicationServices

func ui32ProfileField(_ command:String) -> String? {
  switch command {
  case "profile.name": return "profile-name"
  case "profile.email": return "profile-email"
  default: return nil
  }
}

// Production and negative controls share this exact gate before the existing AXValue action.
func ui32ProfileFocusProof<Node>(_ command:String, _ field:Node, _ main:Node,
  identity:() throws -> Void, resolve:() throws -> Node, owned:(Node)->Bool,
  equal:(Node,Node)->Bool, properties:(Node) throws -> (role:String,id:String),
  focusable:(Node) throws -> Bool, focus:(Node) throws -> Void,
  focusedWindow:() throws -> Node, focusedField:() throws -> Node,
  attempted:()->Void, acknowledged:()->Void, now:()->UInt64) throws -> Bool {
  guard let expected=ui32ProfileField(command) else {return false}
  let started=now()
  try check(started <= UInt64.max-750_000_000)
  let deadline=started+750_000_000
  func fence() throws {
    try check(now() < deadline);try identity();try check(now() < deadline)
  }
  func target() throws {
    try fence();try check(owned(field) && owned(main))
    let current=try resolve()
    try check(owned(current) && equal(current,field) && owned(field))
    let value=try properties(field)
    try check(owned(field) && value.role == "AXTextField" && value.id == expected)
    try fence()
  }
  try target()
  try check(try focusable(field));try check(owned(field));try fence()
  attempted();try focus(field);acknowledged()
  try target()
  let window=try focusedWindow()
  try check(owned(window) && equal(window,main));try fence()
  let current=try focusedField()
  try check(owned(current) && equal(current,field));try fence()
  return true
}

func ui32ProfileFocus(_ command:String, _ field:AXUIElement, _ pinned:OwnedInitialWindow,
  _ resolve:() throws -> AXUIElement, _ visible:(AXUIElement) throws -> Void,
  _ attempt:()->Void, _ facts:inout [String:Any]) throws {
  guard let expected=ui32ProfileField(command) else {return}
  var focusAttempted=false,focusAcknowledged=false
  func current(_ key:String) throws -> AXUIElement {
    try verifyOwnedWindow(pinned)
    try check(owns(pinned.application,pinned.pid) &&
      AXUIElementSetMessagingTimeout(pinned.application,0.03) == .success && owns(pinned.application,pinned.pid))
    var raw:CFTypeRef?
    let status=AXUIElementCopyAttributeValue(pinned.application,key as CFString,&raw)
    try check(owns(pinned.application,pinned.pid))
    guard status == .success,let node=axElement(raw),owns(node,pinned.pid) else {
      throw Refusal(code:"adapter-refused")
    }
    return node
  }
  do {
    let accepted=try ui32ProfileFocusProof(command,field,pinned.main,
      identity:{try verifyOwnedWindow(pinned)},resolve:{
        let node=try resolve();try visible(node);return node
      },owned:{owns($0,pinned.pid)},equal:{CFEqual($0,$1)},properties:{node in
        try check(owns(node,pinned.pid))
        let id=try ui32DOMIdentifiers(node,pinned.pid).resolved
        guard let role=attr(node,kAXRoleAttribute) as? String,let id else {throw Refusal(code:"adapter-refused")}
        try check(owns(node,pinned.pid));return (role,id)
      },focusable:{node in
        try check(owns(node,pinned.pid));var writable=DarwinBoolean(false)
        let status=AXUIElementIsAttributeSettable(node,kAXFocusedAttribute as CFString,&writable)
        try check(owns(node,pinned.pid));return status == .success && writable.boolValue
      },focus:{node in
        try check(owns(node,pinned.pid))
        let status=AXUIElementSetAttributeValue(node,kAXFocusedAttribute as CFString,kCFBooleanTrue)
        if status == .success {focusAcknowledged=true}
        try check(owns(node,pinned.pid));try check(status == .success)
      },focusedWindow:{try current(kAXFocusedWindowAttribute)},focusedField:{try current(kAXFocusedUIElementAttribute)},
      attempted:{focusAttempted=true;attempt()},acknowledged:{focusAcknowledged=true},
      now:{DispatchTime.now().uptimeNanoseconds})
    try check(accepted)
    facts["profileFocus"]=["field":expected,"attempted":focusAttempted,"acknowledged":focusAcknowledged,
      "sameFocusedField":true,"sameMainWindow":true,"note":"Focus proof only, not DOM input delivery or persistence."]
  } catch {
    throw ActionRefusal(diagnostics:["stage":"profile-focus","field":expected,"focusAttempted":focusAttempted,
      "focusAcknowledged":focusAcknowledged,"underlying":ui32ActionCause(error),"retry":false])
  }
}

func ui32ProfileFocusSelfTest() throws -> Int {
  let faults=["none","foreign-target","foreign-main","wrong-target","wrong-id","wrong-role","not-focusable",
    "identity-before","identity-after","focus-error","post-target","foreign-window","wrong-window",
    "foreign-focus","wrong-focus","deadline-before","deadline-after"]
  for command in ["profile.name","profile.email"] {
    for fault in faults {
      var clock:UInt64=0,writes=0,attempts=0,acks=0,values=0,reads=0,focused=30,refused=false
      let expected=ui32ProfileField(command)!
      do {
        let accepted=try ui32ProfileFocusProof(command,10,20,identity:{
          if fault == "identity-before" || (fault == "identity-after" && writes > 0) {throw Refusal(code:"child-owner")}
        },resolve:{fault == "wrong-target" || (fault == "post-target" && writes > 0) ? 11 : 10},owned:{node in
          node != 90 && !(fault == "foreign-target" && node == 10) && !(fault == "foreign-main" && node == 20)
        },equal:==,properties:{node in
          try check(node == 10);reads += 1
          return (fault == "wrong-role" ? "AXTextArea" : "AXTextField",fault == "wrong-id" ? "np-name" : expected)
        },focusable:{_ in
          if fault == "deadline-before" {clock=750_000_000};return fault != "not-focusable"
        },focus:{_ in
          writes += 1
          if fault == "focus-error" {throw Refusal(code:"adapter-refused")}
          focused=10
          if fault == "deadline-after" {clock=750_000_000}
        },focusedWindow:{fault == "foreign-window" ? 90 : fault == "wrong-window" ? 21 : 20},
          focusedField:{fault == "foreign-focus" ? 90 : fault == "wrong-focus" ? 11 : focused},
          attempted:{attempts += 1},acknowledged:{acks += 1},now:{clock})
        try check(accepted && focused == 10);values += 1
      }catch{refused=true}
      let preFocusFailure=["foreign-target","foreign-main","wrong-target","wrong-id","wrong-role",
        "not-focusable","identity-before","deadline-before"].contains(fault)
      try check(refused == (fault != "none") && values == (fault == "none" ? 1 : 0))
      try check(writes == (preFocusFailure ? 0 : 1) && attempts == writes)
      try check(acks == (preFocusFailure || fault == "focus-error" ? 0 : 1))
      if ["foreign-target","foreign-main","wrong-target","identity-before"].contains(fault) {try check(reads == 0)}
    }
  }
  for command in ["profile.role","motion.search","grid.step.value","profile.name.restore","arbitrary.set"] {
    var calls=0
    let accepted=try ui32ProfileFocusProof(command,10,20,identity:{calls += 1},resolve:{calls += 1;return 10},
      owned:{_ in calls += 1;return true},equal:==,properties:{_ in calls += 1;return ("AXTextField","profile-name")},
      focusable:{_ in calls += 1;return true},focus:{_ in calls += 1},focusedWindow:{calls += 1;return 20},
      focusedField:{calls += 1;return 10},attempted:{calls += 1},acknowledged:{calls += 1},now:{calls += 1;return 0})
    try check(!accepted && calls == 0)
  }
  return faults.count*2+5
}
