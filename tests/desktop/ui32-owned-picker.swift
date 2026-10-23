// Fixed fixture picker ownership proofs; no global input or product engine.
import Foundation
import ApplicationServices

// UI32-only discovery certificate. Service authentication and strict service walk follow unchanged.
func ui32AttachedRootCertificate<Node>(_ root:Node, owner expectedOwner:Int, deadline:UInt64,
  now:()->UInt64, equal:(Node,Node)->Bool, owner:(Node)->(Int,Bool,Int), children:(Node)->(Int,[Node]?),
  parent:(Node)->(Int,Node?), foreignChild:(String,Int)->Void = {_,_ in},
  completedOwnedCensus:([Node],[String])->Void = {_,_ in}) throws
  -> (root:Node,parent:Node,pid:Int,route:String,ownedNodeCount:Int,ownedPathRoutes:[String],outsideMismatches:[[String:Any]],ownedNodes:[Node],ownedRoutes:[String]) {
  var nodes=[Node](), routes=[String](), declared=[Int?](), mismatches=[(Int,Int,Node)](), foreign=[(Node,Int,Int,String)]()
  func walk(_ current:Node, _ ancestor:Int?, _ reported:Node?, _ route:String, _ depth:Int) throws {
    try locationCheck(now() < deadline,"walk-deadline")
    try locationCheck(depth <= 12,"depth-limit");try locationCheck(nodes.count < 512,"node-limit")
    let (error,owned,actual)=owner(current)
    try locationCheck(error == 0,"node-owner-query")
    try locationCheck(owned && actual == expectedOwner,"node-owner-mismatch")
    try locationCheck(!nodes.contains {equal($0,current)},"cycle")
    let index=nodes.count;nodes.append(current);routes.append(route);declared.append(ancestor)
    if let ancestor,let reported,!equal(reported,nodes[ancestor]) {mismatches.append((index,ancestor,reported))}
    let (query,rows)=children(current)
    try locationCheck(query == 0 && rows != nil,"children-query")
    guard let rows else {throw ActionRefusal(diagnostics:["stage":"goto-discovery","gotoReason":"children-shape"])}
    try locationCheck(rows.count <= 512,"children-limit")
    for (offset,child) in rows.enumerated() {
      try locationCheck(now() < deadline,"walk-deadline")
      let (childError,childOwned,childPID)=owner(child)
      try locationCheck(childError == 0,"child-owner-query")
      let childRoute=route.isEmpty ? String(offset) : route+"."+String(offset)
      if !childOwned || childPID != expectedOwner {
        // No foreign parent, role, children or value read before caller authentication.
        foreignChild(childRoute,childPID);foreign.append((child,index,childPID,childRoute));continue
      }
      let (parentError,observed)=parent(child)
      try locationCheck(parentError == 0 && observed != nil,"parent-query")
      try walk(child,index,observed,childRoute,depth+1)
    }
  }
  try walk(root,nil,nil,"",0);try locationCheck(now() < deadline,"walk-deadline")
  for (child,_,reported) in mismatches where !nodes.contains(where:{equal($0,reported)}) {
    throw ActionRefusal(diagnostics:["stage":"parent-outside-owned-census","ownedSearchComplete":true,"childRoute":routes[child]])
  }
  completedOwnedCensus(nodes,routes)
  if foreign.isEmpty && !mismatches.isEmpty {
    throw ActionRefusal(diagnostics:["stage":"parent-edge","ownedSearchComplete":true,
      "foreignRootCount":0,"ownedParentMismatchCount":mismatches.count,"candidatePathKnown":false])
  }
  guard foreign.count == 1 else {
    throw ActionRefusal(diagnostics:["stage":"candidate-count","ownedSearchComplete":true,
      "foreignRootCount":foreign.count,"ownedParentMismatchCount":mismatches.count,"ownedNodeCount":nodes.count])
  }
  let (candidate,ancestor,pid,route)=foreign[0]
  try locationCheck(pid > 0 && !equal(candidate,nodes[ancestor]),"context-identity")
  var path=Set<Int>(), cursor:Int?=ancestor
  while let index=cursor {path.insert(index);cursor=declared[index]}
  for (child,parent,_) in mismatches where path.contains(child) {
    throw ActionRefusal(diagnostics:["stage":"parent-edge","ownedSearchComplete":true,"candidatePathKnown":true,
      "candidatePathMismatch":true,"childRoute":routes[child],"declaredParentRoute":routes[parent],"foreignRootCount":1])
  }
  let outside=mismatches.map {child,parent,reported -> [String:Any] in
    let known=nodes.firstIndex {equal($0,reported)}
    return ["childRoute":routes[child],"declaredParentRoute":routes[parent],"reportedParentKnownIndex":known ?? -1,
      "reportedParentKnownRoute":known.map {routes[$0]} ?? "unknown","outsideCandidatePath":true]
  }
  return (candidate,nodes[ancestor],pid,route,nodes.count,path.sorted().map {routes[$0]},outside,nodes,routes)
}

