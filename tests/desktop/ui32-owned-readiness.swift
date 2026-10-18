// Cohesive initial and posted-New owned readiness; no input or product authority.
import Foundation
import ApplicationServices

func ui32NewInvalidReference(_ error:Error) -> Bool {
  guard let refusal=error as? ActionRefusal else {return false}
  let d=refusal.diagnostics
  guard d["stage"] as? String == "parent-query",d["insideContent"] as? Bool == true,
    let t=d["contentTraversal"] as? [String:Any],t["walk"] as? String == "strict-content",
    t["phase"] as? String == "parent",t["queryError"] as? Int == Int(AXError.invalidUIElement.rawValue),
    t["metadataTruncated"] as? Bool == false,t["foreignRootCount"] as? Int == 0,
    t["declaredParentIndices"] as? [Int] == [0],t["depths"] as? [Int] == [1],
    t["reportedMatchesChild"] as? Bool == false,t["reportedMatchesDeclared"] as? Bool == false,
    let routes=t["routes"] as? [String],routes.count == 1,
    let route=routes.first,let ordinal=Int(route),ordinal >= 0,ordinal < 512,String(ordinal) == route,route.range(of:"^[0-9]{1,3}$",options:.regularExpression) != nil,
    let refs=t["references"] as? [String:[String:Any]],
    let root=refs["root"],let parent=refs["declaredParent"],let child=refs["child"],
    refs["reportedParent"]?["relation"] as? String == "unavailable",
    root["owner"] as? String == "owned",root["knownIndex"] as? Int == 0,root["routes"] as? [String] == [""],
    parent["owner"] as? String == "owned",parent["knownIndex"] as? Int == 0,parent["routes"] as? [String] == [""],
    child["owner"] as? String == "owned",let index=child["knownIndex"] as? Int,index > 0,index < 512,
    child["routes"] as? [String] == routes else {return false}
  return true
}
// Same-probe complete census/strict-walk disagreement; never an owner/root waiver.
func ui32NewContentComparison(_ error:Error) -> Bool {
  guard let d=(error as? ActionRefusal)?.diagnostics,let kind=d["comparison"] as? String else {return false}
  var keys:Set<String>=["stage","reason","comparison","forwardContentCount","strictContentCount","ownedWindowNodeCount","fullStrictReadComplete","retainedWebAreaMatched"]
  if kind == "node-order" {keys.insert("mismatchIndex")}
  guard Set(d.keys) == keys,d["stage"] as? String == "content-scope",d["reason"] as? String == "content-changed",
    let complete=d["fullStrictReadComplete"],type(of:complete) == Bool.self,complete as? Bool == true,
    let retained=d["retainedWebAreaMatched"],type(of:retained) == Bool.self,retained as? Bool == true else {return false}
  func integer(_ key:String) -> Int? {guard let raw=d[key],type(of:raw) == Int.self else {return nil};return raw as? Int}
  guard let forward=integer("forwardContentCount"),let strict=integer("strictContentCount"),let owned=integer("ownedWindowNodeCount"),
    forward > 0,strict > 0,owned > forward,owned <= 512,strict <= 512 else {return false}
  if kind == "count" {return forward != strict}
  guard kind == "node-order",forward == strict,let offset=integer("mismatchIndex") else {return false}
  return offset >= 0 && offset < forward
}
// Immediate posted New/Create only: one fresh complete read, same absolute budget.
func ui32NewPostCreateRead(_ command:String, _ enabled:Bool, _ attempted:Bool, _ posted:Bool, _ stage:Int,
  now:()->UInt64, identity:() throws -> Void, read:(UInt64) throws -> Void) throws -> [String:Any] {
  try check(command == "entry.create.default" && enabled && attempted && posted && stage == 2)
  let began=now();try check(began <= UInt64.max-750_000_000);let deadline=began+750_000_000
  var first:[String:Any]?,reads=0
  for slot in 1...2 {
    do {
      try check(now() < deadline);try identity();try check(now() < deadline)
      reads += 1;try read(deadline);try identity();try check(now() < deadline)
      var proof:[String:Any]=["completeStrictContent":true,"readAttempts":reads,"attemptSlots":slot,
        "oneCreateActionOnly":true,"scope":"later complete read; no cause or paint claim"]
      if let first {proof["firstReadFailure"]=first};return proof
    } catch {
      let cause=ui32ActionCause(error)
      if slot == 1 && reads == 1 && (ui32NewInvalidReference(error) || ui32NewContentComparison(error)) && now() < deadline {first=cause;continue}
      throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"new-postcreate-content","readAttempts":reads,
        "attemptSlots":slot,"firstReadFailure":first ?? cause,"lastReadFailure":cause,
        "note":"No action retry or further content read; retain both outcomes."])
    }
  }
  throw Refusal(code:"adapter-refused")
}

