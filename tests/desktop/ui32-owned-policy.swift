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
  case "tutorial.close": return try ui32NewSemanticRule("entry.tutorial.close",nil,true)
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
  let profileFocusCases=try ui32ProfileFocusSelfTest()
  try ui32IdentityPhaseSelfTest();try ui32Phase2ReadSelfTest()
  try ui32EntrySelfTest();try ui32NewSemanticSelfTest()
  try ui32AttachedRootCertificateSelfTest()
  try ui32ZeroForeignFallbackSelfTest();try ui32OwnedSheetReadSelfTest();try ui32ContentWalkSelfTest();try ui32OwnedCandidatePathSelfTest();try ui32FileDomainSelfTest();try ui32RowSelectionProofSelfTest();try ui32NewPostCreateReadSelfTest();try ui32InitialOwnedReadinessSelfTest()
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
  try emit(["status":"ui32-static-model-pass","runtimeAX":false,"foreignReads":0,"purpose":"UI32","profileFocusCases":profileFocusCases])
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

// New preparation mode only. The original-mode rule and action guards remain unchanged.
func ui32NewSemanticRule(_ command:String, _ option:String?, _ enabled:Bool) throws -> UI32Rule {
  let old=try ui32Rule(command,option)
  guard enabled else {return old}
  let allowed=["entry.new.default","entry.create.default","entry.tutorial.close","settings.open","settings.close",
    "profile.name","profile.email","locale.choose","tool.hand","tool.select","tool.brush"]
  try check(allowed.contains(command))
  if command == "entry.new.default" {
    return UI32Rule(identifier:nil,region:nil,labels:["New Project","Nouveau projet","新規プロジェクト","Nuevo proyecto"],
      roles:["AXStaticText"],operation:"press",values:[])
  }
  if command == "entry.tutorial.close" {
    return UI32Rule(identifier:nil,region:nil,labels:["Quitter le tutoriel","Exit tutorial","Salir del tutorial","チュートリアルを終了"],
      roles:["AXButton"],operation:"press",values:[])
  }
  let global=["entry.create.default","settings.close","profile.name","profile.email","locale.choose","tool.hand","tool.select","tool.brush"]
  if global.contains(command) {
    return UI32Rule(identifier:old.identifier,region:nil,labels:old.labels,roles:old.roles,operation:old.operation,values:old.values)
  }
  return old
}
func ui32NewSemanticPress(_ command:String, _ enabled:Bool, _ identifier:String, _ labels:Set<String>,
  _ role:String, _ advertised:Set<String>) throws {
  guard enabled else {return}
  let rule=try ui32NewSemanticRule(command,nil,true)
  try check(rule.operation == "press" && ui32Matches(rule,identifier:identifier,labels:labels,role:role,ancestors:[]) && advertised.contains(kAXPressAction))
}
// Counts are from the entire pinned certificate: matching outside nodes are conflicts.
// Capture stage records a real completed owned capture, not a visual acceptance result.
func ui32NewSemanticContext(_ command:String, _ enabled:Bool, _ stage:Int, _ sourceCaptureStage:Int,
  _ settingsOpen:Bool, _ inside:[String:Int], _ outside:Set<String>) throws {
  guard enabled else {return}
  if ["observe","capture","quit"].contains(command) {return}
  let effects=["settings.open","settings.close","profile.name","profile.name.restore","profile.email","profile.email.restore",
    "locale.choose","locale.choose.restore","tool.hand","tool.select","tool.brush"]
  try check(effects.contains(command) || ["entry.new.default","entry.create.default","entry.tutorial.close"].contains(command))
  let ids:Set<String>=["start-open","np-create","np-name","np-fps","np-preset","tools-panel-handle","project-tabs-settings",
    "settings-close","profile-name","profile-email","settings-language"]
  try check(outside.isDisjoint(with:ids) && inside.values.allSatisfy {$0 >= 0 && $0 <= 512})
  func count(_ id:String) -> Int {inside[id] ?? 0}
  switch command {
  case "entry.new.default":try check(stage == 0 && sourceCaptureStage == 0 && !settingsOpen && count("start-open") == 1 && count("settings-close") == 0)
  case "entry.create.default":try check(stage == 1 && !settingsOpen && ["np-create","np-name","np-fps","np-preset"].allSatisfy {count($0) == 1})
  case "entry.tutorial.close":try check(stage == 2 && sourceCaptureStage == 2 && !settingsOpen && count("start-open") == 0 && count("np-create") == 0 && count("settings-close") == 0)
  default:
    try check(stage == 3 && sourceCaptureStage == 3 && count("start-open") == 0 && count("np-create") == 0)
    if command == "settings.open" {try check(!settingsOpen && count("settings-close") == 0 && count("project-tabs-settings") == 1 && count("tools-panel-handle") == 1)}
    else if command.hasPrefix("tool.") {try check(!settingsOpen && count("settings-close") == 0 && count("tools-panel-handle") == 1)}
    else {
      try check(settingsOpen && count("settings-close") == 1)
      if command != "settings.close" {
        let id=command.hasPrefix("profile.name") ? "profile-name" : command.hasPrefix("profile.email") ? "profile-email" : "settings-language"
        try check(count(id) == 1)
      }
    }
  }
}
func ui32NewSettingsNext(_ command:String, _ enabled:Bool, _ settingsOpen:Bool) throws -> Bool {
  guard enabled else {return settingsOpen}
  if command == "settings.open" {try check(!settingsOpen);return true}
  if command == "settings.close" {try check(settingsOpen);return false}
  return settingsOpen
}
func ui32NewDefaultProof(_ pid:pid_t, _ resolve:(UI32Rule) throws -> AXUIElement) throws {
  try ui32ReadNewDefaults(resolve:{ id,role in
    try resolve(UI32Rule(identifier:id,region:nil,labels:[],roles:[role],operation:"observe",values:[]))
  },owned:{owns($0,pid)},prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},read:{node in
    var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,kAXValueAttribute as CFString,&raw)
    return (Int(status.rawValue),raw)
  })
}

