import Foundation

// Complete forward census is a scope certificate, NOT a successful whole-window
// reciprocal walk. The common verifier remains mandatory inside the WebArea.
struct UI32ContentCertificate<Node> {
  let nodes:[Node], roles:[String], declared:[Int?], reported:[Int?]
  let web:Int, inside:Set<Int>, outsideMismatches:[Int]
}
func ui32ScopeCheck(_ ok:Bool, _ reason:String) throws {
  if !ok { throw ActionRefusal(diagnostics:["stage":"content-scope","reason":reason]) }
}
func ui32ContentCertificate<Node>(_ main:Node, pid:Int, retained:Node?, deadline:UInt64,
  now:()->UInt64, equal:(Node,Node)->Bool, owner:(Node)->(Int,Bool,Int), prepare:(Node)->Bool,
  traits:(Node) throws -> (String,Bool), children:(Node)->(Int,[Node]?), parent:(Node)->(Int,Node?),
  strict:(Node,UInt64) throws -> [Node]) throws -> UI32ContentCertificate<Node> {
  var nodes=[Node](), roles=[String](), modals=[Bool](), declared=[Int?](), reportedRefs=[Node?]()
  func owned(_ node:Node) throws {
    let (error,matches,actual)=owner(node)
    try ui32ScopeCheck(error == 0 && matches && actual == pid,"owner")
  }
  func visit(_ node:Node, _ ancestor:Int?, _ depth:Int) throws {
    try ui32ScopeCheck(now() < deadline,"deadline")
    try ui32ScopeCheck(depth <= 12 && nodes.count < 512,"bounds")
    try owned(node);try ui32ScopeCheck(!nodes.contains {equal($0,node)},"cycle-or-alias")
    try ui32ScopeCheck(prepare(node),"timeout")
    let (role,modal)=try traits(node), index=nodes.count
    try ui32ScopeCheck(!role.isEmpty && (index == 0 ? role == "AXWindow" : role != "AXWindow"),"window-role")
    nodes.append(node);roles.append(role);modals.append(modal);declared.append(ancestor);reportedRefs.append(nil)
    if ancestor != nil {
      let (query,observed)=parent(node)
      guard query == 0 && observed != nil else {
        throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"parent-query",
          "queryError":query,"parentPresent":observed != nil,"nodeIndex":index,"depth":depth,
          "role":role,"declaredParentIndex":ancestor!,"queriedNodeOwnerVerified":true])
      }
      try owned(observed!);reportedRefs[index]=observed!
    }
    let (query,rows)=children(node)
    try ui32ScopeCheck(query == 0 && rows != nil,"children-query")
    try ui32ScopeCheck(rows!.count <= 512,"children-count")
    for child in rows! { try visit(child,index,depth+1) }
    try ui32ScopeCheck(now() < deadline,"deadline")
  }
  try visit(main,nil,0)
  let candidates=roles.indices.filter {roles[$0] == "AXWebArea"}
  do { try ui32ScopeCheck(candidates.count == 1,"webarea-count") }
  catch let error as ActionRefusal {
    // This complete bounded census is already authenticated; no new AX reads.
    let categories=["AXWebArea","AXGroup","AXScrollArea","AXSplitGroup","AXButton","AXWindow"]
    var counts=Dictionary(uniqueKeysWithValues:categories.map {($0,0)})
    counts["other"]=0
    for role in roles { counts[categories.contains(role) ? role : "other",default:0] += 1 }
    var facts=error.diagnostics
    facts["webAreaCount"]=candidates.count;facts["ownedWindowNodeCount"]=nodes.count;facts["roleCounts"]=counts;facts["nativeModalCount"]=modals.filter{$0}.count
    throw ActionRefusal(diagnostics:facts)
  }
  let web=candidates[0]
  if let retained { try ui32ScopeCheck(equal(nodes[web],retained),"webarea-replaced") }
  var reported=[Int?](repeating:nil,count:nodes.count)
  for i in nodes.indices where i != 0 {
    guard let ref=reportedRefs[i],let actual=nodes.firstIndex(where:{equal($0,ref)}) else {
      throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"parent-outside-window"])
    }
    reported[i]=actual
  }
  var cursor=web
  while cursor != 0 {
    guard let next=declared[cursor] else { throw Refusal(code:"adapter-refused") }
    try ui32ScopeCheck(reported[cursor] == next,"webarea-root-path");cursor=next
  }
  var inside=Set<Int>()
  for i in nodes.indices {
    var current:Int?=i
    while let value=current {
      if value == web { inside.insert(i);break };current=declared[value]
    }
  }
  for i in nodes.indices {
    try ui32ScopeCheck(roles[i] != "AXSheet" && (inside.contains(i) || !modals[i]),"native-modal-present")
  }
  // Use the original strict verifier with actual AXParent, never the declared
  // parent. Compare its complete membership/order to the already owned census.
  let content:[Node], expected=nodes.indices.filter {inside.contains($0)}
  do { content=try strict(nodes[web],deadline) }
  catch let error as ActionRefusal {
    var facts=error.diagnostics;facts["insideContent"]=true;throw ActionRefusal(diagnostics:facts)
  }
  func changed(_ comparison:String, _ offset:Int? = nil) -> ActionRefusal {
    var facts:[String:Any]=["stage":"content-scope","reason":"content-changed","comparison":comparison,
      "forwardContentCount":expected.count,"strictContentCount":content.count,"ownedWindowNodeCount":nodes.count,
      "fullStrictReadComplete":true,"retainedWebAreaMatched":retained != nil]
    if let offset {facts["mismatchIndex"]=offset};return ActionRefusal(diagnostics:facts)
  }
  if content.count != expected.count {throw changed("count")}
  for (offset,pair) in zip(content,expected).enumerated() {
    let (node,index)=pair;try ui32ScopeCheck(now() < deadline,"deadline")
    if !equal(node,nodes[index]) {throw changed("node-order",offset)};try owned(node)
    let (query,ref)=parent(node)
    try ui32ScopeCheck(query == 0 && ref != nil,"content-parent-query");try owned(ref!)
    try ui32ScopeCheck(equal(ref!,nodes[declared[index]!]),"content-parent-changed")
  }
  try ui32ScopeCheck(now() < deadline,"deadline")
  let (currentRole,_)=try traits(nodes[web]);try ui32ScopeCheck(currentRole == "AXWebArea","webarea-role-changed")
  try ui32ScopeCheck(now() < deadline,"deadline")
  let outside=nodes.indices.filter {$0 != 0 && !inside.contains($0) && reported[$0] != declared[$0]}
  return UI32ContentCertificate(nodes:nodes,roles:roles,declared:declared,reported:reported,web:web,inside:inside,outsideMismatches:outside)
}
func ui32ScopedCandidate(_ inside:[Int], _ outside:[Int]) throws -> Int {
  try ui32ScopeCheck(outside.isEmpty,"candidate-outside-content")
  try ui32ScopeCheck(inside.count == 1,"candidate-count");return inside[0]
}

