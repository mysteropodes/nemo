// Shared input mechanics. Semantic targets and owner proofs remain in each driver.
// Acknowledged focus is not fresh focus; a posted event is not product acceptance.
import Foundation
import ApplicationServices

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
