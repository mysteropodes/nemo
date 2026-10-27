// Closed Motion-only keyboard cases. Events target one retained PID, never a tap.
import Foundation
import ApplicationServices
import CoreGraphics
import Darwin

// HTML rule IDs are DOM identifiers; native IDs remain an unambiguous compatibility source.
struct UI32Identifiers {
  let dom:String?, native:String?, domStatus:Int, nativeStatus:Int
  var resolved:String? {
    if let dom,!dom.isEmpty { return dom }
    if let native,!native.isEmpty { return native }
    return nil
  }
  var source:String {
    let d = !(dom ?? "").isEmpty, n = !(native ?? "").isEmpty
    return d ? (n ? "dom+native" : "dom") : (n ? "native" : "none")
  }
  var observation:[String:Any] {
    ["dom":dom ?? "","native":native ?? "","resolved":resolved ?? "","source":source,
      "domStatus":domStatus,"nativeStatus":nativeStatus]
  }
}
// Production and pure controls share owner-before/after, typed status and conflict checks.
func ui32ReadIdentifiers<Node>(_ node:Node, owned:(Node)->Bool, prepare:(Node)->Bool,
  read:(Node,String)->(Int,Any?)) throws -> UI32Identifiers {
  func value(_ key:String) throws -> (String?,Int) {
    try check(owned(node));try check(prepare(node));try check(owned(node))
    let (status,raw)=read(node,key)
    try check(owned(node))
    if status == Int(AXError.attributeUnsupported.rawValue) || status == Int(AXError.noValue.rawValue) {
      try check(raw == nil);return (nil,status)
    }
    try check(status == Int(AXError.success.rawValue))
    guard let text=raw as? String else { throw Refusal(code:"adapter-refused") }
    return (text,status)
  }
  let (dom,domStatus)=try value("AXDOMIdentifier"), (native,nativeStatus)=try value(kAXIdentifierAttribute)
  if let dom,!dom.isEmpty,let native,!native.isEmpty {
    try check(Array(dom.utf8) == Array(native.utf8))
  }
  return UI32Identifiers(dom:dom,native:native,domStatus:domStatus,nativeStatus:nativeStatus)
}
func ui32DOMIdentifiers(_ node:AXUIElement, _ pid:pid_t) throws -> UI32Identifiers {
  try ui32ReadIdentifiers(node,owned:{owns($0,pid)},
    prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},read:{node,key in
      var raw:CFTypeRef?
      let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
      return (Int(status.rawValue),raw)
    })
}

