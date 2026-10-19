// New entry's first phase2 observation only; preserves the original ordered certificate.
import Foundation

func ui32Phase2Read<T>(_ enabled:Bool, _ stage:Int, _ captured:Int, _ settings:Bool, _ used:inout Bool,
  now:()->UInt64 = {DispatchTime.now().uptimeNanoseconds}, identity:() throws -> Void,
  read:(UInt64?) throws -> T) throws -> (value:T,proof:[String:Any]?) {
  // Other observations retain exactly one original identity/read path and no new budget.
  guard enabled && stage == 2 && captured == 0 && !settings && !used else {
    try identity();return (try read(nil),nil)
  }
  used=true
  let start=now()
  let (deadline,overflow)=start.addingReportingOverflow(750_000_000)
  try check(!overflow)
  var last=start,reads=0,first:[String:Any]?
  func tick() throws {
    let current=now();try check(current >= last && current < deadline);last=current
  }
  func failure(_ error:Error) -> ActionRefusal {
    ActionRefusal(diagnostics:["purpose":"UI32","stage":"new-phase2-observe-readiness",
      "readAttempts":reads,"attemptSlots":reads,"firstReadFailure":first ?? ui32ActionCause(error),
      "lastReadFailure":ui32ActionCause(error),"inputRetry":false,
      "note":"Original ordered certificate required; no further content read."])
  }
  for slot in 1...2 {
    do {
      try tick();try identity();try tick();reads += 1
      let outcome:Result<T,Error>
      do {outcome = .success(try read(deadline))}catch{outcome = .failure(error)}
      // Even failed probes must finish the current identity and time fences.
      if case .failure(let error)=outcome,first == nil {first=ui32ActionCause(error)}
      try tick();try identity();try tick()
      switch outcome {
      case .success(let value):
        var proof:[String:Any]=["stage":"new-phase2-observe-readiness","readAttempts":reads,
          "attemptSlots":slot,"completeOriginalOrderedCertificate":true,"inputRetry":false,
          "startNanoseconds":start,"lastNanoseconds":last,"deadlineNanoseconds":deadline,
          "scope":"One observation; later full certificate only, no paint or cause claim"]
        if let first {proof["firstReadFailure"]=first};return (value,proof)
      case .failure(let error):
        // Exact finite existing producer schema only. Count, parent, owner, role,
        // root, modal, incomplete/unknown errors and second failures never settle.
        guard slot == 1,ui32NewContentComparison(error),
          (error as? ActionRefusal)?.diagnostics["comparison"] as? String == "node-order" else {throw error}
      }
    }catch{throw failure(error)}
  }
  throw Refusal(code:"adapter-refused")
}

func ui32Phase2ReadSelfTest() throws {
  func certificate(_ order:[Int],_ deadline:UInt64) throws -> UI32ContentCertificate<Int> {
    try ui32ContentCertificate(0,pid:7,retained:1,deadline:deadline,now:{0},equal:==,
      owner:{_ in (0,true,7)},prepare:{_ in true},traits:{($0 == 0 ? "AXWindow" : $0 == 1 ? "AXWebArea" : "AXGroup",false)},
      children:{(0,$0 == 0 ? [1] : $0 == 1 ? [2,3] : [])},parent:{(0,$0 == 1 ? 0 : 1)},strict:{_,_ in order})
  }
  var actual:[String:Any]=[:]
  do {_ = try certificate([1,3,2],10)}catch let error as ActionRefusal {actual=error.diagnostics}
  try check(actual["comparison"] as? String == "node-order" && ui32NewContentComparison(ActionRefusal(diagnostics:actual)))
  let faults=["first-pass","settled","second-failure","count","owner","parent","extra","incomplete","retained","bad-type",
    "before-identity","after-failed-identity","after-success-identity","before-budget","read-budget","after-budget","overflow"]
  for fault in faults {
    var used=false
    var clock:UInt64=fault == "overflow" ? UInt64.max : 0,reads=0,ids=0,published=false,refused=false,proof:[String:Any]?,seen=[UInt64?]()
    do {
      let result=try ui32Phase2Read(true,2,0,false,&used,now:{clock},identity:{
        ids += 1
        if fault == "before-identity" && ids == 1 || fault == "after-failed-identity" && ids == 2 || fault == "after-success-identity" && ids == 4 {throw Refusal(code:"identity-control")}
        if fault == "before-budget" && ids == 1 || fault == "after-budget" && ids == 2 {clock=750_000_000}
      },read:{ deadline in
        reads += 1;seen.append(deadline);try check(deadline == 750_000_000)
        if fault == "read-budget" {clock=750_000_000}
        if fault == "first-pass" || reads == 2 && fault != "second-failure" {return try certificate([1,2,3],deadline!)}
        var d=actual
        switch fault {
        case "count":d.removeValue(forKey:"mismatchIndex");d["comparison"]="count";d["strictContentCount"]=2
        case "owner":d=["stage":"content-scope","reason":"owner"]
        case "parent":d=["stage":"parent-edge","insideContent":true]
        case "extra":d["extra"]=true
        case "incomplete":d["fullStrictReadComplete"]=false
        case "retained":d["retainedWebAreaMatched"]=false
        case "bad-type":d["mismatchIndex"]=true
        default:break
        }
        throw ActionRefusal(diagnostics:d)
      })
      published=true;proof=result.proof;try check(ui32ContentMembers(result.value) == [1,2,3])
    }catch{refused=true}
    let success=["first-pass","settled"].contains(fault)
    try check(published == success && refused == !success)
    let two=["settled","second-failure","after-success-identity"].contains(fault)
    try check(reads == (fault == "overflow" || fault == "before-identity" || fault == "before-budget" ? 0 : two ? 2 : 1))
    try check(seen.allSatisfy{$0 == 750_000_000})
    if fault == "first-pass" {try check(proof?["readAttempts"] as? Int == 1 && proof?["firstReadFailure"] == nil)}
    if fault == "settled" {try check(proof?["readAttempts"] as? Int == 2 && proof?["firstReadFailure"] != nil && ids == 4)}
  }
  for mode in 0...4 {
    var used=mode == 4,reads=0,ids=0,clockCalls=0
    let result=try ui32Phase2Read(mode != 0,mode == 1 ? 3 : 2,mode == 2 ? 2 : 0,mode == 3,&used,
      now:{clockCalls += 1;return 0},identity:{ids += 1},read:{deadline in reads += 1;try check(deadline == nil);return 7})
    try check(result.value == 7 && result.proof == nil && reads == 1 && ids == 1 && clockCalls == 0)
  }
  var used=false,clock:UInt64=10,reads=0,refused=false
  do {let _:(value:Int,proof:[String:Any]?) = try ui32Phase2Read(true,2,0,false,&used,now:{clock},identity:{},read:{_ in reads += 1;clock=9;throw ActionRefusal(diagnostics:actual)})}catch{refused=true}
  try check(refused && reads == 1)
}