// Pure ports execute the exact discovery function above, not a copied walker.
func ui32AttachedRootCertificateSelfTest() throws {
  try ui32OwnedParentReadSelfTest()
  for fault in ["none","outside-edge","path-edge","path-unknown","outside-parent-unknown","outside-owner","outside-parent","outside-children",
    "alias","two","zero","zero-edge","zero-incomplete","foreign-owner","foreign-pid"] {
    var childReads=[Int](),parentReads=[Int](),ownerReads=[Int](),observed=[Int](),facts:[String:Any]?,admitted=false
    let owner:(Int)->(Int,Bool,Int)={node in
      ownerReads.append(node)
      return ((fault == "outside-owner" && node == 4) || (fault == "foreign-owner" && node == 9) ? -1 : 0,
        node != 8 && node != 9,node == 9 && fault == "foreign-pid" ? 0 : node == 8 || node == 9 ? 8 : 7)
    }
    do {
      let result=try ui32AttachedRootCertificate(0,owner:7,deadline:10,now:{0},equal:==,owner:owner,children:{node in
        childReads.append(node)
        if node == 3 && ["outside-children","zero-incomplete"].contains(fault) {return (-1,nil)}
        let graph=[0:[1,3],1:[2],2:fault.hasPrefix("zero") ? [] : [9],3:fault == "alias" ? [1] : fault == "two" ? [4,8] : [4],4:[]]
        return (0,graph[node] ?? [])
      },parent:{node in
        parentReads.append(node)
        if node == 4 && fault == "outside-parent" {return (-1,nil)}
        if (node == 2 && fault == "path-unknown") || (node == 4 && fault == "outside-parent-unknown") {return (0,77)}
        return (0,node == 1 || node == 3 || (node == 2 && fault == "path-edge") ||
          (node == 4 && ["outside-edge","zero-edge"].contains(fault)) ? 0 : node == 2 ? 1 : 3)
      },foreignChild:{_,pid in observed.append(pid)})
      admitted=true
      try check(result.root == 9 && result.parent == 2 && result.pid == 8 && result.route == "0.0.0")
      try check(result.ownedNodeCount == 5 && result.ownedPathRoutes == ["","0","0.0"] && childReads == [0,1,2,3,4])
      try check(result.outsideMismatches.count == (fault == "outside-edge" ? 1 : 0))
      if fault == "outside-edge" {try check(result.outsideMismatches[0]["childRoute"] as? String == "1.0")}
    } catch let error as ActionRefusal {facts=error.diagnostics}
    try check(admitted == ["none","outside-edge"].contains(fault))
    try check(!childReads.contains(9) && !parentReads.contains(9) && !childReads.contains(8) && !parentReads.contains(8) && !ownerReads.contains(77))
    if fault == "path-edge" {try check(facts?["candidatePathMismatch"] as? Bool == true)}
    if ["path-unknown","outside-parent-unknown"].contains(fault) {try check(facts?["stage"] as? String == "parent-outside-owned-census")}
    if fault == "alias" {try check(facts?["stage"] as? String == "cycle")}
    if fault == "two" {try check(facts?["stage"] as? String == "candidate-count" && facts?["foreignRootCount"] as? Int == 2)}
    if fault == "foreign-pid" {try check(facts?["stage"] as? String == "context-identity")}
    if fault == "zero" {try check(facts?["stage"] as? String == "candidate-count" && facts?["ownedSearchComplete"] as? Bool == true && observed.isEmpty)}
    if fault == "zero-edge" {try check(facts?["stage"] as? String == "parent-edge" && observed.isEmpty)}
    if ["outside-children","outside-parent","outside-owner","zero-incomplete","foreign-owner"].contains(fault) {
      try check(facts?["ownedSearchComplete"] as? Bool != true && facts?["stage"] as? String != "candidate-count")
    }
  }
}