func ui32Key(_ option:String) throws -> (code:CGKeyCode,text:String,shift:Bool) {
  let shifted=option.hasPrefix("shift+"), label=shifted ? String(option.dropFirst(6)) : option
  let keys:[String:CGKeyCode]=["P":35,"A":0,"R":15,"S":1,"T":17,"U":32,"E":14,"M":46]
  guard let code=keys[label], !shifted || Set(["P","A","R","S","T"]).contains(label) else { throw Refusal(code:"adapter-refused") }
  return (code,shifted ? label : label.lowercased(),shifted)
}
func ui32MotionContent<Node>(_ certificate:UI32ContentCertificate<Node>, _ focus:Node,
  equal:(Node,Node)->Bool, identifier:(Node) throws -> String?) throws {
  guard let target=certificate.nodes.firstIndex(where:{equal($0,focus)}),certificate.inside.contains(target) else {
    throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"motion-focus-outside-content"])
  }
  var inside=[Int](),outside=[Int]()
  for i in certificate.nodes.indices {
    if try identifier(certificate.nodes[i]) == "motion-props-body" {
      if certificate.inside.contains(i) {inside.append(i)} else {outside.append(i)}
    }
  }
  let context=try ui32ScopedCandidate(inside,outside)
  var current:Int?=target
  while let index=current {
    if index == context {return};current=certificate.declared[index]
  }
  throw ActionRefusal(diagnostics:["stage":"content-scope","reason":"motion-focus-wrong-context"])
}
func ui32MotionFocus(_ pinned:OwnedInitialWindow, _ expected:AXUIElement?, _ text:Bool,
  contentScope:(AXUIElement) throws -> Void) throws {
  try verifyOwnedWindow(pinned)
  guard let window=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),owns(window,pinned.pid),CFEqual(window,pinned.main),
    let node=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(node,pinned.pid) else { throw Refusal(code:"adapter-refused") }
  if let expected { try check(CFEqual(node,expected)) }
  let role=attr(node,kAXRoleAttribute) as? String ?? ""
  try check(text ? role == kAXTextFieldRole : ![kAXTextFieldRole,"AXTextArea","AXComboBox"].contains(role))
  var current=node,visited=[AXUIElement](), inMotion=false
  for _ in 0..<20 {
    try check(owns(current,pinned.pid) && !visited.contains {CFEqual($0,current)});visited.append(current)
    if try ui32DOMIdentifiers(current,pinned.pid).resolved == "motion-props-body" { inMotion=true }
    if CFEqual(current,pinned.main) { try check(inMotion);try contentScope(node);return }
    guard let parent=axElement(attr(current,kAXParentAttribute)),owns(parent,pinned.pid),
      let rows=ownedAXChildren(parent,expectedOwner:Int(pinned.pid)).values,rows.contains(where:{CFEqual($0,current)}) else { throw Refusal(code:"child-owner") }
    current=parent
  }
  throw Refusal(code:"adapter-refused")
}
func runUI32Keys(_ command:String, _ option:String?, _ pinned:OwnedInitialWindow,
  resolve:(UI32Rule) throws -> AXUIElement, visible:(AXUIElement) throws -> Void,
  contentScope:(AXUIElement) throws -> Void) throws -> [String:Any] {
  try verifyOwnedWindow(pinned)
  if command == "motion.focus" {
    try check(option == "search" || option == "toolbar")
    let search=option == "search", rule=try ui32Rule(search ? "motion.search" : "motion.reveal",search ? nil : "All")
    let node=try resolve(rule);try visible(node);var writable=DarwinBoolean(false)
    try check(AXUIElementIsAttributeSettable(node,kAXFocusedAttribute as CFString,&writable) == .success && writable.boolValue)
    try verifyOwnedWindow(pinned);try check(CFEqual(try resolve(rule),node))
    try check(AXUIElementSetAttributeValue(node,kAXFocusedAttribute as CFString,kCFBooleanTrue) == .success)
    try cs7MotionFocus(pinned,node,search,rule,resolve:resolve,visible:visible,contentScope:contentScope)
    return ["focusAcknowledged":true,"note":"Focus proof only; no feature acceptance."]
  }
  try check(command == "motion.shortcut" || command == "motion.guard-typing")
  let typing=command == "motion.guard-typing"
  if typing { try check(option == nil) }
  let options=typing ? ["P","A","R","S","T"] : [option ?? ""]
  let search:AXUIElement?
  if typing { search=try resolve(ui32Rule("motion.search",nil)) } else { search=nil }
  var down=false,up=false,postedPairs=0
  do {
    for option in options {
      let key=try ui32Key(option)
      guard let source=CGEventSource(stateID:.privateState),
        let press=CGEvent(keyboardEventSource:source,virtualKey:key.code,keyDown:true),
        let release=CGEvent(keyboardEventSource:source,virtualKey:key.code,keyDown:false) else { throw Refusal(code:"adapter-refused") }
      let characters=Array(key.text.utf16)
      for event in [press,release] {
        event.flags=key.shift ? .maskShift : []
        characters.withUnsafeBufferPointer { buffer in event.keyboardSetUnicodeString(stringLength:characters.count,unicodeString:buffer.baseAddress!) }
      }
      down=false;up=false
      try ui32MotionKeyPair({ try ui32MotionFocus(pinned,search,typing,contentScope:contentScope) },
        down:{press.postToPid(pinned.pid);down=true},up:{release.postToPid(pinned.pid);up=true;postedPairs += 1},
        releaseIdentity:{try verifyOwnedWindow(pinned)})
    }
    return ["postedPairs":postedPairs,"note":"Exact PID event posting, not application acknowledgement or acceptance."]
  } catch {
    var facts:[String:Any]=["purpose":"UI32","command":command,"postedPairs":postedPairs,"lastDownPosted":down,"lastUpPosted":up,"retry":false]
    if let failure=error as? CS7InputFailure {facts["inputCleanup"]=failure.diagnostics}
    throw ActionRefusal(diagnostics:facts)
  }
}
func ui32KeysSelfTest() throws {
  try ui32IdentifierSelfTest()
  try ui32MotionScopeSelfTest()
  let first=try ui32Key("P"), shifted=try ui32Key("shift+T")
  try check(first.code == 35 && shifted.shift)
  for denied in ["Enter","z","super+s","shift+U","click","", "PP"] {
    var refused=false;do { _ = try ui32Key(denied) } catch { refused=true };try check(refused)
  }
}

