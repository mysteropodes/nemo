// Actual owned AX child-array/count binding, with injectable ports for static
// compiled oracles. It owns no UI mutation, cache or persistent witness.
import Foundation
import ApplicationServices

func ownedChildrenCountResult(_ query: AXError, _ count: CFIndex) -> OwnedChildrenCountQuery {
  if query == .success { return count == 0 ? .zero : count > 0 ? .positive : .invalid }
  if query == .attributeUnsupported { return .unsupported }
  if query == .noValue { return .noValue }
  return .failed
}

func queryOwnedChildren<Node>(_ node: Node, expectedOwner: Int,
  owner: (Node)->Int?, value: (Node)->(OwnedChildrenQuery,[Node]?,Bool),
  count: (Node)->OwnedChildrenCountQuery) -> OwnedChildren<Node> {
  func ownership() -> OwnedOwnerQuery { OwnedOwnerQuery(owner(node),expectedOwner) }
  func refused(_ failure: OwnedOwnerQuery) -> OwnedChildren<Node> {
    OwnedChildren(query:.failed,values:nil,ownerRefusal:failure)
  }
  let beforeValue = ownership()
  guard beforeValue == .matched else { return refused(beforeValue) }
  let (query,values,present) = value(node)
  let afterValue = ownership()
  guard afterValue == .matched else { return refused(afterValue) }
  guard query == .noValue else { return OwnedChildren(query:query,values:values) }
  // A contradictory value cannot become an empty proof, even if count is zero.
  guard !present && values == nil else { return OwnedChildren(query:.noValue,values:nil,countQuery:.invalid) }
  let beforeCount = ownership()
  guard beforeCount == .matched else { return refused(beforeCount) }
  let observed = count(node)
  let afterCount = ownership()
  guard afterCount == .matched else { return refused(afterCount) }
  return OwnedChildren(query:.noValue,values:nil,countQuery:observed)
}

func ownedAXChildren(_ node: AXUIElement, expectedOwner: Int) -> OwnedChildren<AXUIElement> {
  queryOwnedChildren(node,expectedOwner:expectedOwner,
    owner:{ current in var actual:pid_t = 0; return AXUIElementGetPid(current,&actual) == .success ? Int(actual) : nil },
    value:{ current in
      var raw: CFTypeRef?
      let query = AXUIElementCopyAttributeValue(current,kAXChildrenAttribute as CFString,&raw)
      let status: OwnedChildrenQuery = query == .success ? .success : query == .attributeUnsupported ? .unsupported : query == .noValue ? .noValue : .failed
      return (status,raw as? [AXUIElement],raw != nil)
    },count:{ current in
      var size: CFIndex = -1
      let query = AXUIElementGetAttributeValueCount(current,kAXChildrenAttribute as CFString,&size)
      return ownedChildrenCountResult(query,size)
    })
}

func ownedChildrenQuerySelfTest() throws {
  var reads = 0, counts = 0
  let positive: OwnedChildren<Int> = queryOwnedChildren(1,expectedOwner:10,owner:{_ in 10},
    value:{_ in reads += 1; return (.noValue,nil,false)},count:{_ in counts += 1; return .zero})
  try check(positive.query == .noValue && positive.values?.isEmpty == true && positive.witness == "empty-count-confirmed")
  for outcome in [OwnedChildrenCountQuery.positive,.unsupported,.noValue,.failed,.invalid] {
    let result: OwnedChildren<Int> = queryOwnedChildren(1,expectedOwner:10,owner:{_ in 10},
      value:{_ in (.noValue,nil,false)},count:{_ in outcome})
    try check(result.values == nil && result.witness == nil)
  }
  let foreign: OwnedChildren<Int> = queryOwnedChildren(2,expectedOwner:10,owner:{_ in 99},
    value:{_ in reads += 1; return (.noValue,nil,false)},count:{_ in counts += 1; return .zero})
  try check(foreign.ownerRefusal == .mismatch && reads == 1 && counts == 1)
  var checks = 0
  let stale: OwnedChildren<Int> = queryOwnedChildren(1,expectedOwner:10,owner:{_ in checks += 1; return checks == 4 ? nil : 10},
    value:{_ in (.noValue,nil,false)},count:{_ in .zero})
  try check(stale.ownerRefusal == .failed && stale.values == nil && stale.witness == nil)
}