func ui32ZeroForeignFallbackSelfTest() throws {
  let complete:[String:Any]=["stage":"candidate-count","ownedSearchComplete":true,"foreignRootCount":0,"ownedParentMismatchCount":0]
  try check(ui32ZeroForeignFallback(0,complete) && !ui32ZeroForeignFallback(1,complete))
  for (key,bad) in [("stage","parent-edge" as Any),("ownedSearchComplete",false),("foreignRootCount",1),("ownedParentMismatchCount",1)] {
    var facts=complete;facts[key]=bad;try check(!ui32ZeroForeignFallback(0,facts));facts.removeValue(forKey:key);try check(!ui32ZeroForeignFallback(0,facts))
  }
}


// Pure path proof only. Caller supplies a complete owned census and authenticated opaque externals.
func ui32OwnedCandidatePath<Node>(_ candidate:Node, _ root:Node, _ nodes:[Node], _ routes:[String],
  deadline:UInt64, now:()->UInt64, equal:(Node,Node)->Bool, owned:(Node)->Bool,
  parent:(Node)->(Int,Node?), children:(Node)->(Int,[Node]?), knownExternalOpaque:[Node]) throws
  -> (nodes:[Node],indices:[Int],routes:[String]) {
  func require(_ ok:Bool, _ reason:String) throws {try locationCheck(ok,"owned-candidate-path",reason)}
  func fresh() throws {try require(now() < deadline,"deadline")}
  try fresh();try require(!nodes.isEmpty && nodes.count <= 512 && nodes.count == routes.count && knownExternalOpaque.count <= 512,"bounds")
  try require(routes[0].isEmpty && equal(nodes[0],root) && Set(routes).count == routes.count,"root-or-route-alias")
  var declared=[Int?](repeating:nil,count:nodes.count),offsets=[Int](repeating:0,count:nodes.count)
  for i in nodes.indices {
    try fresh();try require(!nodes[..<i].contains {equal($0,nodes[i])},"census-node-alias")
    if i == 0 {continue}
    let parts=routes[i].split(separator:".",omittingEmptySubsequences:false)
    try require(!parts.isEmpty && parts.count <= 12,"route-depth")
    for part in parts {
      guard let value=Int(part),value >= 0,value < 512,String(value) == String(part) else {throw ActionRefusal(diagnostics:["stage":"owned-candidate-path","gotoReason":"route-component"])}
    }
    let prefix=parts.dropLast().joined(separator:".")
    guard let ancestor=routes.firstIndex(of:prefix),let offset=Int(parts.last!) else {throw ActionRefusal(diagnostics:["stage":"owned-candidate-path","gotoReason":"route-parent"])}
    declared[i]=ancestor;offsets[i]=offset
  }
  for i in knownExternalOpaque.indices {
    try fresh();try require(!nodes.contains {equal($0,knownExternalOpaque[i])} && !knownExternalOpaque[..<i].contains {equal($0,knownExternalOpaque[i])},"external-alias")
  }
  guard var index=nodes.firstIndex(where:{equal($0,candidate)}) else {throw ActionRefusal(diagnostics:["stage":"owned-candidate-path","gotoReason":"candidate-outside"])}
  var path=[Int]()
  while true {
    try fresh();try require(path.count <= 12 && !path.contains(index),"path-cycle-or-depth")
    let current=nodes[index];try require(owned(current),"path-owner");path.append(index)
    if index == 0 {break}
    guard let expected=declared[index] else {throw ActionRefusal(diagnostics:["stage":"owned-candidate-path","gotoReason":"path-parent"])}
    try fresh();let (query,actual)=parent(current);try fresh();try require(owned(current),"path-owner-changed")
    try require(query == 0 && actual != nil,"parent-query")
    try require(equal(actual!,nodes[expected]),"parent-edge");try require(owned(nodes[expected]),"parent-owner")
    try fresh();let (status,rows)=children(nodes[expected]);try fresh();try require(owned(nodes[expected]),"parent-owner-changed")
    try require(status == 0 && rows != nil,"children-query")
    guard let rows else {throw ActionRefusal(diagnostics:["stage":"owned-candidate-path","gotoReason":"children-shape"])}
    try require(rows.count <= 512 && rows.filter {equal($0,current)}.count == 1,"child-membership")
    for (offset,sibling) in rows.enumerated() {
      try fresh();try require(!rows[..<offset].contains {equal($0,sibling)},"sibling-alias")
      if let known=nodes.firstIndex(where:{equal($0,sibling)}) {
        try require(declared[known] == expected && offsets[known] == offset,"sibling-route")
      } else {try require(knownExternalOpaque.contains {equal($0,sibling)},"unknown-sibling")}
    }
    try require(offsets[index] < rows.count && equal(rows[offsets[index]],current),"child-order")
    index=expected
  }
  try fresh();return (path.map {nodes[$0]},path,path.map {routes[$0]})
}

