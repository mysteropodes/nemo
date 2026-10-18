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
  case "tool.brush": return rule(nil,"tools-panel",["Brush","Pinceau","ブラシ","Pincel"],button)
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
