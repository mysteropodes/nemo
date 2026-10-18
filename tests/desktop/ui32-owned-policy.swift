// UI32 validation policy: closed semantic operations, never an arbitrary AX setter.
import Foundation
import ApplicationServices

struct UI32Rule {
  let identifier: String?
  let region: String?
  let labels: Set<String>
  let roles: Set<String>
  let operation: String
  let values: Set<String>
}
let ui32Languages: Set<String> = ["English","Français","日本語","Español"]
let ui32Tabs: Set<String> = ["General","Général","一般","General", "Updates","Mises à jour","アップデート","Actualizaciones",
  "Collaboration","コラボレーション","Colaboración","Feedback","フィードバック","Feedback","Figma",
  "Shortcuts","Raccourcis","ショートカット","Atajos","Labs","About","À propos","アプリについて","Acerca de"]
func ui32Scope(_ mode:String, _ env:[String:String]) throws -> (reference:Bool,report:String) {
  try check(mode == "--ui32-persistent")
  guard let grant=env["NEMO_UI32_GRANT"], grant.range(of:"^1062/[0-9]+$",options:.regularExpression) != nil,
    let report=env["NEMO_UI32_REPORT_ROOT"], report.hasPrefix("/"), !report.isEmpty else { throw Refusal(code:"adapter-refused") }
  // The existing purpose grants cannot substitute for an explicit UI32 grant.
  return (false,report)
}
func ui32Rule(_ command:String, _ option:String?) throws -> UI32Rule {
  func rule(_ id:String?, _ region:String?, _ labels:Set<String>, _ roles:Set<String>, _ operation:String="press", _ values:Set<String> = []) -> UI32Rule {
    UI32Rule(identifier:id,region:region,labels:labels,roles:roles,operation:operation,values:values)
  }
  let button:Set<String>=["AXButton"], text:Set<String>=["AXTextField"], popup:Set<String>=["AXPopUpButton"]
  switch command {
  case "playback.pause": return rule("btn-play",nil,[],button)
  case "sync.open-cancel": return rule("sync-choose-folder","settings-modal",[],button)
  case "sync.check-empty": return rule("sync-check","settings-modal",[],button)
  case "fixture.open": return rule("start-open",nil,[],button)
  case "entry.new.default": return rule("start-new","start-screen",[],["AXGroup","AXButton"])
  case "entry.create.default": return rule("np-create","start-newpanel",[],button)
  case "entry.tutorial.close": return rule(nil,"tut-tooltip",["×"],button)
  case "selection.A": return rule(nil,"layer-list",["F06 absent flags (F06-A)"],button)
  case "selection.B": return rule(nil,"layer-list",["F06 explicit false (F06-B)"],button)
  case "frame.first": return rule("btn-ff",nil,[],button)
  case "frame.next": return rule("btn-nf",nil,[],button)
  case "grid.step.more": return rule(nil,"labs-float-params",["+"],button)
  case "grid.step.less": return rule(nil,"labs-float-params",["−"],button)
  case "grid.step.value": return rule(nil,"labs-float-params",[],text,"value",["4","500","100"])
  case "settings.menu.open": return rule("app-menu-btn",nil,[],button)
  case "settings.menu.entry": return rule(nil,nil,["Settings","Réglages","設定","Ajustes"],["AXMenuItem","AXGroup","AXStaticText"])
  case "settings.backdrop": return rule("settings-modal",nil,[],["AXGroup"])
  case "grid.step.focus": return rule(nil,"labs-float-params",[],text,"focus")
  case "settings.open": return rule("project-tabs-settings",nil,[],button)
  case "settings.close": return rule("settings-close","settings-modal",[],button)
  case "settings.tab":
    guard let option, ui32Tabs.contains(option) else { throw Refusal(code:"adapter-refused") }
    return rule(nil,"settings-tabs",[option],button)
  case "locale.choose": return rule("settings-language","settings-modal",[],popup,"value",ui32Languages)
  case "profile.name": return rule("profile-name","settings-modal",[],text,"value",["UI-validation",""])
  case "profile.email": return rule("profile-email","settings-modal",[],text,"value",["ui-validation@example.invalid",""])
  case "profile.role": return rule("profile-role","settings-modal",[],popup,"value",
    ["Animator","Supervisor","Producer","Animateur","Superviseur","Producteur","アニメーター","スーパーバイザー","プロデューサー","Animador","Supervisor","Productor"])
  case "mode.motion": return rule(nil,"app-mode-switch",["Motion"],button)
  case "mode.animation": return rule(nil,"app-mode-switch",["Animation 2D"],button)
  case "tool.hand": return rule(nil,"tools-panel",["Hand","Main","手のひら","Mano","Hand — pan the canvas (H or Space)"],button)
  case "tool.select": return rule(nil,"tools-panel",["Select","Sélection","選択","Selección","Selection / Transform (V)"],button)
  case "tool.brush": return rule(nil,"tools-panel",["Brush","Pinceau","ブラシ","Pincel","Brush (B)"],button)
  case "motion.cancel-pending": return rule(nil,"motion-props-body",["Cancel"],button)
  case "motion.header-menu": return rule(nil,"motion-props-sec",["Layer properties","Propriétés du calque","レイヤープロパティ","Propiedades de capa"],["AXGroup","AXStaticText"],"menu")
  case "motion.filter":
    guard let option, Set(["all","animated","modified","expressions","errors","effects","mattes"]).contains(option) else { throw Refusal(code:"adapter-refused") }
    return rule(nil,nil,[option],["AXMenuItem","AXButton","AXGroup","AXStaticText"],"filter-press")
  case "motion.reveal":
    guard let option, Set(["All","P","A","R","S","T","U","E","M","animated","modified","expressions"]).contains(option) else { throw Refusal(code:"adapter-refused") }
    return rule(nil,"motion-props-body",[option],button)
  case "motion.search": return rule(nil,"motion-props-body",["Search properties"],text,"value",["","Opacity","UI32-no-such-property"])
  case "labs.grid": return rule(nil,"labs-tools-list",["World grid","Grille monde","ワールドグリッド","Cuadrícula de mundo"],["AXCheckBox"])
  case "labs.value": return rule(nil,"labs-tools-list",["Value check","Contrôle des valeurs","値確認","Control de valores"],["AXCheckBox"])
  case "float.grid": return rule(nil,"labs-float-icons",["Grid","Grille","グリッド","Cuadrícula"],button)
  case "float.value": return rule(nil,"labs-float-icons",["Value check","Contrôle des valeurs","値確認","Control de valores"],button)
  default: throw Refusal(code:"adapter-refused")
  }
}
func ui32PauseAllowed(_ glyphs:Set<String>) -> Bool { glyphs.contains("\u{e034}") && !glyphs.contains("\u{e037}") }
func ui32EmptySyncAllowed(_ value:String?, _ queryOK:Bool, _ writable:Bool) -> Bool { value == "" && queryOK && !writable }
func ui32Matches(_ rule:UI32Rule, identifier:String, labels:Set<String>, role:String, ancestors:Set<String>) -> Bool {
  rule.roles.contains(role) && (rule.identifier == nil || rule.identifier == identifier) &&
    (rule.region == nil || ancestors.contains(rule.region!)) && (rule.labels.isEmpty || !rule.labels.isDisjoint(with:labels))
}
// Both production actions and fault controls use this exact pre/action/post sequence.
// A posted native-picker action changes the applicable identity scope, not its owner.
enum UI32IdentityPhase:Equatable { case content, nativePicker, nativePickerCompletion }
func ui32IdentityPhase(_ command:String?, fixtureActive:Bool, syncActive:Bool) throws -> UI32IdentityPhase {
  try check(!(fixtureActive && syncActive))
  if !fixtureActive && !syncActive { return .content }
  guard let command else { throw Refusal(code:"adapter-refused") }
  let allowed:Set<String> = fixtureActive ?
    ["fixture.open","fixture.observe","fixture.choose","fixture.confirm","fixture.cancel","fixture.complete","quit"] :
    ["sync.open-cancel","sync.panel-observe","sync.cancel-only","sync.complete","quit"]
  try check(allowed.contains(command))
  return command == "fixture.complete" || command == "sync.complete" ? .nativePickerCompletion : .nativePicker
}
func ui32ActionCause(_ error:Error) -> [String:Any] {
  if let value=error as? IdentityRefusal { return ["kind":"identity","stage":value.stage,"facts":value.facts] }
  if let value=error as? ActionRefusal { return ["kind":"action","facts":value.diagnostics] }
  if let value=error as? SelectionRefusal { return ["kind":"selection","facts":value.diagnostics] }
  if let value=error as? Refusal { return ["kind":"refusal","code":value.code] }
  return ["kind":"unexpected-error"]
}
func ui32GuardedAction(_ identity:() throws -> Void, _ scope:() throws -> Void, _ action:() throws -> Void,
  postScope:() throws -> Void, posted:() -> Void) throws {
  try identity(); try scope(); try identity(); try action(); posted(); try identity(); try postScope()
}
func ui32SelfTest() throws {
  try ui32IdentityPhaseSelfTest()
  try ui32EntrySelfTest()
  try ui32AttachedRootCertificateSelfTest()
  try ui32ZeroForeignFallbackSelfTest()
  for mode in ["--persistent","--reference-persistent","--png-persistent","--p38-cg-sample"] {
    try check(!ui32ModeAllowed(mode))
    var refused=false
    do { _ = try ui32Scope(mode,["NEMO_UI32_GRANT":"1062/1","NEMO_UI32_REPORT_ROOT":"/private/proof"]) } catch { refused=true }
    try check(refused)
  }
  for key in ["NEMO_P38_GRANT","NEMO_REFERENCE_GRANT","NEMO_PNG_UI_GRANT"] {
    var refused=false
    do { _ = try ui32Scope("--ui32-persistent",[key:"1062/1","NEMO_UI32_REPORT_ROOT":"/private/proof"]) } catch { refused=true }
    try check(refused)
  }
  try check(ui32PauseAllowed(["\u{e034}"]) && !ui32PauseAllowed(["\u{e037}"]) && !ui32PauseAllowed(["\u{e034}","\u{e037}"]) && !ui32PauseAllowed([]))
  try check(ui32EmptySyncAllowed("",true,false) && !ui32EmptySyncAllowed("/shared",true,false) && !ui32EmptySyncAllowed(nil,true,false) && !ui32EmptySyncAllowed("",false,false) && !ui32EmptySyncAllowed("",true,true))
  for observed in [false,true] {
    var writes=0,refused=false
    do { try ui32RequireObserved(observed);writes += 1 } catch { refused=true }
    try check(writes == (observed ? 1 : 0) && refused == !observed)
  }
  try check(ui32ModeAllowed("--ui32-persistent") && ui32ModeAllowed("--ui32-self-test"))
  try ui32FixtureSelfTest()
  try ui32KeysSelfTest()
  let name=try ui32Rule("profile.name",nil)
  try check(ui32Matches(name,identifier:"profile-name",labels:[],role:"AXTextField",ancestors:["settings-modal"]))
  for fault in ["id","role","region"] {
    try check(!ui32Matches(name,identifier:fault == "id" ? "np-name" : "profile-name",labels:[],
      role:fault == "role" ? "AXButton" : "AXTextField",ancestors:fault == "region" ? [] : ["settings-modal"]))
  }
  try check(!name.values.contains("arbitrary user text") && name.values.contains("UI-validation"))
  for denied in ["press","set","select","click","key","draw","Keys","Save","Open","Undo","shortcuts.reset","sync.publish"] {
    var refused=false; do { _ = try ui32Rule(denied,nil) } catch { refused=true }; try check(refused)
  }
  for fault in 0...5 {
    var checks=0, scopes=0, events=[String](), refused=false
    do { try ui32GuardedAction({ checks += 1; if fault == checks { throw Refusal(code:"adapter-refused") } },{
      scopes += 1; if fault == 4 && scopes == 1 { throw Refusal(code:"adapter-refused") }
    },{events.append("ax")},postScope:{ if fault == 5 { throw Refusal(code:"adapter-refused") } },posted:{events.append("posted")}) } catch { refused=true }
    try check(refused == (fault != 0))
    try check(events == ((fault == 0 || fault == 3 || fault == 5) ? ["ax","posted"] : []))
  }
  try emit(["status":"ui32-static-model-pass","runtimeAX":false,"foreignReads":0,"purpose":"UI32"])
}