func ui32OwnedCandidatePathSelfTest() throws {
  for fault in ["none","outside-mismatch","on-path","unknown-parent","stale","node-alias","route-alias","route-parent","cycle",
    "parent-error","children-error","budget","outside-ref","sibling-alias","sibling-moved","candidate-outside","external-alias","parent-stale","expired","depth","missing-child","parent-shape","route-component"] {
    var nodes=[0,1,2,3,4],routes=["","0","0.0","0.0.0","1"],external=[9],expired=false,changed=false
    var ownerCalls=[Int](),parentCalls=[Int](),childCalls=[Int](),refused=false,answer=[Int]()
    if fault == "node-alias" {nodes[4]=3}
    if fault == "route-alias" {routes[4]="0.0"}
    if fault == "route-parent" {routes[4]="4.1"}
    if fault == "external-alias" {external=[4]}
    if fault == "expired" {expired=true}
    if fault == "depth" {routes[4]=Array(repeating:"0",count:13).joined(separator:".")}
    if fault == "route-component" {routes[4]="01"}
    do {
      let result=try ui32OwnedCandidatePath(fault == "candidate-outside" ? 88 : 3,0,nodes,routes,deadline:10,
        now:{expired ? 10 : 0},equal:==,owned:{node in
          ownerCalls.append(node);return !(fault == "stale" && node == 3) && !(fault == "parent-stale" && changed && node == 2)
        },parent:{node in
          parentCalls.append(node)
          if fault == "budget" {expired=true}
          if fault == "parent-error" {return (-1,nil)}
          if fault == "parent-shape" {return (0,nil)}
          if fault == "unknown-parent" {return (0,88)}
          if fault == "cycle" {return (0,node)}
          if fault == "on-path" && node == 3 {return (0,1)}
          return (0,node == 4 && fault == "outside-mismatch" ? 2 : node-1)
        },children:{node in
          childCalls.append(node)
          if fault == "children-error" {return (-1,nil)}
          if fault == "missing-child" {return (0,[])}
          if fault == "parent-stale" {changed=true}
          if node == 2 && fault == "outside-ref" {return (0,[3,88])}
          if node == 2 && fault == "sibling-alias" {return (0,[3,3])}
          if node == 2 && fault == "sibling-moved" {return (0,[3,4])}
          return (0,node == 0 ? [1,4,9] : [node+1])
        },knownExternalOpaque:external)
      answer=result.nodes;try check(result.indices == [3,2,1,0] && result.routes == ["0.0.0","0.0","0",""])
    }catch is ActionRefusal {refused=true}
    try check(refused == !["none","outside-mismatch"].contains(fault))
    try check(!ownerCalls.contains(9) && !parentCalls.contains(9) && !childCalls.contains(9) && !parentCalls.contains(4) && !ownerCalls.contains(88) && !childCalls.contains(88))
    if !refused {try check(answer == [3,2,1,0] && parentCalls == [3,2,1] && childCalls == [2,1,0])}
    if fault == "budget" {try check(parentCalls == [3] && childCalls.isEmpty)}
    if fault == "expired" {try check(ownerCalls.isEmpty && parentCalls.isEmpty && childCalls.isEmpty)}
  }
}

