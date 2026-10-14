// Owned AX projection and retained direct-child pages; no global input or state.
import Foundation
import CoreFoundation

// Inspection and fresh input use the same public, ordered control classifier.
func ownedControl(_ reference: Bool, _ title: String, _ value: String, _ description: ()->String?) -> String? {
  var labels: Set<String> = ["Open","Open Project…","Cancel","A","B","A.json","B.json","Motion","Opacity","R08 rectangle"]
  if reference { labels.formUnion(["File","Edit","Save","Save As…","Save As","Save as…","Open…","Undo","Redo",
    "Reference","Reference…","None","No reference","Visible","Remove","Import Reference Image",
    "ReferenceA.png","ReferenceB.png","Saved.json","frame_0001.png","Export","Export…","Export PNG","PNG","PNG Sequence",
    "SVG Sequence","Work area","Whole timeline","1x","Choose","Select","Export Image","Replace",
    "Go","Go to the folder:","Go to folder:","Go to Folder","Go to the folder"]) }
  if labels.contains(title) { return title }
  if labels.contains(value) { return value }
  if let text = description(), labels.contains(text) { return text }
  return nil
}

struct OwnedProjectionCursor<Node> {
  let token: String
  let base: String
  let root: Node
  let children: [Node]
  let owner: Int
  var next: Int
  var foreign = false
  var unreadable = false
  var depthClipped = false
}

func ownedProjection<Node>(_ root: Node, base: String, depth: Int, maxNodes: Int,
  expectedOwner: Int, request: [String:Any], cursor: inout OwnedProjectionCursor<Node>?,
  equal: (Node,Node)->Bool, hashKey: (Node)->UInt, owner: (Node)->Int?, children: (Node)->[Node]?,
  isWindow: (Node)->Bool, fact: (Node,String)->[String:Any],
  phase: (String,String)->Void) throws -> [String:Any] {
  try check(depth >= 1 && depth <= 5 && maxNodes >= 1 && maxNodes <= 128)
  let paged = request["childOffset"] != nil
  var offset = 0, token: String? = nil
  if paged {
    guard let raw = request["childOffset"] as? NSNumber,
      CFGetTypeID(raw) != CFBooleanGetTypeID(),
      let value = request["childOffset"] as? Int else { throw Refusal(code:"adapter-refused") }
    try check(raw.doubleValue == Double(value) && value >= 0 && value < 4096 && depth == 1 && maxNodes >= 2)
    offset = value
    if let rawToken = request["pageToken"] {
      guard let supplied = rawToken as? String, UUID(uuidString:supplied) != nil else { throw Refusal(code:"adapter-refused") }
      token = supplied
    }
  } else {
    try check(request["pageToken"] == nil)
    cursor = nil
  }
  var nodes = [[String:Any]](), truncated = false, depthClipped = false, unreadable = false, foreign = false
  func owned(_ node: Node) -> Bool {
    phase("ax-owner-query","ownership-refused")
    return owner(node) == expectedOwner
  }
  guard owned(root) else { cursor = nil; throw Refusal(code:"child-owner") }
  phase("ax-children-query","attribute-value-unreadable")
  let rootChildren = children(root)
  if let list = rootChildren { try check(list.count < 4096) }
  if paged {
    guard let list = rootChildren else { cursor = nil; throw Refusal(code:"children-unavailable") }
    try check(offset <= list.count)
    if offset == 0 {
      try check(token == nil)
      var seen = [UInt:[Node]]()
      for child in list {
        let key = hashKey(child)
        try check(!(seen[key] ?? []).contains { equal($0,child) })
        seen[key,default:[]].append(child)
      }
      cursor = OwnedProjectionCursor(token:UUID().uuidString.lowercased(),base:base,root:root,children:list,owner:expectedOwner,next:0)
    } else {
      guard let pinned = cursor, token == pinned.token, pinned.next == offset,
        pinned.base == base, pinned.owner == expectedOwner, equal(pinned.root,root),
        pinned.children.count == list.count,
        zip(pinned.children,list).allSatisfy({ equal($0.0,$0.1) }) else {
        cursor = nil; throw Refusal(code:"adapter-refused")
      }
    }
  }
  func project(_ current: Node, _ path: String, _ level: Int) throws {
    guard owned(current) else {
      foreign = true; nodes.append(["path":path,"role":"other","blocked":"child-owner"]); return
    }
    if level > 0 && isWindow(current) { throw Refusal(code:"standalone-window") }
    phase("ax-attribute-query","attribute-query-failed")
    nodes.append(fact(current,path))
    guard owned(current) else {
      foreign = true; nodes[nodes.count-1] = ["path":path,"role":"other","blocked":"child-owner"]; return
    }
    phase("ax-children-query","attribute-value-unreadable")
    let list = level == 0 ? rootChildren : children(current)
    guard let list else {
      unreadable = true
      if level < depth { nodes[nodes.count-1]["blocked"] = "children-unavailable" }
      else { depthClipped = true }
      return
    }
    guard owned(current) else {
      foreign = true; nodes[nodes.count-1] = ["path":path,"role":"other","blocked":"child-owner"]; return
    }
    phase("ax-projection","topology-refused")
    try check(list.count < 4096)
    guard level < depth else { if !list.isEmpty { depthClipped = true }; return }
    for (index,child) in list.enumerated().dropFirst(level == 0 ? offset : 0) {
      if nodes.count >= maxNodes { truncated = true; return }
      let childPath = path.isEmpty ? String(index) : path+"."+String(index)
      try project(child,childPath,level+1)
    }
  }
  try project(root,base,0)
  guard owned(root) else { cursor = nil; throw Refusal(code:"child-owner") }
  if paged {
    guard let pinned = cursor, let now = children(root),
      now.count == pinned.children.count,
      zip(pinned.children,now).allSatisfy({ equal($0.0,$0.1) }) else {
      cursor = nil; throw Refusal(code:"adapter-refused")
    }
  }
  var result: [String:Any] = ["nodes":nodes,"truncated":truncated,"depthClipped":depthClipped,
    "childrenUnreadable":unreadable,"foreignChildren":foreign,
    "complete":!truncated && !depthClipped && !unreadable && !foreign]
  if paged, var pinned = cursor {
    pinned.foreign = pinned.foreign || foreign
    pinned.unreadable = pinned.unreadable || unreadable
    pinned.depthClipped = pinned.depthClipped || depthClipped
    result["foreignChildren"] = pinned.foreign
    result["childrenUnreadable"] = pinned.unreadable
    result["depthClipped"] = pinned.depthClipped
    pinned.next = min(pinned.children.count,offset+nodes.count-1)
    result["pageToken"] = pinned.token
    result["childOffset"] = offset
    result["nextChildOffset"] = pinned.next
    result["childCount"] = pinned.children.count
    result["childrenComplete"] = pinned.next == pinned.children.count && !pinned.foreign
    result["complete"] = pinned.next == pinned.children.count && !pinned.foreign && !pinned.unreadable && !pinned.depthClipped
    cursor = pinned.next == pinned.children.count ? nil : pinned
  }
  return result
}