// Keep the existing child-array/count witness contract exactly; no nil-as-empty.
func ui32Children(_ node:AXUIElement, _ pid:pid_t) -> (Int,[AXUIElement]?) {
  guard owns(node,pid), AXUIElementSetMessagingTimeout(node,0.03) == .success else { return (-1,nil) }
  let result=ownedAXChildren(node,expectedOwner:Int(pid))
  guard result.ownerRefusal == nil, let values=result.values else { return (-1,nil) }
  return (0,values)
}
func ui32ModeAllowed(_ mode:String) -> Bool { ["--ui32-persistent","--ui32-self-test"].contains(mode) }
func ui32ActionAdmitted(_ observed:Bool) -> Bool { observed }

func ui32RequireObserved(_ observed:Bool) throws { try check(ui32ActionAdmitted(observed)) }

// These controls exercise the production action ordering and phase predicate.
func ui32IdentityPhaseSelfTest() throws {
  try check(try ui32IdentityPhase(nil,fixtureActive:false,syncActive:false) == .content)
  for fixture in [true,false] {
    let allowed=fixture ? ["fixture.open","fixture.observe","fixture.choose","fixture.confirm","fixture.cancel","fixture.complete","quit"] :
      ["sync.open-cancel","sync.panel-observe","sync.cancel-only","sync.complete","quit"]
    for command in allowed {
      let phase=try ui32IdentityPhase(command,fixtureActive:fixture,syncActive:!fixture)
      try check(phase == (command.hasSuffix(".complete") ? .nativePickerCompletion : .nativePicker))
    }
    for denied in [nil,"observe","capture","settings.open","motion.shortcut",fixture ? "sync.complete" : "fixture.choose"] as [String?] {
      var actions=0,refused=false
      do { try ui32GuardedAction({ _ = try ui32IdentityPhase(denied,fixtureActive:fixture,syncActive:!fixture) },{},
        {actions += 1},postScope:{},posted:{}) } catch { refused=true }
      try check(refused && actions == 0)
    }
  }
  var refused=false
  do { _ = try ui32IdentityPhase("fixture.observe",fixtureActive:true,syncActive:true) } catch { refused=true }
  try check(refused)
  for loseOwner in [false,true] {
    var active=false,phases=[UI32IdentityPhase](),actions=0,posts=0,failed=false
    do { try ui32GuardedAction({
      phases.append(try ui32IdentityPhase("fixture.open",fixtureActive:active,syncActive:false))
      if active && loseOwner { throw IdentityRefusal(stage:"process-start",facts:["processStartMatches":false]) }
    },{}, {actions += 1},postScope:{try check(active)},posted:{active=true;posts += 1}) } catch { failed=true }
    try check(phases == [.content,.content,.nativePicker] && actions == 1 && posts == 1 && failed == loseOwner)
  }
  let identity=ui32ActionCause(IdentityRefusal(stage:"process-start",facts:["processStartMatches":false]))
  try check(identity["kind"] as? String == "identity" && identity["stage"] as? String == "process-start")
  let action=ui32ActionCause(ActionRefusal(diagnostics:["stage":"parent-edge"]))
  try check((action["facts"] as? [String:String])?["stage"] == "parent-edge")
  try check(ui32ActionCause(Refusal(code:"child-owner"))["code"] as? String == "child-owner")
}