func ui32NewPostCreateReadSelfTest() throws {
  let root:[String:Any]=["owner":"owned","knownIndex":0,"routes":[""]]
  let facts:[String:Any]=["insideContent":true,"stage":"parent-query","contentTraversal":[
    "walk":"strict-content","phase":"parent","queryError":Int(AXError.invalidUIElement.rawValue),
    "metadataTruncated":false,"foreignRootCount":0,"declaredParentIndices":[0],"depths":[1],"routes":["76"],
    "reportedMatchesChild":false,"reportedMatchesDeclared":false,"references":["root":root,"declaredParent":root,
      "child":["owner":"owned","knownIndex":77,"routes":["76"]],"reportedParent":["relation":"unavailable"]]]]
  try check(ui32NewInvalidReference(ActionRefusal(diagnostics:facts)))
  for fault in ["stage","inside","query","walk","phase","truncated","foreign","depth","route","leading-zero","parent","owner","reported"] {
    var changed=facts,t=facts["contentTraversal"] as! [String:Any]
    switch fault {
    case "stage":changed["stage"]="parent-edge"
    case "inside":changed["insideContent"]=false
    case "query":t["queryError"]=Int(AXError.noValue.rawValue)
    case "walk":t["walk"]="attached-candidate-certificate"
    case "phase":t["phase"]="children"
    case "truncated":t["metadataTruncated"]=true
    case "foreign":t["foreignRootCount"]=1
    case "depth":t["depths"]=[2]
    case "route":t["routes"]=["0.1"]
    case "leading-zero":
      t["routes"]=["076"];var refs=t["references"] as! [String:[String:Any]];refs["child"]?["routes"]=["076"];t["references"]=refs
    case "parent":t["declaredParentIndices"]=[1]
    default:
      var refs=t["references"] as! [String:[String:Any]]
      if fault == "owner" {refs["child"]?["owner"]="foreign"} else {refs["reportedParent"]=["knownIndex":0]}
      t["references"]=refs
    }
    changed["contentTraversal"]=t;try check(!ui32NewInvalidReference(ActionRefusal(diagnostics:changed)))
  }
  for scenario in ["first-pass","second-pass","second-fail","other-fail","deadline","identity","wrong-phase"] {
    var clock:UInt64=0,reads=0,identities=0,deadlines=[UInt64](),refused=false,proof=[String:Any]()
    do {
      proof=try ui32NewPostCreateRead("entry.create.default",true,true,true,scenario == "wrong-phase" ? 1 : 2,now:{clock},identity:{
        identities += 1;if scenario == "identity" && identities == 2 {throw Refusal(code:"child-owner")}
      },read:{deadline in
        reads += 1;deadlines.append(deadline)
        if scenario == "first-pass" || (scenario == "second-pass" && reads == 2) {return}
        if scenario == "deadline" {clock=deadline}
        if scenario == "other-fail" {throw Refusal(code:"child-owner")}
        throw ActionRefusal(diagnostics:facts)
      })
    }catch{refused=true}
    try check(refused == !["first-pass","second-pass"].contains(scenario))
    let expected=["second-pass","second-fail"] .contains(scenario) ? 2 : scenario == "wrong-phase" ? 0 : 1
    try check(reads == expected && Set(deadlines).count <= 1)
    if scenario == "second-pass" {try check(proof["readAttempts"] as? Int == 2 && proof["firstReadFailure"] != nil)}
    if scenario == "first-pass" {try check(proof["firstReadFailure"] == nil)}
  }
  try ui32NewContentComparisonSelfTest()
}