func ui32ContentWalk<Node>(_ root:Node, owner expected:Int, deadline:UInt64,
  now:()->UInt64, equal:@escaping (Node,Node)->Bool, owner:(Node)->(Int,Bool,Int),
  children:(Node)->(Int,[Node]?), parent:(Node)->(Int,Node?)) throws -> [Node] {
  let trace=UI32PanelTrace("strict-content",root,expected,equal:equal)
  do {return try boundedOwnedTree(root,owner:expected,deadline:deadline,now:now,equal:equal,
    owner:{trace.ownership($0,owner($0))},children:{trace.children($0,children($0))},parent:{trace.parent($0,parent($0))})}
  catch let error as ActionRefusal {
    var facts=trace.failure(error).diagnostics
    facts["contentTraversal"]=facts.removeValue(forKey:"panelTraversal")
    throw ActionRefusal(diagnostics:facts)
  }
}

// Only the observed invalid passed-element failure, never a missing parent or cause claim.

func ui32FileDomainSelfTest() throws {
  for typed in [false,true] {
    var calls=0,facts=[String:Any]()
    do {_ = try ui32ProofStep("nemo-audit"){() throws -> Void in calls += 1;if typed {throw ActionRefusal(diagnostics:["stage":"original","gotoReason":"original-reason"])};throw Refusal(code:"adapter-refused")}}catch let error as ActionRefusal{facts=error.diagnostics}
    let cause=facts["underlying"] as? [String:Any],nested=cause?["facts"] as? [String:Any]
    try check(calls == 1 && facts["proofStep"] as? String == "nemo-audit" && facts["actionAttempted"] == nil && facts["actionPosted"] == nil)
    try check(typed ? nested?["stage"] as? String == "original" && nested?["gotoReason"] as? String == "original-reason" : cause?["code"] as? String == "adapter-refused")
  }
  var content=[String:Any]()
  do {_ = try ui32ContentReadStep("tree-identity"){() throws -> Void in throw ActionRefusal(diagnostics:["stage":"parent-query","insideContent":true])}}catch let error as ActionRefusal{content=error.diagnostics}
  try check(content["stage"] as? String == "parent-query" && content["insideContent"] as? Bool == true && content["proofStep"] == nil)

  for fault in ["none","missing","substituted","foreign","stale-certified-owner"] {
    var reads=0,refused=false
    do {try ui32FileBinding(fault == "substituted" ? 2 : 1,fault == "missing" ? nil : 1,equal:==,owned:{_ in
      reads += 1;return fault != "foreign" && !(fault == "stale-certified-owner" && reads == 2)
    })}catch{refused=true}
    try check(refused == (fault != "none"))
  }
  func facts(_ match:Bool) -> [String:Any] {[
    "complete":true,"truncated":false,"unqueriedQueries":0,"failedQueries":0,"ownedNodeCount":1,
    "observedExpectedURLMatches":match ? 1 : 0,"nodes":[["index":0,"queriesStarted":2,"expectedURLMatch":match]]]}
  try check(try ui32FileDomain(facts(true),facts(false),nil) == "nemo-owned-sheet")
  try check(try ui32FileDomain(facts(false),facts(true),"authenticated-service") == "authenticated-service")
  for fault in ["partial","truncated","unqueried","failed","zero","duplicate","switch","mismatch","missing-match"] {
    var own=facts(true),service=facts(false),retained:String?=nil
    switch fault {
    case "partial":own["complete"]=false
    case "truncated":own["truncated"]=true
    case "unqueried":own["unqueriedQueries"]=1
    case "failed":own["failedQueries"]=1
    case "zero":own=facts(false)
    case "duplicate":service=facts(true)
    case "switch":retained="authenticated-service"
    case "mismatch":own["observedExpectedURLMatches"]=0
    default:own["nodes"]=[["index":0,"queriesStarted":2]]
    }
    var refused=false;do {_ = try ui32FileDomain(own,service,retained)}catch{refused=true};try check(refused)
  }
}