// Actual certificate + production key-pair route; no event objects/native AX.
func ui32MotionScopeSelfTest() throws {
  for fault in ["none","outside-focus","outside-context","duplicate-context","missing-context",
    "wrong-context","move-after-down","move-after-up","unreadable-context"] {
    let graph=[0:[1,4],1:[2],2:[3],3:[],4:[]],parents=[1:0,2:1,3:2,4:0]
    var focus=fault == "outside-focus" ? 4 : fault == "wrong-context" ? 1 : 3
    var down=0,up=0,calls=0,refused=false
    let owner:(Int)->(Int,Bool,Int)={_ in (0,true,7)}
    let children:(Int)->(Int,[Int]?)={(0,graph[$0] ?? [])}
    let parent:(Int)->(Int,Int?)={(0,parents[$0])}
    do {
      try ui32MotionKeyPair({
        calls += 1
        let certificate=try ui32ContentCertificate(0,pid:7,retained:1,deadline:10,now:{0},equal:==,
          owner:owner,prepare:{_ in true},traits:{($0 == 0 ? "AXWindow" : $0 == 1 ? "AXWebArea" : "AXGroup",false)},
          children:children,parent:parent,strict:{web,deadline in
            try boundedOwnedTree(web,owner:7,deadline:deadline,now:{0},equal:==,owner:owner,children:children,parent:parent)
          })
        try ui32MotionContent(certificate,focus,equal:==,identifier:{ node in
          if fault == "unreadable-context" && node == 4 {throw Refusal(code:"adapter-refused")}
          if fault == "missing-context" {return nil}
          return node == 2 || (fault == "outside-context" && node == 4) ||
            (fault == "duplicate-context" && node == 3) ? "motion-props-body" : nil
        })
      },down:{down += 1;if fault == "move-after-down" {focus=4}},
        up:{up += 1;if fault == "move-after-up" {focus=4}})
    } catch {refused=true}
    try check(refused == (fault != "none"))
    if fault == "none" {try check(down == 1 && up == 1 && calls == 3)}
    else if fault == "move-after-down" {try check(down == 1 && up == 0 && calls == 3)}
    else if fault == "move-after-up" {try check(down == 1 && up == 1 && calls == 3)}
    else {try check(down == 0 && up == 0 && calls == 1)}
  }
}