// Actual certificate producer feeds the actual finite post-Create reader.
func ui32NewContentComparisonSelfTest() throws {
  for kind in ["count","node-order"] {
    var produced:Error?
    do {let _:UI32ContentCertificate<Int>=try ui32ContentCertificate(0,pid:7,retained:1,deadline:10,
      now:{0},equal:==,owner:{_ in (0,true,7)},prepare:{_ in true},traits:{($0 == 0 ? "AXWindow" : $0 == 1 ? "AXWebArea" : "AXGroup",false)},
      children:{(0,$0 == 0 ? [1] : $0 == 1 ? [2] : [])},parent:{(0,$0 == 1 ? 0 : 1)},strict:{_,_ in kind == "count" ? [1] : [2,1]})
    }catch{produced=error}
    guard let produced,let source=(produced as? ActionRefusal)?.diagnostics else {throw Refusal(code:"adapter-refused")}
    try check(ui32NewContentComparison(produced) && source["comparison"] as? String == kind)
    for fault in ["none","second-fail","deadline","identity","phase","command","enabled","attempted","posted","stage","reason","kind","retained","complete","forward","strict","owned","type","extra","missing"] {
      var facts=source,clock:UInt64=0,reads=0,ids=0,deadlines=[UInt64](),refused=false,answer=[String:Any]()
      switch fault {
      case "stage":facts["stage"]="other"
      case "reason":facts["reason"]="webarea-replaced"
      case "kind":facts["comparison"]="unknown"
      case "retained":facts["retainedWebAreaMatched"]=false
      case "complete":facts["fullStrictReadComplete"]=false
      case "forward":facts["forwardContentCount"]=0
      case "strict":facts["strictContentCount"]=0
      case "owned":facts["ownedWindowNodeCount"]=512+1
      case "type":facts["forwardContentCount"]=true
      case "extra":facts["unknown"]=true
      case "missing":facts.removeValue(forKey:"comparison")
      default:break
      }
      do {answer=try ui32NewPostCreateRead(fault == "command" ? "observe" : "entry.create.default",fault != "enabled",fault != "attempted",fault != "posted",fault == "phase" ? 1 : 2,
        now:{clock},identity:{ids += 1;if fault == "identity" && ids == 2 {throw Refusal(code:"owner-control")}},read:{deadline in
          reads += 1;deadlines.append(deadline)
          if reads == 2 && fault != "second-fail" {return}
          if fault == "deadline" {clock=deadline};throw ActionRefusal(diagnostics:facts)
        })}catch{refused=true}
      try check(refused == (fault != "none"))
      let blocked=["phase","command","enabled","attempted","posted"].contains(fault)
      let expected=blocked ? 0 : ["none","second-fail"].contains(fault) ? 2 : 1
      try check(reads == expected && Set(deadlines).count <= 1 && deadlines.allSatisfy{$0 == 750_000_000})
      if fault == "none" {try check(answer["readAttempts"] as? Int == 2 && answer["oneCreateActionOnly"] as? Bool == true && answer["firstReadFailure"] != nil)}
    }
    if kind == "node-order" {for offset in [-1,2] {var d=source;d["mismatchIndex"]=offset;try check(!ui32NewContentComparison(ActionRefusal(diagnostics:d)))}}
    else {var d=source;d["strictContentCount"]=d["forwardContentCount"];try check(!ui32NewContentComparison(ActionRefusal(diagnostics:d)))}
  }
  try check(!ui32NewContentComparison(ActionRefusal(diagnostics:["stage":"content-scope","reason":"content-changed"])))
}