func ui32RowSelectionProofSelfTest() throws {
  for fault in ["enabled","selectable","already-selected","disabled","enabled-number","enabled-string","enabled-error","enabled-nil",
    "unsupported-payload","no-value","selected-error","selected-number","selected-nil","settable-error","not-settable",
    "foreign","stale","timeout","deadline","expired-after-read","expired-before-read","changed-after-settable","owner-consumes-budget"] {
    var calls=[String](),owners=0,clock:UInt64=fault == "deadline" ? 10 : 0,refused=false,proof=[String:Any]()
    do {
      proof=try ui32RowSelectionProof(7,deadline:10,now:{clock},owned:{node in
        owners += 1;calls.append("owner");if fault == "owner-consumes-budget" {clock=10};return node == 7 && fault != "foreign" && !(fault == "stale" && owners == 3) && !(fault == "changed-after-settable" && calls.contains("settable"))
      },prepare:{node in
        calls.append("prepare");if fault == "expired-before-read" {clock=10};return node == 7 && fault != "timeout"
      },read:{node,key in
        calls.append(key);if fault == "expired-after-read" {clock=10}
        if key == kAXEnabledAttribute {
          if fault == "enabled" {return (0,kCFBooleanTrue)}
          if fault == "disabled" {return (0,kCFBooleanFalse)}
          if fault == "enabled-number" {return (0,NSNumber(value:1))}
          if fault == "enabled-string" {return (0,"true" as CFString)}
          if fault == "enabled-error" {return (-1,nil)}
          if fault == "enabled-nil" {return (0,nil)}
          if fault == "unsupported-payload" {return (Int(AXError.attributeUnsupported.rawValue),kCFBooleanTrue)}
          if fault == "no-value" {return (Int(AXError.noValue.rawValue),nil)}
          return (Int(AXError.attributeUnsupported.rawValue),nil)
        }
        if fault == "selected-error" {return (-1,nil)}
        if fault == "selected-number" {return (0,NSNumber(value:1))}
        if fault == "selected-nil" {return (0,nil)}
        return (0,fault == "already-selected" ? kCFBooleanTrue : kCFBooleanFalse)
      },selectedSettable:{node in
        calls.append("settable");return (fault == "settable-error" ? -1 : 0,fault != "not-settable")
      })
    }catch is ActionRefusal {refused=true}
    let positive=["enabled","selectable","already-selected"].contains(fault)
    try check(refused == !positive)
    let attributes=calls.filter {$0 == kAXEnabledAttribute || $0 == kAXSelectedAttribute || $0 == "settable"}
    try check(attributes.count <= 3)
    if fault == "enabled" {try check(attributes == [kAXEnabledAttribute] && owners == 3 && calls.filter {$0 == "prepare"}.count == 1 && proof["basis"] as? String == "row-enabled")}
    if fault == "selectable" || fault == "already-selected" {
      try check(attributes == [kAXEnabledAttribute,kAXSelectedAttribute,"settable"] && proof["basis"] as? String == "row-selection-capability" && proof["enabled"] == nil)
      try check(proof["selected"] as? Bool == (fault == "already-selected") && owners == 9 && calls.filter {$0 == "prepare"}.count == 3)
    }
    if ["disabled","enabled-number","enabled-string","enabled-error","enabled-nil","unsupported-payload","no-value","stale","expired-after-read"].contains(fault) {try check(attributes == [kAXEnabledAttribute])}
    if ["selected-error","selected-number","selected-nil"].contains(fault) {try check(attributes == [kAXEnabledAttribute,kAXSelectedAttribute])}
    if ["foreign","timeout","deadline","expired-before-read"].contains(fault) {try check(attributes.isEmpty)}
    if fault == "deadline" {try check(calls.isEmpty)}
    if fault == "owner-consumes-budget" {try check(calls == ["owner"] && owners == 1 && attributes.isEmpty)}
  }
}