// No AX/process calls: inject the same reader used by all DOM ID routes.
func ui32IdentifierSelfTest() throws {
  let ok=Int(AXError.success.rawValue),absent=Int(AXError.attributeUnsupported.rawValue),none=Int(AXError.noValue.rawValue)
  let cases:[(Int,String?,Int,String?,String?,String)]=[
    (ok,"motion-props-body",ok,"","motion-props-body","dom"),
    (absent,nil,ok,"motion-props-body","motion-props-body","native"),
    (ok,"motion-props-body",none,nil,"motion-props-body","dom"),
    (ok,"same",ok,"same","same","dom+native"),
    (ok,"",ok,"",nil,"none"),(none,nil,absent,nil,nil,"none"),
    (ok,"",ok," untouched "," untouched ","native")]
  for (ds,dom,ns,native,resolved,source) in cases {
    var owners=0,prepares=0,keys=[String]()
    let result=try ui32ReadIdentifiers(7,owned:{node in owners += 1;return node == 7},
      prepare:{_ in prepares += 1;return true},read:{_,key in
        keys.append(key);return key == "AXDOMIdentifier" ? (ds,dom) : (ns,native)
      })
    try check(result.dom == dom && result.native == native && result.resolved == resolved && result.source == source)
    try check(result.domStatus == ds && result.nativeStatus == ns)
    try check(keys == ["AXDOMIdentifier",kAXIdentifierAttribute] && owners == 6 && prepares == 2)
    try check(result.observation["dom"] as? String == (dom ?? "") && result.observation["native"] as? String == (native ?? ""))
  }
  for pair in [("a","b"),("a","a "),("\u{e9}","e\u{301}")] {
    var refused=false
    do { _ = try ui32ReadIdentifiers(7,owned:{_ in true},prepare:{_ in true},
      read:{_,key in (ok,key == "AXDOMIdentifier" ? pair.0 : pair.1)}) } catch { refused=true }
    try check(refused)
  }
  let malformed:[(Int,Any?)]=[(ok,nil),(ok,true),(ok,7),(ok,["id"]),(ok,NSNull()),
    (Int(AXError.cannotComplete.rawValue),nil),(absent,"unexpected"),(none,"unexpected")]
  for bad in malformed {
    for badDOM in [true,false] {
      var refused=false,reads=0
      do { _ = try ui32ReadIdentifiers(7,owned:{_ in true},prepare:{_ in true},read:{_,key in
        reads += 1;return (key == "AXDOMIdentifier") == badDOM ? bad : (ok,"same")
      }) } catch { refused=true }
      try check(refused && reads == (badDOM ? 1 : 2))
    }
  }
  for failure in 1...6 {
    var owners=0,prepares=0,reads=0,refused=false
    do { _ = try ui32ReadIdentifiers(7,owned:{_ in owners += 1;return owners != failure},
      prepare:{_ in prepares += 1;return true},read:{_,_ in reads += 1;return (ok,"same")}) } catch { refused=true }
    try check(refused && owners == failure && reads == [0,0,1,1,1,2][failure-1])
    try check(prepares == [0,1,1,1,2,2][failure-1])
  }
  for failure in 1...2 {
    var prepares=0,reads=0,refused=false
    do { _ = try ui32ReadIdentifiers(7,owned:{_ in true},prepare:{_ in prepares += 1;return prepares != failure},
      read:{_,_ in reads += 1;return (ok,"same")}) } catch { refused=true }
    try check(refused && prepares == failure && reads == failure-1)
  }
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

// Shared typed default proof: only the three fixed New fields are queried.
func ui32ReadNewDefaults<Node>(resolve:(String,String) throws -> Node, owned:(Node)->Bool,
  prepare:(Node)->Bool, read:(Node)->(Int,Any?)) throws {
  var values=[String:String]()
  for (id,role) in [("np-name","AXTextField"),("np-fps","AXPopUpButton"),("np-preset","AXPopUpButton")] {
    let node=try resolve(id,role)
    try check(owned(node) && prepare(node) && owned(node))
    let (status,raw)=read(node);try check(owned(node) && status == Int(AXError.success.rawValue))
    guard let value=raw as? String else {throw Refusal(code:"adapter-refused")};values[id]=value
  }
  try ui32NewDefaults(values)
}
// Caller obtains a fresh strict certificate first; this adds no alternative traversal.
func ui32NewContextProof(_ command:String, _ stage:Int, _ capture:Int, _ settingsOpen:Bool,
  _ certificate:UI32ContentCertificate<AXUIElement>, _ pid:pid_t) throws {
  var inside=[String:Int](),outside=Set<String>()
  for i in certificate.nodes.indices {
    guard let id=try ui32DOMIdentifiers(certificate.nodes[i],pid).resolved else {continue}
    if certificate.inside.contains(i) {inside[id,default:0] += 1} else {outside.insert(id)}
  }
  try ui32NewSemanticContext(command,true,stage,capture,settingsOpen,inside,outside)
}

// Append to existing keys source; call from existing New self-test after moving it intact.
func ui32NewSemanticSelfTest() throws {
  for name in ["New Project","Nouveau projet","新規プロジェクト","Nuevo proyecto"] {
    try ui32NewSemanticPress("entry.new.default",true,"",[name],"AXStaticText",[kAXPressAction])
  }
  for title in ["Quitter le tutoriel","Exit tutorial","Salir del tutorial","チュートリアルを終了"] {
    try ui32NewSemanticPress("entry.tutorial.close",true,"",[title,"×"],"AXButton",[kAXPressAction])
  }
  for (command,label,role,actions) in [("entry.new.default","New Project","AXButton",Set([kAXPressAction])),
    ("entry.new.default","New Project extra","AXStaticText",[kAXPressAction]),("entry.new.default","New Project","AXStaticText",[]),
    ("entry.tutorial.close","×","AXButton",[kAXPressAction])] {
    var refused=false;do {try ui32NewSemanticPress(command,true,"",[label],role,actions)}catch{refused=true};try check(refused)
  }
  let start=["start-open":1,"tools-panel-handle":1],create=["np-create":1,"np-name":1,"np-fps":1,"np-preset":1]
  let editor=["tools-panel-handle":1,"project-tabs-settings":1]
  let modal=["tools-panel-handle":1,"project-tabs-settings":1,"settings-close":1,"profile-name":1,"profile-email":1,"settings-language":1]
  // Ignored container IDs are deliberately absent; positive exposed sentinels remain mandatory.
  try ui32NewSemanticContext("entry.new.default",true,0,0,false,start,[])
  try ui32NewSemanticContext("entry.create.default",true,1,0,false,create,[])
  try ui32NewDefaults(["np-name":"Untitled","np-fps":"24","np-preset":"1920 × 1080 (Full HD)"])
  try ui32NewSemanticContext("entry.tutorial.close",true,2,2,false,editor,[])
  try ui32NewSemanticContext("settings.open",true,3,3,false,editor,[])
  try ui32NewSemanticContext("profile.name",true,3,3,true,modal,[])
  try ui32NewSemanticContext("locale.choose.restore",true,3,3,true,modal,[])
  try ui32NewSemanticContext("tool.hand",true,3,3,false,editor,[])
  for fault in ["startup-capture","postcreate-capture","postclose-capture","early-prefs","settings-phase","settings-sentinel",
    "outside-field","duplicate-field","covered-tools","missing-handle","active-create","tab"] {
    var command="profile.name",stage=3,capture=3,open=true,counts=modal,outside=Set<String>()
    switch fault {
    case "startup-capture":command="entry.new.default";stage=0;capture = -1;open=false;counts=start
    case "postcreate-capture":command="entry.tutorial.close";stage=2;capture=0;open=false;counts=editor
    case "postclose-capture":capture=2
    case "early-prefs":stage=2;capture=2
    case "settings-phase":open=false
    case "settings-sentinel":counts["settings-close"]=0
    case "outside-field":outside=["profile-name"]
    case "duplicate-field":counts["profile-name"]=2
    case "covered-tools":command="tool.hand";open=false;counts=start
    case "missing-handle":command="tool.brush";open=false;counts=[:]
    case "active-create":counts["np-create"]=1
    default:command="settings.tab"
    }
    var effects=0,refused=false
    do {try ui32NewSemanticContext(command,true,stage,capture,open,counts,outside);effects += 1}catch{refused=true}
    try check(refused && effects == 0)
  }
  let old=try ui32NewSemanticRule("settings.tab","General",false)
  try check(old.region == "settings-tabs" && old.labels == ["General"])
  try ui32NewSemanticContext("settings.tab",false,0,-1,false,[:],[])
  var refused=false;do {_ = try ui32NewSemanticRule("settings.tab","General",true)}catch{refused=true};try check(refused)
  let opened=try ui32NewSettingsNext("settings.open",true,false),closed=try ui32NewSettingsNext("settings.close",true,true)
  try check(opened && !closed)
  try check(try ui32NewSettingsNext("settings.open",false,true))
  for command in ["settings.menu.entry","playback.pause","motion.cancel-pending"] {
    var denied=false;do {_ = try ui32NewSemanticRule(command,nil,true)}catch{denied=true};try check(denied)
  }
  for command in ["tool.hand","tool.select","tool.brush"] {
    let original=try ui32Rule(command,nil),next=try ui32NewSemanticRule(command,nil,true)
    try check(next.region == nil && next.labels == original.labels && next.roles == original.roles)
  }
  for fault in ["none","resolve","owner-before","timeout","owner-after","status","type","wrong-default"] {
    var reads=0,ownerCalls=0,refused=false,requested=[String]()
    do {
      try ui32ReadNewDefaults(resolve:{ id,role in
        requested.append(id+":"+role);if fault == "resolve" {throw Refusal(code:"adapter-refused")};return id
      },owned:{ _ in
        ownerCalls += 1
        return !((fault == "owner-before" && ownerCalls == 1) || (fault == "owner-after" && ownerCalls == 3))
      },prepare:{_ in fault != "timeout"},read:{ id in
        reads += 1
        let defaults=["np-name":"Untitled","np-fps":"24","np-preset":"1920 × 1080 (Full HD)"]
        if fault == "status" {return (-25205,nil)}
        if fault == "type" {return (0,24)}
        return (0,fault == "wrong-default" ? "other" : defaults[id])
      })
    }catch{refused=true}
    try check(refused == (fault != "none"))
    if ["resolve","owner-before","timeout"].contains(fault) {try check(reads == 0)}
    if ["owner-after","status","type"].contains(fault) {try check(reads == 1)}
    if fault == "none" {try check(reads == 3 && requested == ["np-name:AXTextField","np-fps:AXPopUpButton","np-preset:AXPopUpButton"])}
  }
}

// Observation only: opaque nodes/routes come from the completed owned attachment census.
func ui32OwnedSheetRead<Node>(_ nodes:[Node], _ routes:[String], _ expected:URL, deadline:UInt64, limit:Int,
  now:()->UInt64, owned:(Node)->Bool, prepare:(Node)->Bool, read:(Node,String)->(Int,Any?)) throws -> [String:Any] {
  try check(!nodes.isEmpty && nodes.count <= 512 && nodes.count == routes.count && limit >= 0 && limit <= 5120)
  let categories=["AXSheet","AXWindow","AXGroup","AXButton","AXSplitGroup","AXScrollArea","AXOutline","AXRow","AXCell","AXTextField","AXStaticText"]
  var rows=[[String:Any]](), queries=0,failed=0,matched=0,completed=0,stopped=false,reason="none",calls=0
  func permit() -> Bool {
    if now() >= deadline || calls >= limit {stopped=true;reason="budget";return false}
    calls += 1;return true
  }
  func owner(_ node:Node) -> Bool {permit() && owned(node)}
  var roles=Dictionary(uniqueKeysWithValues:(categories+["other","unreadable"]).map {($0,0)})
  for i in nodes.indices {
    var row:[String:Any]=["index":i,"route":routes[i]],started=0,valid=0
    for key in ["AXRole","AXURL"] {
      guard owner(nodes[i]),permit(),prepare(nodes[i]),owner(nodes[i]),permit() else {
        if !stopped {reason="owner-or-timeout"};stopped=true;break
      }
      let (status,raw)=read(nodes[i],key);queries += 1;started += 1
      guard owner(nodes[i]) else {failed += 1;if !stopped {reason="owner-changed"};stopped=true;break}
      row[key == "AXRole" ? "roleStatus" : "urlStatus"]=status
      if key == "AXRole" {
        if status == 0,let role=raw as? String,!role.isEmpty {
          let category=categories.contains(role) ? role : "other";row["role"]=category;roles[category,default:0] += 1;valid += 1
        } else {row["role"]="unreadable";roles["unreadable",default:0] += 1;failed += 1}
      } else if status == 0,let url=raw as? URL {
        let match=ui32FixtureURLMatches(url,expected);row["urlPresence"]="present";row["expectedURLMatch"]=match
        if match {matched += 1};valid += 1
      } else if [Int(AXError.attributeUnsupported.rawValue),Int(AXError.noValue.rawValue)].contains(status) && raw == nil {
        row["urlPresence"]="absent";row["expectedURLMatch"]=false;valid += 1
      } else {row["urlPresence"]="unproven";row["expectedURLMatch"]=NSNull();failed += 1}
    }
    if started > 0 {row["queriesStarted"]=started;rows.append(row)}
    if valid == 2 {completed += 1}
    if stopped {break}
  }
  let complete = !stopped && completed == nodes.count && queries == nodes.count*2 && failed == 0 && now() < deadline
  return ["scope":"completed-owned-sheet-census-only; not the service subtree","structuralCensusComplete":true,"ownedNodeCount":nodes.count,"queriesStarted":queries,"nativeCallsStarted":calls,"nativeCallLimit":limit,
    "unqueriedQueries":nodes.count*2-queries,"queriedNodes":rows.count,"unqueriedNodes":nodes.count-rows.count,
    "completedNodes":completed,"failedQueries":failed,"truncated":stopped,"complete":complete,"stopReason":reason,
    "observedExpectedURLMatches":matched,"expectedURLAbsenceProven":complete && matched == 0,"roleCounts":roles,"nodes":rows]
}
func ui32OwnedSheetAXRead(_ nodes:[AXUIElement], _ routes:[String], _ pid:pid_t, _ expected:URL) throws -> [String:Any] {
  try ui32OwnedSheetRead(nodes,routes,expected,deadline:DispatchTime.now().uptimeNanoseconds+750_000_000,limit:5120,
    now:{DispatchTime.now().uptimeNanoseconds},owned:{owns($0,pid)},
    prepare:{AXUIElementSetMessagingTimeout($0,0.03) == .success},read:{node,key in
      var raw:CFTypeRef?;let status=AXUIElementCopyAttributeValue(node,key as CFString,&raw)
      return (Int(status.rawValue),raw)
    })
}

// Call from the existing UI32 self-test chain; no AX, process, or file operations.
func ui32OwnedSheetReadSelfTest() throws {
  let expected=URL(fileURLWithPath:"/private/fixed-fixture.json")
  for fault in ["none","no-match","unsupported","no-value","bad-role","bad-url","status","status-payload","foreign","stale","timeout","deadline","budget"] {
    var reads=[String](),owners=0
    let result=try ui32OwnedSheetRead([0],["0.2"],expected,deadline:10,limit:fault == "budget" ? 4 : 10,
      now:{fault == "deadline" ? 10 : 0},owned:{_ in
        owners += 1;return fault != "foreign" && !(fault == "stale" && owners == 3)
      },prepare:{_ in fault != "timeout"},read:{node,key in
        reads.append(key)
        if key == "AXRole" {if fault == "bad-role" {return (0,1)};return (0,"AXRow")}
        if fault == "unsupported" {return (Int(AXError.attributeUnsupported.rawValue),nil)}
        if fault == "no-value" {return (Int(AXError.noValue.rawValue),nil)}
        if fault == "status-payload" {return (Int(AXError.noValue.rawValue),expected)}
        if fault == "status" {return (-1,nil)}
        if fault == "bad-url" {return (0,"file:///private/fixed-fixture.json")}
        return (0,fault == "no-match" ? URL(fileURLWithPath:"/private/other.json") : expected)
      })
    let complete=["none","no-match","unsupported","no-value"].contains(fault)
    try check(result["complete"] as? Bool == complete)
    try check(result["expectedURLAbsenceProven"] as? Bool == ["no-match","unsupported","no-value"].contains(fault))
    if ["foreign","timeout","deadline"].contains(fault) {try check(reads.isEmpty)}
    if ["stale","budget"].contains(fault) {try check(reads == ["AXRole"])}
    if fault == "none" {try check(result["observedExpectedURLMatches"] as? Int == 1 && result["queriesStarted"] as? Int == 2)}
    try check(result["unqueriedQueries"] as? Int == 2-reads.count)
  }
}
func ui32ContentWalkSelfTest() throws {
  for fault in 0...2 {
    var calls=[String](),facts=[String:Any](),rows=[Int]()
    do {rows=try ui32ContentWalk(0,owner:7,deadline:10,now:{0},equal:==,
      owner:{calls.append("o\($0)");return (0,true,7)},
      children:{calls.append("c\($0)");return (0,$0 < 2 ? [$0+1] : [])},
      parent:{calls.append("p\($0)");return $0 == 2 && fault == 1 ? (-1,nil) : (0,$0 == 2 && fault == 2 ? 0 : $0-1)})}
    catch let error as ActionRefusal {facts=error.diagnostics}
    try check(calls == ["o0","c0","o1","p1","o1","c1","o2","p2"]+(fault == 0 ? ["o2","c2"] : []))
    if fault == 0 {try check(rows == [0,1,2] && facts.isEmpty)}
    else {
      let trace=facts["contentTraversal"] as? [String:Any]
      try check(facts["stage"] as? String == (fault == 1 ? "parent-query" : "parent-edge"))
      try check(trace?["routes"] as? [String] == ["0.0"] && trace?["queryError"] as? Int == (fault == 1 ? -1 : 0))
    }
  }
}