// First observation only; every success still comes from the existing FULL strict read.
func ui32InitialOwnedReadiness<T>(first:Bool, now:()->UInt64, identity:() throws -> Void,
  pause:(UInt64)->Void, read:(UInt64) throws -> T) throws -> (value:T,proof:[String:Any]) {
  var attempts=0,waits=0,firstFailure:[String:Any]?,lastFailure:[String:Any]?,start:UInt64=0,last:UInt64=0,end:UInt64=0
  func proof(_ reason:String) -> [String:Any] {
    ["stage":"initial-owned-readiness","reason":reason,"attempts":attempts,"waits":waits,
      "firstFailure":(firstFailure as Any?) ?? NSNull(),"lastFailure":(lastFailure as Any?) ?? NSNull(),
      "startNanoseconds":start,"lastNanoseconds":last,"deadlineNanoseconds":end,"fullStrictReadRequired":true]
  }
  func failure(_ reason:String, _ cause:Error? = nil) -> ActionRefusal {
    var facts=proof(reason);if let cause {facts["underlying"]=ui32ActionCause(cause)}
    return ActionRefusal(diagnostics:facts)
  }
  func tick() throws -> UInt64 {
    let value=now();guard value >= last else {throw failure("clock-regression")};last=value;return value
  }
  func retryable(_ error:Error) -> Bool {
    guard let facts=(error as? ActionRefusal)?.diagnostics,
      Set(facts.keys) == ["stage","reason","webAreaCount","ownedWindowNodeCount","roleCounts","nativeModalCount"],
      facts["stage"] as? String == "content-scope",facts["reason"] as? String == "webarea-count",
      let web=facts["webAreaCount"],type(of:web) == Int.self,web as? Int == 0,
      let count=facts["ownedWindowNodeCount"],type(of:count) == Int.self,count as? Int == 6,
      let modal=facts["nativeModalCount"],type(of:modal) == Int.self,modal as? Int == 0,
      let rawRoles=facts["roleCounts"] as? [String:Any],rawRoles.values.allSatisfy({type(of:$0) == Int.self}),
      let roles=facts["roleCounts"] as? [String:Int] else {return false}
    return roles == ["AXWebArea":0,"AXGroup":2,"AXScrollArea":0,"AXSplitGroup":0,"AXButton":3,"AXWindow":1,"other":0]
  }
  guard first else {throw failure("not-first-observation")}
  start=now();last=start
  let (deadline,overflow)=start.addingReportingOverflow(5_000_000_000)
  guard !overflow else {throw failure("deadline-overflow")};end=deadline
  while attempts < 64 {
    guard try tick() < end else {throw failure("deadline")}
    do {try identity()}catch{throw failure("identity-before",error)}
    let before=try tick();guard before < end else {throw failure("deadline")}
    let probeDeadline=before+min(750_000_000,end-before)
    attempts += 1
    let outcome:Result<T,Error>
    do {outcome = .success(try read(probeDeadline))}catch{outcome = .failure(error)}
    if case .failure(let error)=outcome {
      let facts=ui32ActionCause(error);if firstFailure == nil {firstFailure=facts};lastFailure=facts
    }
    guard try tick() < probeDeadline else {throw failure("probe-deadline")}
    do {try identity()}catch{throw failure("identity-after",error)}
    let after=try tick();guard after < probeDeadline else {throw failure("probe-deadline")}
    switch outcome {
    case .success(let value):return (value,proof("full-strict-read"))
    case .failure(let error):
      guard retryable(error) else {throw failure("nonretryable",error)}
    }
    guard attempts < 64 else {throw failure("max-attempts")}
    let waiting=try tick();guard waiting < end else {throw failure("deadline")}
    waits += 1;pause(min(75_000_000,end-waiting))
    guard try tick() < end else {throw failure("deadline")}
    do {try identity()}catch{throw failure("identity-after-wait",error)}
    guard try tick() < end else {throw failure("deadline")}
  }
  throw failure("max-attempts")
}

