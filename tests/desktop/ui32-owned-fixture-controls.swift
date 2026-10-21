// Existing pure fixture controls; original bodies moved byte-for-byte.
import Foundation

func ui32FixtureSelfTest() throws {
  try ui32FixtureOpenIdentitySelfTest()
  try ui32PanelTraceSelfTest()
  try ui32FixedFixtureSelfTest()
  try ui32ContentScopeSelfTest()
  for failure in ["none","window","certificate"] {
    var completed=false,refused=false
    do { try ui32FinishAfterDismissal(failure != "window",certificate:{
      if failure == "certificate" { throw Refusal(code:"adapter-refused") }
    },finish:{completed=true}) } catch { refused=true }
    try check(completed == (failure == "none") && refused == (failure != "none"))
  }
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

// These injected AX ports exercise the SAME certificate and unchanged strict
// walker used by production. No native app/AX read is part of this self-test.
func ui32ContentScopeSelfTest() throws {
  try ui32WebAreaCountSelfTest()
  let faults=["none","no-web","multiple","nested","replaced","foreign","owner-error","unreadable",
    "role-error","timeout","parent-unreadable","parent-no-value","parent-invalid-element","parent-error-with-value","parent-outside","root-path","inside-edge","sheet","modal",
    "inside-sheet","inside-html-dialog","window","alias","children-limit","depth","nodes","deadline","changed","root-path-changed"]
  for fault in faults {
    var graph=[0:[1,3],1:[2],2:[],3:[4],4:[]],parents=[1:0,2:1,3:0,4:0]
    var roles=[0:"AXWindow",1:"AXWebArea",2:"AXButton",3:"AXButton",4:"AXGroup"]
    if fault == "no-web" { roles[1]="AXGroup" }
    if fault == "multiple" { roles[3]="AXWebArea" }
    if fault == "nested" { roles[2]="AXWebArea" }
    if fault == "root-path" { parents[1]=3 }
    if fault == "parent-outside" { parents[4]=99 }
    if fault == "inside-edge" { parents[2]=0 }
    if fault == "sheet" { roles[4]="AXSheet" }
    if fault == "inside-sheet" { roles[2]="AXSheet" }
    if fault == "window" { roles[4]="AXWindow" }
    if fault == "alias" { graph[0]=[1,3,1] }
    if fault == "children-limit" { graph[0]=Array(repeating:1,count:513) }
    if fault == "depth" {
      for i in 0...13 { graph[i]=i < 13 ? [i+1] : [];if i>0 { parents[i]=i-1;roles[i]=i == 1 ? "AXWebArea" : "AXGroup" } }
    }
    if fault == "nodes" {
      graph[0]=Array(1...512)
      for i in 1...512 { graph[i]=[];parents[i]=0;roles[i]=i == 1 ? "AXWebArea" : "AXGroup" }
    }
    var attributeReads=[Int](),refused=false,reason="",completed=false,facts=[String:Any](),result:UI32ContentCertificate<Int>?
    let owner:(Int)->(Int,Bool,Int)={ node in
      if fault == "owner-error" && node == 4 { return (-1,false,0) }
      return (0,!(fault == "foreign" && node == 4),fault == "foreign" && node == 4 ? 8 : 7)
    }
    let children:(Int)->(Int,[Int]?)={ node in
      attributeReads.append(node);return fault == "unreadable" && node == 4 ? (-1,nil) : (0,graph[node] ?? [])
    }
    let parent:(Int)->(Int,Int?)={ node in
      attributeReads.append(node)
      if node == 4 && fault == "parent-no-value" {return (0,nil)}
      if node == 4 && ["parent-invalid-element","parent-error-with-value"].contains(fault) {return (-25202,fault == "parent-error-with-value" ? 3 : nil)}
      return fault == "parent-unreadable" && node == 4 ? (-1,nil) : (0,parents[node])
    }
    do {
      try ui32FinishAfterDismissal(true,certificate:{
      result=try ui32ContentCertificate(0,pid:7,retained:fault == "replaced" ? 99 : nil,deadline:10,
        now:{fault == "deadline" ? 10 : 0},equal:==,owner:owner,prepare:{node in fault != "timeout" || node != 4},traits:{ node in
          attributeReads.append(node)
          if fault == "role-error" && node == 4 { throw Refusal(code:"adapter-refused") }
          return (roles[node] ?? "AXGroup",(fault == "modal" && node == 4) || (fault == "inside-html-dialog" && node == 2))
        },children:children,parent:parent,strict:{ web,deadline in
          if fault == "changed" { graph[1]=[2,6];parents[6]=1;roles[6]="AXGroup" }
          if fault == "root-path-changed" { parents[1]=3 }
          return try boundedOwnedTree(web,owner:7,deadline:deadline,now:{0},equal:==,owner:owner,children:children,parent:parent)
        })
      },finish:{completed=true})
    } catch let error as ActionRefusal { refused=true;facts=error.diagnostics;reason=facts["stage"] as? String ?? "" }
    catch { refused=true }
    try check(refused == (fault != "none" && fault != "inside-html-dialog"))
    try check(completed == (!refused))
    if ["parent-unreadable","parent-no-value","parent-invalid-element","parent-error-with-value"].contains(fault) {
      try check(facts["reason"] as? String == "parent-query" && facts["queryError"] as? Int == (fault == "parent-no-value" ? 0 : fault == "parent-unreadable" ? -1 : -25202))
      try check(facts["parentPresent"] as? Bool == (fault == "parent-error-with-value") && facts["nodeIndex"] as? Int == 4 && facts["depth"] as? Int == 2 && facts["role"] as? String == "AXGroup")
      try check(facts["declaredParentIndex"] as? Int == 3 && facts["queriedNodeOwnerVerified"] as? Bool == true && attributeReads.filter{$0 == 4}.count == 2)
    }
    if fault == "inside-sheet" { try check(!completed) }
    if fault == "inside-edge" { try check(reason == "parent-edge") }
    if fault == "foreign" || fault == "owner-error" { try check(!attributeReads.contains(4)) }
    if fault == "none" {
      try check(result!.inside == Set([1,2]) && result!.outsideMismatches == [4])
      // The original whole-window reciprocal result remains a failure.
      var originalRefused=false
      do { _ = try boundedOwnedTree(0,owner:7,deadline:10,now:{0},equal:==,owner:owner,children:children,parent:parent) }
      catch let error as ActionRefusal { originalRefused=error.diagnostics["stage"] as? String == "parent-edge" }
      try check(originalRefused)
    }
  }
  try check(try ui32ScopedCandidate([2],[]) == 2)
  for values in [([Int](),[Int]()),([1,2],[]),([2],[3])] {
    var refused=false;do { _ = try ui32ScopedCandidate(values.0,values.1) } catch { refused=true };try check(refused)
  }
}

// Exercise the actual shared walks; no copied traversal or native AX access.
func ui32PanelTraceSelfTest() throws {
  for kind in ["attached-foreign-root","bounded-owned-tree"] {
    for fault in ["edge","children","zero","foreign","owner-error","parent-query","parent-outside"] {
      let trace=UI32PanelTrace(kind,0,7,equal:==)
      var readNodes=[Int](),parentNodes=[Int](),described=[Int](),facts:[String:Any]?,succeeded=false
      let owner:(Int)->(Int,Bool,Int)={node in
        trace.ownership(node,(fault == "owner-error" && node == 2 ? -1 : 0,
          !(fault == "foreign" && node == 2),fault == "foreign" && node == 2 ? 8 : 7))
      }
      let children:(Int)->(Int,[Int]?)={node in
        readNodes.append(node)
        return trace.children(node,fault == "children" && node == 1 ? (-1,nil) : (0,node < 2 ? [node+1] : []))
      }
      let parent:(Int)->(Int,Int?)={node in
        parentNodes.append(node)
        return trace.parent(node,fault == "parent-query" && node == 2 ? (-1,nil) : (0,fault == "parent-outside" && node == 2 ? 99 : fault == "edge" && node == 2 ? 0 : node-1))
      }
      do {
        if kind == "attached-foreign-root" {
          let found=try attachedForeignRoot(0,owner:7,deadline:10,now:{0},equal:==,owner:owner,children:children,parent:parent,
            foreignChild:{_,_ in trace.foreignRoots += 1})
          try check(found.root == 2 && fault == "foreign")
        } else {try check(try boundedOwnedTree(0,owner:7,deadline:10,now:{0},equal:==,owner:owner,children:children,parent:parent) == [0,1,2])}
        succeeded=true
      } catch let error as ActionRefusal {
        facts=trace.failure(error,describe:{node in described.append(node);return ["injectedNode":node]}).diagnostics
      }
      try check(succeeded == ((kind == "attached-foreign-root" && fault == "foreign") || (kind == "bounded-owned-tree" && fault == "zero")))
      if let facts,let context=facts["panelTraversal"] as? [String:Any] {
        try check(context["walk"] as? String == kind)
        let complete=kind == "attached-foreign-root" && fault == "zero"
        try check(context["completedForeignSearch"] as? Bool == complete)
        if fault == "edge" {
          try check(facts["stage"] as? String == "parent-edge" && context["routes"] as? [String] == ["0.0"])
          try check(context["enteredNodeCount"] as? Int == 2 && context["declaredParentIndices"] as? [Int] == [1])
          try check(context["reportedMatchesDeclared"] as? Bool == false)
        }
        if fault == "children" {try check(facts["stage"] as? String == "children-query" && !complete)}
      }
      if fault == "foreign" || fault == "owner-error" {try check(!readNodes.contains(2) && !parentNodes.contains(2) && !described.contains(2))}
      try check(!described.contains(99))
    }
  }
}

// Full identity is an injected read only; no action or AX is simulated as acceptance.
func ui32FixtureOpenIdentitySelfTest() throws {
  let busy: [String: Any] = ["axPidMatches": true, "axPidQueryError": 0,
    "applicationWindowsQueryError": -25204, "applicationWindowsReadable": false,
    "pinnedAXWindowMember": false, "minimizedQueryError": -25204,
    "minimizedReadable": false, "minimized": false,
    "pinnedCGWindowListReadable": true, "pinnedCGWindowCount": 1]
  func identityError(_ facts: [String: Any]) -> IdentityRefusal {
    IdentityRefusal(stage: "application-window-membership", facts: facts)
  }
  try check(ui32FixtureOpenBusy(identityError(busy)))
  var changed = [[String: Any]]()
  for key in busy.keys {
    var absent = busy; absent.removeValue(forKey: key); changed.append(absent)
    var wrongType = busy
    if type(of: busy[key]!) == Bool.self { wrongType[key] = 0 }
    else { wrongType[key] = false }
    changed.append(wrongType)
    var wrongValue = busy
    if let value = busy[key] as? Bool, type(of: busy[key]!) == Bool.self { wrongValue[key] = !value }
    else { wrongValue[key] = 99 }
    changed.append(wrongValue)
  }
  var extra = busy; extra["unexpected"] = true; changed.append(extra)
  for facts in changed {
    try check(!ui32FixtureOpenBusy(identityError(facts)))
    var reads = 0, waits = 0, refused = false
    do {
      _ = try ui32FixtureOpenIdentity("fixture.open", true, true, now: { 0 },
        pause: { _ in waits += 1 }, identity: { reads += 1; throw identityError(facts) })
    } catch { refused = true }
    try check(refused && reads == 1 && waits == 0)
  }
  try check(!ui32FixtureOpenBusy(IdentityRefusal(stage: "process-start", facts: busy)))
  try check(!ui32FixtureOpenBusy(Refusal(code: "adapter-refused")))
  let scenarios = ["immediate", "settled", "persistent", "other", "before-read",
    "after-first-read", "late-immediate", "before-delay", "delay-deadline", "after-delay", "after-second-read",
    "clock-regression", "overflow", "command", "unattempted", "unposted"]
  for scenario in scenarios {
    var clock: UInt64 = scenario == "overflow" ? UInt64.max : 100
    var ticks = 0, reads = 0, waits = 0, refused = false
    var delays = [UInt64]()
    var proof = [String: Any]()
    do {
      proof = try ui32FixtureOpenIdentity(scenario == "command" ? "fixture.observe" : "fixture.open",
        scenario != "unattempted", scenario != "unposted", now: {
          ticks += 1
          if ticks == 2 && scenario == "before-read" { clock = 750_000_100 }
          if ticks == 2 && scenario == "clock-regression" { clock = 99 }
          if ticks == 4 && scenario == "delay-deadline" { clock = 750_000_100 }
          return clock
        }, pause: { delay in
          delays.append(delay)
          waits += 1; clock += delay
          if scenario == "after-delay" { clock = 750_000_100 }
        }, identity: {
          reads += 1
          if ["after-first-read", "late-immediate"].contains(scenario) || (scenario == "after-second-read" && reads == 2) {
            clock = 750_000_100
          }
          if scenario == "before-delay" { clock = 700_000_100 }
          if scenario == "other" { throw Refusal(code: "adapter-refused") }
          if !["immediate", "late-immediate"].contains(scenario) && (reads == 1 || scenario == "persistent") { throw identityError(busy) }
        })
    } catch let error as ActionRefusal { refused = true; proof = error.diagnostics }
    catch { refused = true }
    try check(refused == !["immediate", "settled"].contains(scenario))
    let unread = ["before-read", "clock-regression", "overflow", "command", "unattempted", "unposted"].contains(scenario)
    let second = ["settled", "persistent", "after-second-read"].contains(scenario)
    try check(reads == (unread ? 0 : second ? 2 : 1))
    try check(delays.allSatisfy { $0 == 75_000_000 })
    let delayed = ["settled", "persistent", "after-delay", "after-second-read"].contains(scenario)
    try check(waits == (delayed ? 1 : 0) && proof["readAttempts"] as? Int == reads)
    if !unread && !["immediate", "late-immediate"].contains(scenario) {
      let first = proof["firstReadFailure"] as? [String: Any]
      try check(first != nil && first!["kind"] as? String == (scenario == "other" ? "refusal" : "identity"))
    }
    if !refused { try check(proof["reason"] as? String == "full-original-identity") }
  }
  // The real generic guarded-action route still posts exactly ONE Open action.
  for scenario in ["immediate", "settled", "persistent", "strict-post-scope"] {
    var attempted = false, posted = false, actions = 0, identities = 0, postScopes = 0
    var waits = 0, refused = false, retained = [String: Any]()
    func identity() throws {
      identities += 1
      if posted && (identities == 3 && scenario != "immediate" || scenario == "persistent") {
        throw identityError(busy)
      }
      if postScopes > 0 && scenario == "strict-post-scope" { throw Refusal(code: "adapter-refused") }
    }
    do {
      try ui32GuardedAction({
        if attempted && posted {
          retained = try ui32FixtureOpenIdentity("fixture.open", attempted, posted,
            now: { 0 }, pause: { _ in waits += 1 }, identity: identity)
        } else { try identity() }
      }, {}, { actions += 1; attempted = true }, postScope: {
        postScopes += 1; try identity()
      }, posted: { posted = true })
    } catch { refused = true }
    try check(actions == 1 && attempted && posted)
    try check(refused == ["persistent", "strict-post-scope"].contains(scenario))
    try check(postScopes == (scenario == "persistent" ? 0 : 1))
    try check(identities == (scenario == "immediate" || scenario == "persistent" ? 4 : 5))
    try check(waits == (scenario == "immediate" ? 0 : 1))
    if scenario == "settled" || scenario == "strict-post-scope" {
      try check(retained["readAttempts"] as? Int == 2 && retained["firstReadFailure"] as? [String: Any] != nil)
    }
  }
}
