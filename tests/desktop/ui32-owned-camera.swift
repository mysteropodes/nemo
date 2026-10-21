// T28 installed Camera View support in the existing owned UI32 helper.
// Only fixed fixture/semantic commands. No camera evaluation, document/API writer or global input.
import Foundation
import ApplicationServices
import CoreGraphics

let ui32CameraCommands:Set<String> = ["camera.observe","camera.toggle","camera.pan","camera.wheel.in",
  "camera.key.in","camera.key.out","camera.key.fit","camera.zoom.in","camera.fit","camera.reset"]
func ui32CameraFixture() -> (name:String,size:Int,hash:String,f03:Bool) {
  ("Camera-view.json",4269,"0019e59ef3f204ec8879911da6a0fab121e8491f93caf2537270622921a8610b",false)
}
func ui32CameraRule(_ id:String, _ roles:Set<String> = ["AXButton"]) -> UI32Rule {
  UI32Rule(identifier:id,region:nil,labels:[],roles:roles,operation:"press",values:[])
}
struct UI32CameraView:Equatable {
  let frame:Int, pressed:Bool, bounds:CGRect
}
func ui32CameraFrame(_ raw:Any?) throws -> Int {
  guard let value=raw as? String, value.range(of:"^[1-9][0-9]?$",options:.regularExpression) != nil,
    let number=Int(value),number<=21 else {throw Refusal(code:"adapter-refused")}
  return number-1
}
func ui32CameraPressed(_ raw:Any?) throws -> Bool {
  guard let value=raw as? NSNumber, value.doubleValue.isFinite,
    value.doubleValue == 0 || value.doubleValue == 1 else {throw Refusal(code:"adapter-refused")}
  return value.doubleValue == 1
}
func ui32CameraBounds(_ rect:CGRect, _ window:CGRect) -> Bool {
  [rect.minX,rect.minY,rect.width,rect.height,window.minX,window.minY,window.width,window.height].allSatisfy{$0.isFinite}
    && rect.width>40 && rect.height>40 && window.width>0 && window.height>0 && window.contains(rect)
}
// Shared production/static-control fence; never retry or post an unproved release.
func ui32CameraEvents(_ fence:() throws->Void, _ events:[() throws->Void]) throws {
  for event in events {try fence();try event();try fence()}
}
// A returned AX error does not prove that the action had no effect.
final class UI32CameraActions {
  private(set) var attempted=0, posted=0
  func press(_ action:()->AXError) throws {
    attempted += 1
    try check(action() == .success)
    posted += 1
  }
  func post(_ action:()->Void) {attempted += 1;action();posted += 1}
  var diagnostics:[String:Any] {
    ["attemptedInputs":attempted,"postedInputs":posted,"actionAttempted":attempted>0,
      "actionPosted":posted>0,"effectUncertain":attempted>posted]
  }
}
final class UI32CameraOperator {
  let pinned:OwnedInitialWindow
  var retainedButton:AXUIElement?, retainedCanvas:AXUIElement?, observed:UI32CameraView?
  init(_ pinned:OwnedInitialWindow) {self.pinned=pinned}
  func perform(_ command:String, resolve:(UI32Rule) throws->AXUIElement,
    visible:(AXUIElement) throws->Void, certificate:() throws->UI32ContentCertificate<AXUIElement>,
    fixture:() throws->Void) throws -> [String:Any] {
    try check(ui32CameraCommands.contains(command));try fixture();try verifyOwnedWindow(pinned)
    func proof(_ keyboard:Bool=false) throws -> UI32CameraView {
      try fixture();try verifyOwnedWindow(pinned)
      let current=try certificate()
      func member(_ node:AXUIElement) throws {
        try check(owns(node,pinned.pid))
        guard let i=current.nodes.firstIndex(where:{CFEqual($0,node)}), current.inside.contains(i) else {
          throw Refusal(code:"adapter-refused")
        }
      }
      let button=try resolve(ui32CameraRule("btn-camera"))
      let canvas=try resolve(ui32CameraRule("drawing-canvas",["AXImage","AXGroup","AXLayoutArea"]))
      let frame=try resolve(ui32CameraRule("info-frame",["AXStaticText"]))
      try member(button);try member(canvas);try member(frame);try visible(button)
      if let retainedButton {try check(CFEqual(button,retainedButton))}
      if let retainedCanvas {try check(CFEqual(canvas,retainedCanvas))}
      guard let origin=xy(canvas),let size=wh(canvas),let wp=xy(pinned.main),let ws=wh(pinned.main) else {
        throw Refusal(code:"adapter-refused")
      }
      let rect=CGRect(origin:origin,size:size)
      try check(ui32CameraBounds(rect,CGRect(origin:wp,size:ws)))
      try check(attr(canvas,kAXEnabledAttribute) as? Bool != false)
      guard let focused=axElement(attr(pinned.application,kAXFocusedWindowAttribute)),
        owns(focused,pinned.pid),CFEqual(focused,pinned.main) else {throw Refusal(code:"adapter-refused")}
      for node in current.nodes {
        try check(owns(node,pinned.pid) && attr(node,kAXModalAttribute) as? Bool != true)
      }
      if keyboard {
        guard let focus=axElement(attr(pinned.application,kAXFocusedUIElementAttribute)),owns(focus,pinned.pid) else {
          throw Refusal(code:"adapter-refused")
        }
        try member(focus)
        try check(!["AXTextField","AXTextArea","AXComboBox"].contains(attr(focus,kAXRoleAttribute) as? String ?? ""))
      }
      let value=try UI32CameraView(frame:ui32CameraFrame(attr(frame,kAXValueAttribute)),
        pressed:ui32CameraPressed(attr(button,kAXValueAttribute)),bounds:rect)
      try verifyOwnedWindow(pinned);try fixture()
      retainedButton=button;retainedCanvas=canvas
      return value
    }
    let before=try proof()
    func evidence(_ value:UI32CameraView) -> [String:Any] {
      ["frame":value.frame,"pressed":value.pressed,"canvas":["x":value.bounds.minX,"y":value.bounds.minY,
        "width":value.bounds.width,"height":value.bounds.height]]
    }
    if command == "camera.observe" {observed=before;return ["camera":evidence(before),"actionPosted":false]}
    guard observed == before else {throw Refusal(code:"adapter-refused")}
    if command != "camera.toggle" {try check(before.pressed)}
    let actions=UI32CameraActions()
    func fence(_ keyboard:Bool=false) throws {try check(try proof(keyboard) == before)}
    func press(_ rule:UI32Rule) throws {
      try fence();let node=try resolve(rule);try visible(node)
      var names:CFArray?
      try check(AXUIElementCopyActionNames(node,&names) == .success && (names as? [String] ?? []).contains(kAXPressAction as String))
      try fence();try check(CFEqual(try resolve(rule),node))
      try actions.press {AXUIElementPerformAction(node,kAXPressAction as CFString)}
    }
    observed=nil // Every mutation needs a new explicit observation; no input retry.
    do {
      if command == "camera.toggle" {
        try press(ui32CameraRule("btn-camera"))
        // Toggle invalidates presentation and can hide its own button until repaint.
        // Post proves the same retained window/content/fixture, not invented paint readiness.
        try verifyOwnedWindow(pinned);try fixture();_ = try certificate()
      } else if command == "camera.fit" || command == "camera.reset" {
        try press(ui32CameraRule(command == "camera.fit" ? "btn-fit" : "btn-resetv"));try fence()
      } else if command.hasPrefix("camera.key.") {
        let code:CGKeyCode=command == "camera.key.in" ? 0x18 : command == "camera.key.out" ? 0x1B : 0x1D
        guard let source=CGEventSource(stateID:.privateState),
          let down=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:true),
          let up=CGEvent(keyboardEventSource:source,virtualKey:code,keyDown:false) else {throw Refusal(code:"adapter-refused")}
        down.flags=[.maskCommand,.maskAlternate];up.flags=down.flags
        try ui32CameraEvents({try fence(true)},[{actions.post {down.postToPid(self.pinned.pid)}},
          {actions.post {up.postToPid(self.pinned.pid)}}])
      } else {
        guard let source=CGEventSource(stateID:.privateState) else {throw Refusal(code:"adapter-refused")}
        let point=CGPoint(x:before.bounds.midX,y:before.bounds.midY)
        if command == "camera.wheel.in" {
          guard let event=CGEvent(scrollWheelEvent2Source:source,units:.pixel,wheelCount:1,wheel1:100,wheel2:0,wheel3:0) else {
            throw Refusal(code:"adapter-refused")
          }
          event.location=point
          try ui32CameraEvents({try fence()},[{actions.post {event.postToPid(self.pinned.pid)}}])
        } else {
          if command == "camera.pan" {try press(try ui32Rule("tool.hand",nil))}
          else {
            try press(UI32Rule(identifier:nil,region:"tools-panel",labels:["Zoom","ズーム","Zoom — click to zoom in, Alt+click to zoom out (Z)"],
              roles:["AXButton"],operation:"press",values:[]))
          }
          try fence()
          let end=command == "camera.pan" ? CGPoint(x:point.x+before.bounds.width/20,y:point.y) : point
          guard before.bounds.contains(end),
            let down=CGEvent(mouseEventSource:source,mouseType:.leftMouseDown,mouseCursorPosition:point,mouseButton:.left),
            let drag=CGEvent(mouseEventSource:source,mouseType:.leftMouseDragged,mouseCursorPosition:end,mouseButton:.left),
            let up=CGEvent(mouseEventSource:source,mouseType:.leftMouseUp,mouseCursorPosition:end,mouseButton:.left) else {
            throw Refusal(code:"adapter-refused")
          }
          let events=command == "camera.pan" ? [down,drag,up] : [down,up]
          try ui32CameraEvents({try fence()},events.map { event in {actions.post {event.postToPid(self.pinned.pid)}} })
        }
      }
      return ["cameraBefore":evidence(before),"attemptedInputs":actions.attempted,
        "postedInputs":actions.posted,"actionAttempted":true,"actionPosted":true,"effectUncertain":false,
        "awaitPresentedObservation":true,"note":"Input posting only; actual native pixels/source checks remain mandatory."]
    } catch {
      var diagnostics=actions.diagnostics
      diagnostics.merge(["purpose":"UI32","command":command,"retry":false,
        "underlying":ui32ActionCause(error)]) {_,new in new}
      throw ActionRefusal(diagnostics:diagnostics)
    }
  }
}
func ui32CameraSelfTest() throws -> Int {
  var cases=0
  for raw in [nil,"0","01","22","-1","1 ",true,1] as [Any?] {
    var refused=false;do {_ = try ui32CameraFrame(raw)} catch {refused=true};try check(refused);cases += 1
  }
  for (raw,expected) in [("1",0),("5",4),("21",20)] {try check(try ui32CameraFrame(raw)==expected);cases += 1}
  for raw in [nil,"true",2,-1,Double.nan,Double.infinity] as [Any?] {
    var refused=false;do {_ = try ui32CameraPressed(raw)} catch {refused=true};try check(refused);cases += 1
  }
  try check(try ui32CameraPressed(NSNumber(value:true)));try check(try ui32CameraPressed(NSNumber(value:false)) == false);cases += 2
  let good=CGRect(x:10,y:10,width:100,height:80), window=CGRect(x:0,y:0,width:200,height:200)
  try check(ui32CameraBounds(good,window));cases += 1
  for rect in [CGRect(x:-1,y:10,width:100,height:80),CGRect(x:10,y:10,width:0,height:80),CGRect(x:190,y:10,width:100,height:80)] {
    try check(!ui32CameraBounds(rect,window));cases += 1
  }
  let base=UI32CameraView(frame:4,pressed:true,bounds:good)
  for fault in 0...7 {
    var checks=0,posted=0,refused=false
    do {
      try ui32CameraEvents({checks += 1
        let current=UI32CameraView(frame:fault == 5 ? 5 : 4,pressed:fault != 6,bounds:fault == 7 ? window : good)
        try check(current == base && fault != checks)
      },[{posted += 1},{posted += 1},{posted += 1}])
    } catch {refused=true}
    try check(refused == (fault != 0))
    let expected=fault == 0 ? 3 : fault <= 4 ? fault/2 : 0
    try check(posted == expected);cases += 1
  }
  // Inject AX results through the exact production attempt marker and event fence.
  for fault in 0...2 {
    let actions=UI32CameraActions()
    var trace:[String]=[], refused=false, published=false
    do {
      try ui32CameraEvents({try check(fault != 0)},[{
        try actions.press {
          trace.append("AX")
          // Attempt must already be recorded when the real action starts.
          if actions.attempted != 1 || actions.posted != 0 {return .failure}
          return fault == 1 ? .failure : .success
        }
      },{actions.post {trace.append("next-input")}}])
      published=true
    } catch {refused=true}
    let diagnostic=actions.diagnostics
    try check(refused == (fault != 2) && published == (fault == 2))
    try check(trace == (fault == 0 ? [] : fault == 1 ? ["AX"] : ["AX","next-input"]))
    try check(diagnostic["attemptedInputs"] as? Int == (fault == 0 ? 0 : fault == 1 ? 1 : 2))
    try check(diagnostic["postedInputs"] as? Int == (fault == 2 ? 2 : 0))
    try check(diagnostic["actionAttempted"] as? Bool == (fault != 0))
    try check(diagnostic["actionPosted"] as? Bool == (fault == 2))
    try check(diagnostic["effectUncertain"] as? Bool == (fault == 1));cases += 1
  }
  try check(!ui32CameraCommands.contains("camera.key.raw") && !ui32CameraCommands.contains("camera.drag.arbitrary"));cases += 2
  let spec=ui32CameraFixture();try check(spec.name == "Camera-view.json" && spec.size == 4269 && spec.hash == "0019e59ef3f204ec8879911da6a0fab121e8491f93caf2537270622921a8610b" && !spec.f03);cases += 1
  return cases
}