func ui32InitialOwnedReadinessSelfTest() throws {
  let stub:[String:Any]=["stage":"content-scope","reason":"webarea-count","webAreaCount":0,"ownedWindowNodeCount":6,"nativeModalCount":0,
    "roleCounts":["AXWebArea":0,"AXGroup":2,"AXScrollArea":0,"AXSplitGroup":0,"AXButton":3,"AXWindow":1,"other":0]]
  for fault in ["first-pass","later-pass","deadline","max-attempts","identity-before","identity-after","identity-wait",
    "scope","count","web","role","modal","unsupported","not-first","overflow","clock","late-success","late-failure","changed-error","wrong-number-type","native-modal","missing-modal","modal-type","clamped-pass","remaining"] {
    var clock:UInt64=fault == "overflow" ? UInt64.max-1 : 100,ids=0,reads=[UInt64](),pauses=[UInt64](),answer:Int?,facts=[String:Any]()
    do {
      let result=try ui32InitialOwnedReadiness(first:fault != "not-first",now:{clock},identity:{ () throws -> Void in
        ids += 1
        if (fault == "identity-before" && ids == 1) || (fault == "identity-after" && ids == 2) || (fault == "identity-wait" && ids == 3) {throw Refusal(code:"identity-control")}
      },pause:{ duration in
        pauses.append(duration)
        if fault == "deadline" {clock=5_000_000_100}
        else if fault == "clamped-pass" {clock=4_900_000_100}
        else if fault == "remaining" && pauses.count == 1 {clock=4_990_000_100}
        else if fault != "max-attempts" {clock += duration}
      },read:{ (deadline:UInt64) throws -> Int in
        reads.append(deadline)
        if fault == "clock" {clock=99}
        if fault == "late-success" || fault == "late-failure" {clock=deadline}
        if fault == "first-pass" || fault == "late-success" || (["later-pass","clamped-pass"].contains(fault) && reads.count == 2) {return 17}
        if fault == "unsupported" {throw Refusal(code:"unsupported-control")}
        var failed=stub
        if fault == "scope" || (fault == "changed-error" && reads.count == 2) {failed["stage"]="other"}
        if fault == "wrong-number-type" {failed["webAreaCount"]=false}
        if fault == "native-modal" {failed["nativeModalCount"]=1}
        if fault == "missing-modal" {failed.removeValue(forKey:"nativeModalCount")}
        if fault == "modal-type" {failed["nativeModalCount"]=false}
        if fault == "count" {failed["ownedWindowNodeCount"]=7}
        if fault == "web" {failed["webAreaCount"]=2}
        if fault == "role" {failed["roleCounts"]=["AXWindow":1,"AXButton":2,"AXGroup":3,"AXWebArea":0,"AXScrollArea":0,"AXSplitGroup":0,"other":0]}
        if fault == "modal" {failed["modal"]=true}
        throw ActionRefusal(diagnostics:failed)
      })
      answer=result.value;facts=result.proof
    }catch let error as ActionRefusal {facts=error.diagnostics}
    let passed=["first-pass","later-pass","clamped-pass"].contains(fault)
    try check((answer == 17) == passed && facts["attempts"] as? Int == reads.count && facts["waits"] as? Int == pauses.count)
    try check(reads.count <= 64 && pauses.allSatisfy {$0 > 0 && $0 <= 75_000_000} && reads.allSatisfy {$0 <= 5_000_000_100})
    if fault == "first-pass" {try check(reads == [750_000_100] && ids == 2 && pauses.isEmpty)}
    if fault == "later-pass" {try check(reads == [750_000_100,825_000_100] && ids == 5 && pauses == [75_000_000])}
    if fault == "clamped-pass" {try check(reads == [750_000_100,5_000_000_100] && ids == 5)}
    if fault == "remaining" {try check(reads == [750_000_100,5_000_000_100] && pauses == [75_000_000,10_000_000] && ids == 5 && facts["reason"] as? String == "deadline")}
    if fault == "max-attempts" {try check(reads.count == 64 && pauses.count == 63 && facts["reason"] as? String == "max-attempts")}
    if ["scope","count","web","role","modal","unsupported","identity-after","clock","late-success","late-failure","wrong-number-type","native-modal","missing-modal","modal-type"].contains(fault) {try check(reads.count == 1 && pauses.isEmpty)}
    if ["scope","count","web","role","modal","unsupported","wrong-number-type","native-modal","missing-modal","modal-type"].contains(fault) {try check(facts["reason"] as? String == "nonretryable")}
    if fault == "late-success" || fault == "late-failure" {try check(facts["reason"] as? String == "probe-deadline")}
    if ["not-first","overflow","identity-before"].contains(fault) {try check(reads.isEmpty && pauses.isEmpty)}
    if fault == "not-first" || fault == "overflow" {try check(ids == 0)}
    if fault == "identity-wait" || fault == "deadline" {try check(reads.count == 1 && pauses.count == 1)}
    if fault == "deadline" {try check(ids == 2)}
    if ["late-success","late-failure","clock"].contains(fault) {try check(ids == 1)}
    if fault == "changed-error" {
      let first=(facts["firstFailure"] as? [String:Any])?["facts"] as? [String:Any],last=(facts["lastFailure"] as? [String:Any])?["facts"] as? [String:Any]
      try check(reads.count == 2 && pauses.count == 1 && first?["stage"] as? String == "content-scope" && last?["stage"] as? String == "other")
    }
    if !reads.isEmpty && fault != "first-pass" && fault != "late-success" {try check(facts["firstFailure"] is [String:Any] && facts["lastFailure"] is [String:Any])}
  }
}