// A finite preparation mode, never a caller-selected document or input route.
func ui32EntryMode(_ raw:String?) throws -> Bool {
  if raw == nil { return false }
  try check(raw == "ordinary-new-default");return true
}
func ui32EntryNext(_ command:String, _ enabled:Bool, _ stage:Int) throws -> Int {
  if command.hasPrefix("entry.") {
    try check(enabled)
    switch command {
    case "entry.new.default":try check(stage == 0);return 1
    case "entry.create.default":try check(stage == 1);return 2
    case "entry.tutorial.close":try check(stage == 2);return 3
    default:throw Refusal(code:"adapter-refused")
    }
  }
  if enabled {
    let readOnly=["observe","capture","quit"]
    let effects=["settings.open","settings.close","settings.tab","profile.name","profile.name.restore",
      "profile.email","profile.email.restore","locale.choose","locale.choose.restore","tool.hand","tool.select","tool.brush"]
    try check(readOnly.contains(command) || (stage >= 2 && effects.contains(command)))
  }
  return stage
}
let ui32EntryOverlayIDs:Set<String>=["start-screen","start-newpanel","tut-tooltip","tut-spotlight"]
func ui32EntryClassTokens(_ query:Int, _ raw:[String]?) throws -> Set<String> {
  guard query == 0,let raw,raw.count <= 8,raw.allSatisfy({!$0.isEmpty && $0.utf8.count <= 64}),Set(raw).count == raw.count else {
    throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"entry-overlay-class-query","queryError":query])
  }
  return Set(raw)
}
func ui32EntryDOMClasses(_ node:AXUIElement, _ pid:pid_t) throws -> Set<String> {
  try check(owns(node,pid) && AXUIElementSetMessagingTimeout(node,0.03) == .success && owns(node,pid))
  var raw:CFTypeRef?;let query=AXUIElementCopyAttributeValue(node,"AXDOMClassList" as CFString,&raw)
  try check(owns(node,pid));return try ui32EntryClassTokens(Int(query.rawValue),raw as? [String])
}
// Known class target-state only; visible/native-readiness oracles remain mandatory.
func ui32EntrySurface(_ command:String, _ enabled:Bool, _ states:[String:Set<String>]) throws {
  guard enabled && !command.hasPrefix("entry.") else {return}
  let allowed:[String:Set<String>]=["start-screen":["hid"],"start-newpanel":[],
    "tut-tooltip":["on","tutorial-native-card","tutorial-native-pending"],"tut-spotlight":["on","no-target"]]
  for (id,classes) in states {
    var reason:String?
    if !ui32EntryOverlayIDs.contains(id) || !classes.isSubset(of:allowed[id] ?? []) {reason="unknown-classes"}
    else if id == "start-newpanel" {reason="new-panel-present"}
    else if id == "start-screen" ? !classes.contains("hid") : classes.contains("on") {reason="active-overlay"}
    if let reason {throw ActionRefusal(diagnostics:["purpose":"UI32","stage":"entry-overlay-state","overlay":id,"reason":reason,"classes":Array(classes.sorted().prefix(8)).map {String($0.prefix(64))}])}
  }
}
func ui32NewDefaults(_ values:[String:String]) throws {
  try check(values.count == 3 && values["np-name"] == "Untitled" && values["np-fps"] == "24" &&
    ["1920x1080","1920 × 1080 (Full HD)"].contains(values["np-preset"] ?? ""))
}
func ui32EntrySelfTest() throws {
  try check(try ui32EntryMode(nil) == false && ui32EntryMode("ordinary-new-default"))
  for mode in ["","new","arbitrary"] {var refused=false;do {_ = try ui32EntryMode(mode)}catch{refused=true};try check(refused)}
  var stage=0
  for command in ["entry.new.default","entry.create.default","entry.tutorial.close"] {stage=try ui32EntryNext(command,true,stage)}
  try check(stage == 3 && ui32EntryNext("profile.name",true,stage) == stage)
  for (command,enabled,state) in [("entry.new.default",false,0),("entry.create.default",true,0),
    ("entry.new.default",true,1),("entry.tutorial.close",true,1),("entry.tutorial.close",true,3),
    ("entry.create.default.restore",true,1),("entry.unknown",true,2),("profile.name",true,0),("settings.open",true,1)] {
    var effects=0,refused=false;do {_ = try ui32EntryNext(command,enabled,state);effects += 1}catch{refused=true}
    try check(refused && effects == 0)
  }
  for command in ["observe","capture","quit"] {try check(try ui32EntryNext(command,true,0) == 0)}
  for state in [2,3] {
    for command in ["motion.focus","motion.shortcut","motion.guard-typing","selection.toggle.B","fixture.confirm","sync.cancel-only","frame.next","grid.step.value"] {
      var effects=0,refused=false;do {_ = try ui32EntryNext(command,true,state);effects += 1}catch{refused=true};try check(refused && effects == 0)
      try check(try ui32EntryNext(command,false,state) == state)
    }
    for command in ["settings.open","settings.close","settings.tab","profile.name","profile.name.restore","profile.email","profile.email.restore","locale.choose","locale.choose.restore","tool.hand","tool.select","tool.brush"] {
      try check(try ui32EntryNext(command,true,state) == state)
    }
  }
  for (id,classes) in [("start-screen",Set<String>()),("start-newpanel",[]),("tut-tooltip",["on"]),("tut-spotlight",["on"]),("tut-tooltip",["unknown"]),("other",[])] {
    var effects=0,refused=false;do {try ui32EntrySurface("settings.open",true,[id:classes]);effects += 1}catch{refused=true}
    try check(refused && effects == 0)
  }
  var overlayFacts:[String:Any]=[:]
  do {try ui32EntrySurface("settings.open",true,["tut-tooltip":["on"]])}catch let error as ActionRefusal {overlayFacts=error.diagnostics}
  try check(overlayFacts["overlay"] as? String == "tut-tooltip" && overlayFacts["reason"] as? String == "active-overlay" && overlayFacts["classes"] as? [String] == ["on"])
  try ui32EntrySurface("settings.open",true,[:])
  try ui32EntrySurface("settings.open",true,["start-screen":["hid"],"tut-tooltip":["tutorial-native-card"],"tut-spotlight":["no-target"]])
  try check(try ui32EntryClassTokens(0,["on"]) == ["on"] && ui32EntryClassTokens(0,[]) == [])
  for (status,raw) in [(0,nil),(1,[String]()),(0,["on","on"]),(0,[""]),(0,[String(repeating:"a",count:65)]),(0,Array(repeating:"a",count:9))] {
    var refused=false;do {_ = try ui32EntryClassTokens(status,raw)}catch{refused=true};try check(refused)
  }
  let brush=try ui32Rule("tool.brush",nil)
  try check(ui32Matches(brush,identifier:"",labels:["Brush (B)"],role:"AXButton",ancestors:["tools-panel"]))
  let defaults=["np-name":"Untitled","np-fps":"24","np-preset":"1920 × 1080 (Full HD)"]
  try ui32NewDefaults(defaults)
  for key in defaults.keys {var changed=defaults;changed[key]="other";var refused=false;do {try ui32NewDefaults(changed)}catch{refused=true};try check(refused)}
  for command in ["entry.new.default","entry.create.default","entry.tutorial.close"] {
    let rule=try ui32Rule(command,nil);try check(rule.operation == "press" && rule.region != nil && rule.values.isEmpty)
    try check(!ui32Matches(rule,identifier:rule.identifier ?? "",labels:rule.labels,role:"AXTextField",ancestors:[rule.region!]))
  }
}

