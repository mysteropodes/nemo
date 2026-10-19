// Three reviewed fixed fixture presets; no caller-selected filename, bytes or route.
import Foundation
import ApplicationServices
import CryptoKit

func ui32FixtureURLMatches(_ observed:URL?, _ expected:URL) -> Bool {
  observed?.isFileURL == true && observed?.standardizedFileURL == expected
}
// Records only results already returned to the unchanged shared walkers.
final class UI32PanelTrace<Node> {
  let kind:String, expected:Int, equal:(Node,Node)->Bool
  var nodes:[Node], routes:[[String]], declared:[[Int]], owners:[String]
  var entered=Set<Int>(), current:Int?, phase="none", reported:Node?, query=0, foreignRoots=0, truncated=false
  var parentRead=[String:Any]()
  init(_ kind:String, _ root:Node, _ expected:Int, equal:@escaping (Node,Node)->Bool) {
    self.kind=kind;self.expected=expected;self.equal=equal
    nodes=[root];routes=[[""]];declared=[[]];owners=["not-queried"]
  }
  func index(_ node:Node) -> Int? { nodes.firstIndex {equal($0,node)} }
  func ownership(_ node:Node, _ value:(Int,Bool,Int)) -> (Int,Bool,Int) {
    current=index(node);phase="owner";reported=nil;query=value.0
    if let i=current {owners[i]=value.0 != 0 ? "query-error" : value.1 && value.2 == expected ? "owned" : "foreign"}
    return value
  }
  func children(_ node:Node, _ value:(Int,[Node]?)) -> (Int,[Node]?) {
    current=index(node);phase="children";reported=nil;query=value.0
    if let i=current {
      entered.insert(i)
      if value.0 == 0,let rows=value.1 {
        for (offset,child) in rows.prefix(512).enumerated() {
          let route=(routes[i].first ?? "").isEmpty ? String(offset) : routes[i][0]+"."+String(offset)
          if let j=index(child) {
            if routes[j].count < 2 {routes[j].append(route)} else {truncated=true}
            if !declared[j].contains(i) {declared[j].append(i)}
          } else if nodes.count < 513 {
            nodes.append(child);routes.append([route]);declared.append([i]);owners.append("not-queried")
          } else {truncated=true}
        }
      }
    }
    return value
  }
  func parent(_ node:Node, _ value:(Int,Node?)) -> (Int,Node?) {
    current=index(node);phase="parent";reported=value.1;query=value.0;return value
  }
  func failure(_ error:ActionRefusal, describe:(Node)->[String:Any] = {_ in [:]}) -> ActionRefusal {
    var facts=error.diagnostics, context:[String:Any]=["basis":"last-shared-walk-port-result","walk":kind,"phase":phase,"knownNodeCount":nodes.count,
      "enteredNodeCount":entered.count,"queryError":query,"metadataTruncated":truncated,"foreignRootCount":foreignRoots,
      "completedForeignSearch":error.diagnostics["ownedSearchComplete"] as? Bool == true || (kind == "attached-foreign-root" && error.diagnostics["stage"] as? String == "candidate-count")]
    var references:[String:Any]=[:]
    func record(_ name:String, _ node:Node?) {
      guard let node else {references[name]=["relation":"unavailable"];return}
      let i=index(node);var value:[String:Any]=["knownIndex":i ?? -1,"owner":i.map {owners[$0]} ?? "not-queried"]
      if let i {value["routes"]=routes[i];if owners[i] == "owned" {value["categories"]=describe(node)}}
      references[name]=value
    }
    record("root",nodes[0]);record("child",current.map {nodes[$0]})
    if let i=current {
      context["routes"]=routes[i];context["depths"]=routes[i].map {$0.isEmpty ? 0 : $0.split(separator:".").count}
      context["declaredParentIndices"]=declared[i]
      record("declaredParent",declared[i].count == 1 ? nodes[declared[i][0]] : nil)
      if phase == "parent" {
        context["reportedMatchesChild"]=reported.map {equal($0,nodes[i])} ?? false
        context["reportedMatchesDeclared"]=reported.map {p in declared[i].contains {equal(p,nodes[$0])}} ?? false
      }
    }
    record("reportedParent",reported);context["references"]=references
    if !parentRead.isEmpty {context["lastParentRead"]=parentRead};facts["panelTraversal"]=context
    return ActionRefusal(diagnostics:facts)
  }
}