// Fresh complete typed reads, never a saved diagnostic dictionary, supply these facts.
func ui32FileDomain(_ nemo:[String:Any], _ service:[String:Any], _ retained:String?) throws -> String {
  func matches(_ facts:[String:Any]) throws -> Int {
    try check(facts["complete"] as? Bool == true && facts["truncated"] as? Bool == false &&
      facts["unqueriedQueries"] as? Int == 0 && facts["failedQueries"] as? Int == 0)
    guard let count=facts["ownedNodeCount"] as? Int,count > 0,count <= 512,
      let rows=facts["nodes"] as? [[String:Any]],rows.count == count else {throw Refusal(code:"adapter-refused")}
    var actual=0
    for (i,row) in rows.enumerated() {
      try check(row["index"] as? Int == i && row["queriesStarted"] as? Int == 2)
      guard let match=row["expectedURLMatch"] as? Bool else {throw Refusal(code:"adapter-refused")}
      if match {actual += 1}
    }
    try check(facts["observedExpectedURLMatches"] as? Int == actual);return actual
  }
  let own=try ui32ProofStep("nemo-domain-facts"){try matches(nemo)},remote=try ui32ProofStep("service-domain-facts"){try matches(service)};try ui32ProofCheck(own+remote == 1,"combined-url-cardinality",["nemoMatches":own,"serviceMatches":remote])
  let domain=own == 1 ? "nemo-owned-sheet" : "authenticated-service"
  try ui32ProofCheck(retained == nil || retained == domain,"retained-file-domain");return domain
}
// Shared UI32 metadata only. A proof failure never asserts the state of prior effects.
func ui32ProofCheck(_ ok:Bool, _ step:String, _ facts:[String:Any]=[:]) throws {
  if !ok {throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"read-proof","proofStep":step,"facts":facts])}
}
func ui32ProofStep<T>(_ step:String, _ body:() throws -> T) throws -> T {
  do {return try body()} catch {
    throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"read-proof","proofStep":step,"underlying":ui32ActionCause(error)])
  }
}
func ui32ContentReadStep<T>(_ step:String, _ body:() throws -> T) throws -> T {
  do {return try body()} catch let error as ActionRefusal {throw error} catch {
    throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"content-read-proof","proofStep":step,"underlying":ui32ActionCause(error)])
  }
}
func ui32DiagnosticText(_ node:AXUIElement, _ pid:pid_t, _ key:String) throws -> String? {
  let category=key == kAXRoleAttribute ? "role" : key == kAXSubroleAttribute ? "subrole" : key == kAXValueAttribute ? "value" : "other"
  try ui32ProofCheck(owns(node,pid) && AXUIElementSetMessagingTimeout(node,0.03) == .success,"text-owner-timeout",["attribute":category])
  var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
  if query == .attributeUnsupported || query == .noValue {return nil}
  try ui32ProofCheck(query == .success,"text-query",["attribute":category,"queryError":Int(query.rawValue)])
  if let value=raw as? String {return value}
  if key == kAXValueAttribute && raw is NSNumber {return nil}
  throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"text-shape","attribute":category,"rawPresent":raw != nil])
}
// Operation-specific row selection proof; does not synthesize an Enabled value.
func ui32RowSelectionProof<Node>(_ row:Node, deadline:UInt64, now:()->UInt64,
  owned:(Node)->Bool, prepare:(Node)->Bool, read:(Node,String)->(Int,CFTypeRef?),
  selectedSettable:(Node)->(Int,Bool)) throws -> [String:Any] {
  func guarded<T>(_ query:()->T) throws -> T {
    try ui32ProofCheck(now() < deadline && owned(row) && now() < deadline && prepare(row) && now() < deadline && owned(row) && now() < deadline,"row-selection-preconditions")
    let result=query()
    try ui32ProofCheck(now() < deadline && owned(row) && now() < deadline,"row-selection-postconditions")
    return result
  }
  func boolean(_ raw:CFTypeRef?) -> Bool? {
    guard let raw,CFGetTypeID(raw) == CFBooleanGetTypeID() else {return nil}
    return raw as? Bool
  }
  let (status,raw)=try guarded {read(row,kAXEnabledAttribute)}
  let enabled=boolean(raw)
  let facts:[String:Any]=["queryError":status,"boolTyped":enabled != nil]
  if status == 0 {
    try ui32ProofCheck(enabled == true,"row-enabled",facts)
    return ["basis":"row-enabled","enabled":true]
  }
  try ui32ProofCheck(status == Int(AXError.attributeUnsupported.rawValue) && raw == nil,"row-enabled",facts)
  let (query,selected)=try guarded {read(row,kAXSelectedAttribute)}
  guard query == 0,let selectedValue=boolean(selected) else {
    throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"row-selected-capability","queryError":query,"boolTyped":boolean(selected) != nil])
  }
  let (settable,writable)=try guarded {selectedSettable(row)}
  try ui32ProofCheck(settable == 0 && writable,"row-selection-settable",["queryError":settable,"rawSettableBool":writable])
  return ["basis":"row-selection-capability","enabledQueryError":status,"selected":selectedValue,"selectedSettable":true]
}