// UI32-only discovery certificate. Service authentication and strict service walk follow unchanged.
func ui32AttachedRootCertificate<Node>(_ root:Node, owner expectedOwner:Int, deadline:UInt64,
  now:()->UInt64, equal:(Node,Node)->Bool, owner:(Node)->(Int,Bool,Int), children:(Node)->(Int,[Node]?),
  parent:(Node)->(Int,Node?), foreignChild:(String,Int)->Void = {_,_ in}) throws
  -> (root:Node,parent:Node,pid:Int,route:String,ownedNodeCount:Int,ownedPathRoutes:[String],outsideMismatches:[[String:Any]]) {
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
  return (candidate,nodes[ancestor],pid,route,nodes.count,path.sorted().map {routes[$0]},outside)
}

// Pure ports execute the exact discovery function above, not a copied walker.
func ui32AttachedRootCertificateSelfTest() throws {
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

func ui32ZeroForeignFallback(_ observed:Int, _ facts:[String:Any]) -> Bool {
  observed == 0 && facts["stage"] as? String == "candidate-count" && facts["ownedSearchComplete"] as? Bool == true &&
    facts["foreignRootCount"] as? Int == 0 && facts["ownedParentMismatchCount"] as? Int == 0
}
func ui32ZeroForeignFallbackSelfTest() throws {
  let complete:[String:Any]=["stage":"candidate-count","ownedSearchComplete":true,"foreignRootCount":0,"ownedParentMismatchCount":0]
  try check(ui32ZeroForeignFallback(0,complete) && !ui32ZeroForeignFallback(1,complete))
  for (key,bad) in [("stage","parent-edge" as Any),("ownedSearchComplete",false),("foreignRootCount",1),("ownedParentMismatchCount",1)] {
    var facts=complete;facts[key]=bad;try check(!ui32ZeroForeignFallback(0,facts));facts.removeValue(forKey:key);try check(!ui32ZeroForeignFallback(0,facts))
  }
}