final class UI32Fixture {
  let pinned:OwnedInitialWindow
  let fixture:URL
  var attempted=false, completed=false, confirmed=false
  let cancelOnly:Bool
  let fixtureSpec:(name:String,size:Int,hash:String,f03:Bool)
  let afterDismissal:() throws -> Void
  var sheet:AXUIElement?, root:AXUIElement?, parent:AXUIElement?, owner:pid_t?, start:ProcessStart?
  var selected:AXUIElement?;var fileDomain:String?, fileCertifiedRow:AXUIElement?, fileCertifiedButton:AXUIElement?
  var attachmentProof=[String:Any]()
  var observedOwnedNodes=[AXUIElement](),observedOwnedRoutes=[String]()
  init(_ pinned:OwnedInitialWindow, _ environment:[String:String], cancelOnly:Bool=false, afterDismissal:@escaping () throws -> Void) throws {
    self.pinned=pinned;self.cancelOnly=cancelOnly;self.afterDismissal=afterDismissal
    self.fixtureSpec=try ui32FixedFixture(environment["NEMO_UI32_FIXTURE_KIND"])
    guard let path=environment["NEMO_UI32_FIXTURE"], path.hasPrefix("/") else { throw Refusal(code:"adapter-refused") }
    fixture=URL(fileURLWithPath:path).standardizedFileURL
    try fileProof()
  }
  func fileProof() throws {
    try check(fixture.resolvingSymlinksInPath() == fixture && fixture.lastPathComponent == fixtureSpec.name)
    let attributes=try FileManager.default.attributesOfItem(atPath:fixture.path)
    try check(attributes[.type] as? FileAttributeType == .typeRegular && (attributes[.size] as? NSNumber)?.intValue == fixtureSpec.size)
    let bytes=try Data(contentsOf:fixture);try check(SHA256.hash(data:bytes).map {String(format:"%02x",$0)}.joined() == fixtureSpec.hash)
  }
  func ownedTree(_ root:AXUIElement, _ pid:pid_t) throws -> [AXUIElement] {
    let trace=UI32PanelTrace("bounded-owned-tree",root,Int(pid),equal:{CFEqual($0,$1)})
    do { return try boundedOwnedTree(root,owner:Int(pid),deadline:DispatchTime.now().uptimeNanoseconds+1_000_000_000,
      now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:{ node in
        var actual:pid_t=0;let q=AXUIElementGetPid(node,&actual)
        return trace.ownership(node,(Int(q.rawValue),q == .success && actual == pid,Int(actual)))
      },children:{ node in
        trace.children(node,ui32Children(node,pid))
      },parent:{ node in
        guard AXUIElementSetMessagingTimeout(node,0.03) == .success else { return trace.parent(node,(-1,nil)) }
        var raw:CFTypeRef?;let q=AXUIElementCopyAttributeValue(node,kAXParentAttribute as CFString,&raw)
        return trace.parent(node,(Int(q.rawValue),axElement(raw)))
      }) } catch let error as ActionRefusal { throw panelFailure(error,trace,pid) }
  }
  func panelFailure(_ error:ActionRefusal, _ trace:UI32PanelTrace<AXUIElement>, _ pid:pid_t) -> ActionRefusal {
    let deadline=DispatchTime.now().uptimeNanoseconds+250_000_000
    let enriched=trace.failure(error,describe:{node in
      var facts:[String:Any]=[:]
      for key in [kAXRoleAttribute,kAXSubroleAttribute] {
        guard DispatchTime.now().uptimeNanoseconds < deadline,owns(node,pid),
          AXUIElementSetMessagingTimeout(node,0.03) == .success,owns(node,pid) else {facts["status"]="unreadable-or-owner-changed";return facts}
        var raw:CFTypeRef?;let q=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
        guard owns(node,pid) else {facts["status"]="owner-changed";return facts}
        let allowed=key == kAXRoleAttribute ? ["AXSheet","AXWindow","AXGroup","AXButton","AXSplitGroup","AXScrollArea","AXOutline","AXRow","AXCell","AXTextField","AXStaticText"] : ["AXDialog","AXSystemDialog","AXStandardWindow"]
        let value=raw as? String
        facts[key == kAXRoleAttribute ? "role" : "subrole"]=q == .success && value != nil ? (allowed.contains(value!) ? value! : "other") : "unreadable"
        facts[key == kAXRoleAttribute ? "roleQuery" : "subroleQuery"]=Int(q.rawValue)
      }
      return facts
    })
    var facts=enriched.diagnostics, context=facts["panelTraversal"] as? [String:Any] ?? [:]
    context["reportedParentMatchesPinnedMain"]=trace.reported.map {CFEqual($0,pinned.main)} ?? false
    context["reportedParentMatchesRetainedSheet"]=trace.reported.map {node in sheet.map {CFEqual(node,$0)} ?? false} ?? false
    context["rootMatchesPinnedMain"]=CFEqual(trace.nodes[0],pinned.main)
    context["rootMatchesRetainedSheet"]=sheet.map {CFEqual(trace.nodes[0],$0)} ?? false
    facts["panelTraversal"]=context;if !attachmentProof.isEmpty {facts["attachmentCertificate"]=attachmentProof};return ActionRefusal(diagnostics:facts)
  }
  func attachedSheet() throws -> AXUIElement {
    try verifyOwnedWindow(pinned)
    guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement],
      let children=attr(pinned.main,kAXChildrenAttribute) as? [AXUIElement], windows.count <= 8, children.count <= 512 else { throw Refusal(code:"sheet-ambiguous") }
    var found=[AXUIElement](), roles=[String:Int]()
    for node in windows+children {
      try check(owns(node,pinned.pid))
      guard let role=attr(node,kAXRoleAttribute) as? String else {
        throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"fixture-sheet-role-unreadable"])
      }
      let category=["AXWindow","AXSheet","AXGroup"].contains(role) ? role : "other"
      roles[category,default:0] += 1
      if role != kAXSheetRole { continue }
      guard let ancestor=axElement(attr(node,kAXParentAttribute)), owns(ancestor,pinned.pid), CFEqual(ancestor,pinned.main) else { throw Refusal(code:"sheet-parent") }
      if !found.contains(where:{CFEqual($0,node)}) { found.append(node) }
    }
    guard found.count == 1 else {
      throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"fixture-sheet-count","sheetCount":found.count,
        "applicationWindowCount":windows.count,"mainDirectChildCount":children.count,"observedRoleCounts":roles])
    }
    if let sheet { try check(CFEqual(sheet,found[0])) } else { sheet=found[0] }
    return found[0]
  }
  func panel() throws -> ([AXUIElement],pid_t) {
    attachmentProof=[:];observedOwnedNodes=[];observedOwnedRoutes=[];let outer=try attachedSheet();var foreign=[Int]()
    let trace=UI32PanelTrace("attached-candidate-certificate",outer,Int(pinned.pid),equal:{CFEqual($0,$1)})
    let parentDeadline=DispatchTime.now().uptimeNanoseconds+1_000_000_000
    var currentRoot=outer, currentParent=pinned.main, currentOwner=pinned.pid, currentStart=pinned.start
    do {
      let found=try ui32AttachedRootCertificate(outer,owner:Int(pinned.pid),deadline:parentDeadline,
        now:{DispatchTime.now().uptimeNanoseconds},equal:{CFEqual($0,$1)},owner:{ node in
          var actual:pid_t=0;let q=AXUIElementGetPid(node,&actual)
          return trace.ownership(node,(Int(q.rawValue),q == .success && actual == self.pinned.pid,Int(actual)))
        },children:{ node in
          trace.children(node,ui32Children(node,self.pinned.pid))
        },parent:{ node in
          trace.parent(node,ui32OwnedParentAX(node,self.pinned.pid,parentDeadline,{trace.parentRead=$0}))
        },foreignChild:{ _,pid in foreign.append(pid);trace.foreignRoots += 1 },
        completedOwnedCensus:{nodes,routes in self.observedOwnedNodes=nodes;self.observedOwnedRoutes=routes})
      attachmentProof=["ownedSearchComplete":true,"ownedNodeCount":found.ownedNodeCount,"foreignRootCount":1,
        "candidateRoute":found.route,"ownedPathRoutes":found.ownedPathRoutes,"outsideMismatches":found.outsideMismatches,
        "scope":"Discovery only; service authentication and strict panel/effect guards still required."]
      let pid=pid_t(found.pid)
      guard let stamp=processStart(pid), authenticatedPanelService(pid), processStart(pid) == stamp else { throw Refusal(code:"sheet-owner") }
      try check(AXUIElementSetMessagingTimeout(found.root,0.03) == .success)
      guard let observedParent=axElement(attr(found.root,kAXParentAttribute)), owns(observedParent,pinned.pid) else { throw Refusal(code:"sheet-parent") }
      let reaches=ownedAncestor(observedParent,outer,equal:{CFEqual($0,$1)},owned:{owns($0,self.pinned.pid)},
        isWindow:{attr($0,kAXRoleAttribute) as? String == kAXWindowRole},parent:{axElement(attr($0,kAXParentAttribute))},children:{attr($0,kAXChildrenAttribute) as? [AXUIElement]})
      try check(attachedServiceParentValid(CFEqual(observedParent,found.parent),pinned.pid,pinned.pid,reaches))
      currentRoot=found.root;currentParent=observedParent;currentOwner=pid;currentStart=stamp
    } catch let error as ActionRefusal {
      // Only a completed walk proving ZERO foreign roots may use same-PID AX.
      guard ui32ZeroForeignFallback(foreign.count,error.diagnostics) else { throw panelFailure(error,trace,pinned.pid) }
      attachmentProof=["ownedSearchComplete":true,"foreignRootCount":0,"ownedParentMismatchCount":0,"samePIDFallback":true]
    }
    if let root, let parent, let owner, let start {
      try check(CFEqual(root,currentRoot) && CFEqual(parent,currentParent) && owner == currentOwner && start == currentStart)
    } else { root=currentRoot;parent=currentParent;owner=currentOwner;start=currentStart }
    let rows=try ownedTree(currentRoot,currentOwner)
    try verifyOwnedWindow(pinned);_ = try attachedSheet()
    try check(processStart(currentOwner) == currentStart && (currentOwner == pinned.pid || authenticatedPanelService(currentOwner)))
    return (rows,currentOwner)
  }
  func exactRow(_ rows:[AXUIElement], _ pid:pid_t) throws -> AXUIElement {
    let matches=rows.filter { node in
      guard owns(node,pid) else { return false }
      return ui32FixtureURLMatches(attr(node,kAXURLAttribute) as? URL,fixture)
    }
    guard matches.count == 1 else { throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"fixture-url-match","countBucket":min(matches.count,2)]) }
    var current=matches[0],visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
      if attr(current,kAXRoleAttribute) as? String == kAXRowRole { return current }
      guard let parent=axElement(attr(current,kAXParentAttribute)), owns(parent,pid),
        let children=attr(parent,kAXChildrenAttribute) as? [AXUIElement],children.count <= 512,
        children.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"sheet-parent") }
      current=parent
    }
    throw Refusal(code:"adapter-refused")
  }
  func singleSelectedFile(_ rows:[AXUIElement], _ pid:pid_t, _ expected:AXUIElement) throws {
    try check(CFEqual(try exactRow(rows,pid),expected) && rows.contains {CFEqual($0,expected)})
    var current=expected,outline:AXUIElement?,visited=[AXUIElement]()
    for _ in 0..<20 {
      try check(owns(current,pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
      if attr(current,kAXRoleAttribute) as? String == kAXOutlineRole { outline=current;break }
      guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pid),
        let children=ownedAXChildren(parent,expectedOwner:Int(pid)).values,children.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"sheet-parent") }
      current=parent
    }
    guard let outline,rows.contains(where:{CFEqual($0,outline)}) else { throw Refusal(code:"adapter-refused") }
    var rawRows:CFTypeRef?,rawSelection:CFTypeRef?
    try check(AXUIElementCopyAttributeValue(outline,kAXRowsAttribute as CFString,&rawRows) == .success)
    try check(AXUIElementCopyAttributeValue(outline,"AXSelectedRows" as CFString,&rawSelection) == .success)
    guard let files=rawRows as? [AXUIElement], let selection=rawSelection as? [AXUIElement] else { throw Refusal(code:"sheet-ambiguous") }
    try ui32SingleSelection(files,selection,expected,equal:{CFEqual($0,$1)},owned:{ node in owns(node,pid) && rows.contains {CFEqual($0,node)} },selected:{ file in
      var raw:CFTypeRef?;try check(AXUIElementCopyAttributeValue(file,kAXSelectedAttribute as CFString,&raw) == .success)
      guard let selected=raw as? Bool else { throw Refusal(code:"adapter-refused") };return selected
    })
  }
  func perform(_ requested:String) throws -> [String:Any] {
    if cancelOnly { try check(["sync.panel-observe","sync.cancel-only","sync.complete"].contains(requested)) }
    else { try check(requested.hasPrefix("fixture.")) }
    let command=requested == "sync.cancel-only" ? "fixture.cancel" : requested == "sync.complete" ? "fixture.complete" : requested == "sync.panel-observe" ? "fixture.observe" : requested
    try check(attempted && !completed);try fileProof();try verifyOwnedWindow(pinned)
    if command == "fixture.complete" {
      guard let windows=attr(pinned.application,kAXWindowsAttribute) as? [AXUIElement] else { throw Refusal(code:"sheet-active") }
      let absent=windows.count == 1 && owns(windows[0],pinned.pid) && CFEqual(windows[0],pinned.main)
      try ui32FinishAfterDismissal(absent,certificate:afterDismissal,finish:{completed=true})
      return ["complete":true,"fixtureConfirmed":confirmed,"note":"Requires actual native SDK fixture baseline; not acceptance."]
    }
    let (rows,pid)=try actionPanel(command)
    if command == "fixture.observe" {
      _ = try attachedSheet()
      let ownedSheet=try ui32OwnedSheetAXRead(observedOwnedNodes,observedOwnedRoutes,pinned.pid,fixture)
      _ = try attachedSheet()
      return ["panelOwner":pid,"attachmentCertificate":attachmentProof,"ownedSheetDiagnostic":ownedSheet,"nodes":rows.map { node -> [String:Any] in
        ["role":attr(node,kAXRoleAttribute) as? String ?? "", "title":String((attr(node,kAXTitleAttribute) as? String ?? "").prefix(120)),
         "value":String((attr(node,kAXValueAttribute) as? String ?? "").prefix(120))]
      }]
    }
    var attemptedAction=false, acknowledgedAction=false
    do {
      if command == "fixture.choose" {
        try check(selected == nil)
        let before=try exactRow(rows,pid);try fileBind(before,pid)
        let (latest,latestPID)=try actionPanel(command);try check(latestPID == pid)
        let row=try exactRow(latest,pid);try fileBind(row,pid);try check(CFEqual(row,before))
        var writable=DarwinBoolean(false)
        try check(AXUIElementIsAttributeSettable(row,kAXSelectedAttribute as CFString,&writable) == .success && writable.boolValue)
        try fileProof();attemptedAction=true
        let query=AXUIElementSetAttributeValue(row,kAXSelectedAttribute as CFString,kCFBooleanTrue);acknowledgedAction=query == .success
        try ui32ProofCheck(query == .success,"fixture-row-selection-post",["queryError":Int(query.rawValue)])
        selected=row;let (fresh,freshPID)=try actionPanel(command);try check(freshPID == pid)
        try fileBind(row,pid);try singleSelectedFile(fresh,pid,row)
      } else {
        try check(command == "fixture.confirm" || command == "fixture.cancel")
        if command == "fixture.confirm" {
          guard let selected else { throw Refusal(code:"adapter-refused") }
          try fileBind(selected,pid);try check(CFEqual(try exactRow(rows,pid),selected) && attr(selected,kAXSelectedAttribute) as? Bool == true)
        }
        let labels:Set<String> = command == "fixture.confirm" ? ["Open","Ouvrir","開く","Abrir"] : ["Cancel","Annuler","キャンセル","Cancelar"]
        let buttons=rows.filter { attr($0,kAXRoleAttribute) as? String == kAXButtonRole && labels.contains(attr($0,kAXTitleAttribute) as? String ?? "") && attr($0,kAXEnabledAttribute) as? Bool == true }
        try check(buttons.count == 1);try fileBind(buttons[0],pid,button:true);var actions:CFArray?
        try check(AXUIElementCopyActionNames(buttons[0],&actions) == .success && (actions as? [String] ?? []).contains(kAXPressAction as String))
        let (latest,latestPID)=try actionPanel(command);try check(latestPID == pid)
        let freshButtons=latest.filter { attr($0,kAXRoleAttribute) as? String == kAXButtonRole && labels.contains(attr($0,kAXTitleAttribute) as? String ?? "") && attr($0,kAXEnabledAttribute) as? Bool == true }
        try check(freshButtons.count == 1 && CFEqual(freshButtons[0],buttons[0]));try fileBind(freshButtons[0],pid,button:true)
        var freshActions:CFArray?
        try check(AXUIElementCopyActionNames(freshButtons[0],&freshActions) == .success && (freshActions as? [String] ?? []).contains(kAXPressAction as String))
        if command == "fixture.confirm" {
          guard let selected else { throw Refusal(code:"adapter-refused") }
          try fileBind(selected,pid);try singleSelectedFile(latest,pid,selected)
        }
        if command == "fixture.confirm" { try check(owns(freshButtons[0],pid) && AXUIElementSetMessagingTimeout(freshButtons[0],0.5) == .success && owns(freshButtons[0],pid)) }
        try fileProof();attemptedAction=true
        let query=AXUIElementPerformAction(freshButtons[0],kAXPressAction as CFString);acknowledgedAction=query == .success
        try ui32ProofCheck(query == .success,"fixture-button-post",["queryError":Int(query.rawValue)])
        if command == "fixture.confirm" { confirmed=true }
        try ui32ProofStep("fixture-post-window-identity"){try verifyOwnedWindow(pinned)}
      }
      return ["actionAttempted":attemptedAction,"actionPosted":true,"panelOwner":pid]
    } catch {
      var facts:[String:Any]=["purpose":"UI32","command":command,"actionAttempted":attemptedAction,"stage":"fixture-guard-or-action","retry":false,"underlying":ui32ActionCause(error)]
      if acknowledgedAction {facts["actionPosted"]=true};throw ActionRefusal(diagnostics:facts)
    }
  }
}

// Production selected-set gate, exercised with foreign/extra/unreadable controls.
func ui32SingleSelection<Node>(_ rows:[Node], _ selection:[Node], _ expected:Node,
  equal:(Node,Node)->Bool, owned:(Node)->Bool, selected:(Node) throws -> Bool) throws {
  try check(!rows.isEmpty && rows.count <= 512 && selection.count == 1 && owned(selection[0]) && equal(selection[0],expected))
  var count=0
  for row in rows {
    try check(owned(row))
    if try selected(row) { count += 1;try check(equal(row,expected)) }
  }
  try check(count == 1)
}
func ui32FinishAfterDismissal(_ windowAbsent:Bool, certificate:() throws -> Void, finish:()->Void) throws {
  try ui32ScopeCheck(windowAbsent,"native-window-present");try certificate();finish()
}
func ui32FixtureSelfTest() throws {
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