// Actual New orchestration must not publish on its final outer identity/budget failure.
func ui32PostedNewContentReadSelfTest() throws {
  for fault in ["none","outer-native","late-certificate"] {
    var clock:UInt64=0,nativeCalls=0,reads=0,published=false,refused=false
    do {
      let accepted=try ui32PostedNewContentRead("entry.create.default",true,true,true,2,expected:1,now:{clock},retained:{1},equal:==,native:{
        nativeCalls += 1;if fault == "outer-native" && nativeCalls == 4 {throw Refusal(code:"child-owner")}
      },certificate:{deadline,expected in
        reads += 1
        let proof:UI32ContentCertificate<Int>=try ui32ContentCertificate(0,pid:7,retained:expected,deadline:deadline,now:{clock},equal:==,
          owner:{_ in (0,true,7)},prepare:{_ in true},traits:{($0 == 0 ? "AXWindow" : $0 == 1 ? "AXWebArea" : "AXGroup",false)},
          children:{(0,$0 == 0 ? [1] : $0 == 1 ? [2] : [])},parent:{(0,$0 == 1 ? 0 : 1)},strict:{_,_ in [1,2]})
        if fault == "late-certificate" {clock=deadline};return proof
      })
      try check(ui32ContentMembers(accepted.certificate) == [1,2]);published=true
    }catch{refused=true}
    try check(reads == 1 && published == (fault == "none") && refused == (fault != "none"))
    try check(nativeCalls == (fault == "late-certificate" ? 2 : 4))
  }
}

func ui32ZeroForeignFallback(_ observed:Int, _ facts:[String:Any]) -> Bool {
  observed == 0 && facts["stage"] as? String == "candidate-count" && facts["ownedSearchComplete"] as? Bool == true &&
    facts["foreignRootCount"] as? Int == 0 && facts["ownedParentMismatchCount"] as? Int == 0
}
func ui32FileBinding<Node>(_ actual:Node, _ certified:Node?, equal:(Node,Node)->Bool, owned:(Node)->Bool) throws {
  guard let certified else {throw Refusal(code:"adapter-refused")}
  try check(owned(actual) && owned(certified) && equal(actual,certified))
}