extension UI32Fixture {
  func fileRead(_ node:AXUIElement, _ pid:pid_t, _ key:String, _ deadline:UInt64) throws -> (Int,CFTypeRef?) {
    try ui32ProofCheck(DispatchTime.now().uptimeNanoseconds < deadline && owns(node,pid) &&
      AXUIElementSetMessagingTimeout(node,0.03) == .success && owns(node,pid),"file-read-preconditions")
    var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
    try ui32ProofCheck(owns(node,pid) && DispatchTime.now().uptimeNanoseconds < deadline,"file-read-postconditions",["queryError":Int(status.rawValue)]);return (Int(status.rawValue),raw)
  }
  func fileRole(_ node:AXUIElement, _ pid:pid_t, _ deadline:UInt64) throws -> String {
    let (status,raw)=try fileRead(node,pid,kAXRoleAttribute,deadline)
    guard status == 0,let role=raw as? String,!role.isEmpty else {throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"file-role-query-shape","queryError":status])};return role
  }
  func filePath(_ candidate:AXUIElement, _ deadline:UInt64) throws -> [AXUIElement] {
    let outer=try ui32ProofStep("path-attached-sheet"){try attachedSheet()};let external=owner != pinned.pid ? root.map {[$0]} ?? [] : []
    var parentCause:[String:Any]?
    do {let proof=try ui32OwnedCandidatePath(candidate,outer,observedOwnedNodes,observedOwnedRoutes,
      deadline:deadline,now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owned:{owns($0,self.pinned.pid)},
      parent:{node in
        do {let (status,raw)=try self.fileRead(node,self.pinned.pid,kAXParentAttribute,deadline);return (status,axElement(raw))}
        catch {if parentCause == nil {parentCause=ui32ActionCause(error)};return (-1,nil)}
      },children:{ui32Children($0,self.pinned.pid)},knownExternalOpaque:external)
    _ = try ui32ProofStep("path-final-sheet"){try attachedSheet()};return proof.nodes
    }catch{var facts:[String:Any]=["stage":"read-proof","proofStep":"candidate-path","underlying":ui32ActionCause(error)];if let parentCause {facts["parentReadFailure"]=parentCause};throw ActionRefusal(diagnostics:facts)}
  }
  func fileAudit(_ nodes:[AXUIElement], _ pid:pid_t, _ deadline:UInt64, _ limit:Int) throws -> [String:Any] {
    try ui32OwnedSheetRead(nodes,nodes.indices.map {String($0)},fixture,deadline:deadline,limit:limit,
      now:{DispatchTime.now().uptimeNanoseconds},owned:{owns($0,pid)},
      prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},read:{node,key in
        var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw);return (Int(status.rawValue),raw)
      })
  }
  func fileNemoRow(_ urlNode:AXUIElement, _ deadline:UInt64) throws -> AXUIElement {
    try ui32ProofCheck(observedOwnedNodes.contains {CFEqual($0,urlNode)},"url-candidate-membership");let path=try filePath(urlNode,deadline)
    var row:AXUIElement?,outline:AXUIElement?
    for node in path {
      let role=try fileRole(node,pinned.pid,deadline)
      if role == kAXRowRole {try ui32ProofCheck(row == nil,"row-unique");row=node}
      if role == kAXOutlineRole {try ui32ProofCheck(row != nil && outline == nil,"outline-order-unique");outline=node}
    }
    guard let row,outline != nil else {throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"row-outline-present"])}
    _ = try ui32RowSelectionProof(row,deadline:deadline,now:{DispatchTime.now().uptimeNanoseconds},
      owned:{owns($0,self.pinned.pid)},prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},read:{node,key in
        var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
        return (Int(status.rawValue),raw)
      },selectedSettable:{node in
        var writable=DarwinBoolean(false)
        let status=AXUIElementIsAttributeSettable(node,kAXSelectedAttribute as CFString,&writable)
        return (Int(status.rawValue),writable.boolValue)
      })
    let (status,raw)=try fileRead(urlNode,pinned.pid,kAXURLAttribute,deadline)
    try ui32ProofCheck(status == 0 && ui32FixtureURLMatches(raw as? URL,fixture),"row-url-reread",["queryError":status,"urlTyped":raw is URL]);try ui32ProofStep("row-file-hash"){try fileProof()};return row
  }
  func fileButton(_ labels:Set<String>, _ scopes:[([AXUIElement],pid_t,[String:Any])], _ expectedPID:pid_t, _ deadline:UInt64) throws -> AXUIElement {
    var matches=[(AXUIElement,pid_t)]()
    for (nodes,pid,facts) in scopes {
      guard let rows=facts["nodes"] as? [[String:Any]],rows.count == nodes.count else {throw Refusal(code:"adapter-refused")}
      for (i,node) in nodes.enumerated() where rows[i]["role"] as? String == kAXButtonRole {
        let (status,raw)=try fileRead(node,pid,kAXTitleAttribute,deadline)
        if [Int(AXError.attributeUnsupported.rawValue),Int(AXError.noValue.rawValue)].contains(status) {try check(raw == nil);continue}
        guard status == 0,let title=raw as? String,title.utf8.count <= 240 else {throw Refusal(code:"adapter-refused")}
        if labels.contains(title) {
          let (enabled,value)=try fileRead(node,pid,kAXEnabledAttribute,deadline)
          guard enabled == 0,let isEnabled=value as? Bool else {throw Refusal(code:"adapter-refused")}
          if isEnabled {matches.append((node,pid))}
        }
      }
    }
    try check(matches.count == 1)
    let (button,pid)=matches[0];try check(pid == expectedPID)
    if pid == pinned.pid {_ = try filePath(button,deadline)};return button
  }
  func fileBind(_ node:AXUIElement, _ pid:pid_t, button:Bool=false) throws {
    if cancelOnly || fileDomain == "same-pid-original" {return}
    try check(pid == (fileDomain == "nemo-owned-sheet" ? pinned.pid : owner))
    try ui32FileBinding(node,button ? fileCertifiedButton : fileCertifiedRow,equal:{CFEqual($0,$1)},owned:{owns($0,pid)})
  }
  func actionPanel(_ command:String) throws -> ([AXUIElement],pid_t) {
    fileCertifiedRow=nil;fileCertifiedButton=nil
    let (remote,pid)=try ui32ProofStep("base-panel"){try panel()}
    if cancelOnly || command == "fixture.observe" {return (remote,pid)}
    try check(["fixture.choose","fixture.confirm","fixture.cancel"].contains(command))
    if pid == pinned.pid {
      try check(fileDomain == nil || fileDomain == "same-pid-original");fileDomain="same-pid-original";return (remote,pid)
    }
    let deadline=DispatchTime.now().uptimeNanoseconds+750_000_000
    _ = try ui32ProofStep("sheet-before-audit"){try attachedSheet()};try ui32ProofStep("file-before-audit"){try fileProof()}
    let own=try ui32ProofStep("nemo-audit"){try fileAudit(observedOwnedNodes,pinned.pid,deadline,5120)}
    guard let used=own["nativeCallsStarted"] as? Int,used >= 0,used <= 5120 else {throw ActionRefusal(diagnostics:["stage":"read-proof","proofStep":"shared-call-budget"])}
    let service=try ui32ProofStep("service-audit"){try fileAudit(remote,pid,deadline,5120-used)}
    try ui32ProofCheck(DispatchTime.now().uptimeNanoseconds < deadline && processStart(pid) == start && authenticatedPanelService(pid),"audit-deadline-service-identity")
    _ = try ui32ProofStep("sheet-after-audit"){try attachedSheet()}
    let domain=try ui32FileDomain(own,service,fileDomain);fileDomain=domain
    let owned=domain == "nemo-owned-sheet",chosenPID=owned ? pinned.pid : pid,chosenRows=owned ? observedOwnedNodes : remote
    if owned {
      guard let facts=own["nodes"] as? [[String:Any]],let index=facts.firstIndex(where:{$0["expectedURLMatch"] as? Bool == true}) else {throw Refusal(code:"adapter-refused")}
      let row=try fileNemoRow(observedOwnedNodes[index],deadline);fileCertifiedRow=row
      if let selected {try ui32ProofCheck(CFEqual(selected,row),"retained-selected-row")}
    } else {fileCertifiedRow=try ui32ProofStep("service-exact-row"){try exactRow(remote,pid)}}
    if command != "fixture.choose" {
      let labels:Set<String> = command == "fixture.confirm" ? ["Open","Ouvrir","開く","Abrir"] : ["Cancel","Annuler","キャンセル","Cancelar"]
      let button=try fileButton(labels,[(observedOwnedNodes,pinned.pid,own),(remote,pid,service)],chosenPID,deadline)
      try check(chosenRows.contains {CFEqual($0,button)});fileCertifiedButton=button
    }
    _ = try ui32ProofStep("final-sheet"){try attachedSheet()};try ui32ProofCheck(processStart(pid) == start && authenticatedPanelService(pid),"final-service-identity");return (chosenRows,chosenPID)
  }
}