func ui32WebAreaCountSelfTest() throws {
  for (count,modal,startup) in [(0,false,false),(2,false,false),(0,false,true),(0,true,true)] {
    let roles=startup ? ["AXWindow","AXButton","AXButton","AXButton","AXGroup","AXGroup"] : ["AXWindow","AXGroup","AXScrollArea","AXSplitGroup","AXButton","AXStaticText"] + Array(repeating:"AXWebArea",count:count)
    var traits=0,parents=0,children=0,strict=0,facts:[String:Any]?
    do {
      let _:UI32ContentCertificate<Int>=try ui32ContentCertificate(0,pid:7,retained:nil,deadline:10,
        now:{0},equal:==,owner:{_ in (0,true,7)},prepare:{_ in true},traits:{node in
          traits += 1;return (roles[node],modal && node == 4)
        },children:{node in
          children += 1;return (0,node == 0 ? Array(1..<roles.count) : [])
        },parent:{_ in parents += 1;return (0,0)},strict:{_,_ in strict += 1;return []})
    } catch let error as ActionRefusal { facts=error.diagnostics }
    guard let facts,let counts=facts["roleCounts"] as? [String:Int] else { throw Refusal(code:"adapter-refused") }
    try check(facts["stage"] as? String == "content-scope" && facts["reason"] as? String == "webarea-count")
    try check(facts["webAreaCount"] as? Int == count && facts["ownedWindowNodeCount"] as? Int == roles.count)
    let expected=startup ? ["AXWebArea":0,"AXGroup":2,"AXScrollArea":0,"AXSplitGroup":0,"AXButton":3,"AXWindow":1,"other":0] : ["AXWebArea":count,"AXGroup":1,"AXScrollArea":1,"AXSplitGroup":1,"AXButton":1,"AXWindow":1,"other":1]
    try check(counts == expected && facts["nativeModalCount"] as? Int == (modal ? 1 : 0))
    try check(Set(facts.keys) == Set(["stage","reason","webAreaCount","ownedWindowNodeCount","roleCounts","nativeModalCount"]))
    try check(traits == roles.count && children == roles.count && parents == roles.count-1 && strict == 0)
    if startup {
      var reads=0,accepted=false
      do {let ready=try ui32InitialOwnedReadiness(first:true,now:{0},identity:{},pause:{_ in},read:{(_) throws -> Int in
        reads += 1;if reads == 1 {throw ActionRefusal(diagnostics:facts)};return 17
      });accepted=ready.value == 17}catch is ActionRefusal {}
      try check(accepted == !modal && reads == (modal ? 1 : 2))
    }
  }
}