func ui32OwnedParentRead<Node>(_ node:Node, deadline:UInt64, now:()->UInt64,
  owned:(Node)->Bool, prepare:(Float)->Int, read:()->(Int,Node?), record:([String:Any])->Void) -> (Int,Node?) {
  let begin=now();var last=begin, setup:Int?, query:Int?, value:Node?, reason="deadline", timeout:Float?
  func live() -> Bool {let t=now();let valid=t >= last && t < deadline;last=t;return valid}
  defer {
    let end=now()
    record(["setupStatus":setup as Any? ?? NSNull(),"parentStatus":query as Any? ?? NSNull(),
      "elapsedNs":end >= begin ? end-begin as Any : NSNull(),"remainingAtStartNs":begin < deadline ? deadline-begin : 0,
      "timeoutSeconds":timeout as Any? ?? NSNull(),"typedParent":value != nil,"reason":reason])
  }
  guard live(),owned(node),live() else {return (-1,nil)}
  let ns=min(UInt64(100_000_000),deadline-last), seconds=Double(ns)/1_000_000_000
  var bounded=Float(seconds);if Double(bounded) > seconds {bounded=bounded.nextDown}
  guard bounded.isFinite && bounded > 0 else {return (-1,nil)};timeout=bounded
  setup=prepare(bounded)
  guard setup == 0,live(),owned(node),live() else {reason="setup-owner-or-deadline";return (-1,nil)}
  let result=read();query=result.0;value=result.1
  guard live(),owned(node),live() else {reason="owner-or-deadline";return (query ?? -1,nil)}
  guard result.0 == 0,let parent=result.1 else {reason="parent-query-or-shape";return (result.0,nil)}
  guard live(),owned(parent),live() else {reason="parent-owner-or-deadline";return (-1,nil)}
  reason="returned";return (0,parent)
}
func ui32OwnedParentAX(_ node:AXUIElement, _ pid:pid_t, _ deadline:UInt64, _ record:([String:Any])->Void) -> (Int,AXUIElement?) {
  ui32OwnedParentRead(node,deadline:deadline,now:{DispatchTime.now().uptimeNanoseconds},owned:{owns($0,pid)},
    prepare:{Int(AXUIElementSetMessagingTimeout(node,$0).rawValue)},read:{
      var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
      return (Int(status.rawValue),axElement(raw))
    },record:record)
}
func ui32OwnedParentReadSelfTest() throws {
  for fault in ["none","clamp","expired","owner-budget","setup-budget","read-budget","owner-loss","cannot-complete","nil","foreign","setup-error","clock"] {
    var tick:UInt64=1,owners=0,setups=0,reads=0,facts=[String:Any](),bounded:Float=0
    let end:UInt64=fault == "expired" ? 0 : fault == "clamp" ? 10 : 1_000_000_000
    let result=ui32OwnedParentRead(1,deadline:end,now:{tick},owned:{node in
      owners += 1;if fault == "owner-budget" {tick=end}
      return !(fault == "owner-loss" && owners == 3) && !(fault == "foreign" && node == 0)
    },prepare:{seconds in
      setups += 1;bounded=seconds
      if fault == "setup-budget" {tick=end};return fault == "setup-error" ? -1 : 0
    },read:{reads += 1;if fault == "read-budget" {tick=end};if fault == "clock" {tick=0}
      return (fault == "cannot-complete" ? -25204 : 0,fault == "nil" ? nil : 0)
    },record:{facts=$0})
    try check((result.1 != nil) == ["none","clamp"].contains(fault) && reads <= 1)
    let noSetup=["expired","owner-budget"],noRead=noSetup+["setup-budget","setup-error"]
    try check(setups == (noSetup.contains(fault) ? 0 : 1) && reads == (noRead.contains(fault) ? 0 : 1))
    try check(setups == 0 || bounded > 0 && Double(bounded) <= min(0.1,Double(end-1)/1e9))
    try check(facts["parentStatus"] as? Int == (reads == 0 ? nil : fault == "cannot-complete" ? -25204 : 0))
  }
}
