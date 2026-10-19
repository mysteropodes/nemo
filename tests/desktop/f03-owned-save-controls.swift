// Source-authored controls. No installed/native/client acceptance from fixtures.
import Foundation
import Darwin
import ApplicationServices

func f03OwnedSaveSelfTest() throws -> Int {
  var count=0
  // Actual read predicate: optional absence never bypasses post-owner or
  // current-context/deadline proof. Failed owner must not query context.
  for status in [AXError.attributeUnsupported,.noValue] {
    for fault in ["none","owner-drift","expired-deadline"] {
      for present in [false,true] {
        for optional in [false,true] {
          var events=[String](),accepted=false,absence=false
          let raw:String?=present ? "unexpected" : nil
          do {
            let result=try f03ReadResult(status,raw,optional:optional,owned:{
              events.append("owner");return fault != "owner-drift"
            },context:{
              events.append("context")
              if fault == "expired-deadline" {throw Refusal(code:"deadline-control")}
            })
            accepted=true;absence=result == nil
          }catch{}
          let allowed=fault == "none" && optional && !present
          try check(accepted == allowed && absence == allowed)
          try check(events == (fault == "owner-drift" ? ["owner"] : ["owner","context"]))
          count += 1
        }
      }
    }
  }
  for status in [AXError.success,.failure] {
    var events=[String](),accepted=false,result:String?
    do {result=try f03ReadResult(status,"value",optional:true,
      owned:{events.append("owner");return true},context:{events.append("context")});accepted=true}catch{}
    try check(accepted == (status == .success) && events == ["owner","context"])
    if accepted {try check(result == "value")};count += 1
  }
  let commands=["f03.save.goto","f03.save.directory","f03.save.name","f03.save.confirm","f03.save.receipt"]
  for phase in 0...5 {
    for command in commands+["f03.save.path","f03.save.reopen","", "f03.save.confirm "] {
      var accepted=false
      do {let next=try f03SaveNext(phase,command);try check(next == phase+1);accepted=true}catch{}
      try check(accepted == (phase < commands.count && commands[phase] == command));count += 1
    }
  }
  for option in ["/private/other", "", "F03-static-saved.project.json"] {
    var refused=false
    do {_ = try f03SaveNext(0,commands[0],option,nil)}catch{refused=true}
    try check(refused);count += 1
  }
  for value in ["/private/other", "", "caller-hash"] {
    var refused=false
    do {_ = try f03SaveNext(0,commands[0],nil,value)}catch{refused=true}
    try check(refused);count += 1
  }
  for own in 0...2 {
    for service in 0...2 {
      var accepted=false
      do {let chosen=try f03SaveDomain(own,service,samePID:false);try check(chosen == (own == 1));accepted=true}catch{}
      try check(accepted == (own+service == 1));count += 1
    }
  }
  for service in 0...2 {
    var accepted=false
    do {try check(try f03SaveDomain(0,service,samePID:true) == false);accepted=true}catch{}
    try check(accepted == (service == 1));count += 1
  }
  // A duplicate or absent candidate is not first-match selection.
  for nodes in [[1,2,3],[1,2,2,3],[1,3],[1,2,4]] {
    var accepted=false
    do {_ = try f03Unique(nodes,equal:==,matches:{$0 == 2 || $0 == 4});accepted=true}catch{}
    try check(accepted == (nodes == [1,2,3]));count += 1
  }
  let target:[String:Any]=["contextId":"scene-root","layerUid":"r08_curve_layer"]
  let opacity:[String:Any]=["property":"opacity","static":[25]]
  let drawing:[String:Any]=["format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
    "layers":[["layerUid":"r08_curve_layer"]],"objects":[],"propertyHolders":[["target":target,"properties":[opacity]]]]
  let project:[String:Any]=["canvasW":320,"canvasH":180,"fps":24,"canvasBg":"#ffffff","waIn":0,"waOut":20]
  let envelope:[String:Any]=["format":"nemo.native-project-document","formatVersion":1,"drawing":drawing,"project":project]
  let expected=try f03Canonical(drawing),valid=try f03Canonical(envelope)
  for fault in ["none","keyed20","opacity-track","expression","missing-static","boolean-static","duplicate-opacity","wrong-target",
    "wrong-layer","changed-drawing","wrong-canvas","wrong-envelope","oversize","malformed"] {
    var candidate=envelope,body=drawing,record=opacity,holderTarget=target,metadata=project
    switch fault {
    case "keyed20":record["static"]=[20]
    case "opacity-track":record["track"]=["keys":[["frame":0,"v":[20]],["frame":20,"v":[80]]]]
    case "expression":record["expression"]=["enabled":true]
    case "missing-static":record.removeValue(forKey:"static")
    case "boolean-static":record["static"]=[true]
    case "wrong-target":holderTarget["layerUid"]="other"
    case "wrong-layer":body["layers"]=[["layerUid":"other"]]
    case "changed-drawing":body["objects"]=[["sentinel":"different"]]
    case "wrong-canvas":metadata["canvasW"]=640
    case "wrong-envelope":candidate["format"]="other"
    default:break
    }
    body["propertyHolders"]=[["target":holderTarget,"properties":fault == "duplicate-opacity" ? [record,record] : [record]]]
    candidate["drawing"]=body;candidate["project"]=metadata
    let data:Data
    if fault == "oversize" {data=Data(repeating:32,count:32001)}
    else if fault == "malformed" {data=Data("bad".utf8)}
    else {data=try f03Canonical(candidate)}
    var accepted=false
    do {try f03SavedProject(data,expectedDrawing:expected);accepted=true}catch{}
    try check(accepted == (fault == "none"));count += 1
  }
  // Actual temporary-file controls exercise the production fd/hash/lineage
  // reader. These synthetic control bytes are never an installed receipt.
  let scratch=FileManager.default.temporaryDirectory.resolvingSymlinksInPath().appendingPathComponent("f03-save-controls-"+UUID().uuidString)
  try FileManager.default.createDirectory(at:scratch,withIntermediateDirectories:false)
  defer{try? FileManager.default.removeItem(at:scratch)}
  let file=scratch.appendingPathComponent(f03SavedName)
  try valid.write(to:file,options:.withoutOverwriting)
  let receipt=try f03CaptureSaved(file,expectedDrawing:expected);try receipt.validate();count += 1
  try check(receipt.url == file && receipt.bytes == valid.count && receipt.sha256 == f03Hash(valid))
  var changed=envelope;changed["sentinel"]="actual bytes changed"
  try f03Canonical(changed).write(to:file)
  var refused=false;do {try receipt.validate()}catch{refused=true};try check(refused);count += 1
  try FileManager.default.removeItem(at:file)
  refused=false;do {try receipt.validate()}catch{refused=true};try check(refused);count += 1
  let elsewhere=scratch.appendingPathComponent("elsewhere.json");try valid.write(to:elsewhere)
  try FileManager.default.createSymbolicLink(at:file,withDestinationURL:elsewhere)
  refused=false;do {_ = try f03ReadSaved(file)}catch{refused=true};try check(refused);count += 1
  refused=false;do {_ = try f03ReadSaved(elsewhere)}catch{refused=true};try check(refused);count += 1
  try FileManager.default.removeItem(at:file);try valid.write(to:file)
  let linked=scratch.appendingPathComponent("hard-link.json")
  try FileManager.default.linkItem(at:file,to:linked)
  refused=false;do {_ = try f03ReadSaved(file)}catch{refused=true};try check(refused);count += 1
  return count
}

// Exercise the actual additive phase predicate; original policy remains
// the comparison oracle for every non-Save/non-initiating-Open combination.
func f03JoinSelfTest() throws -> Int {
  let commands:[String?]=[nil,"observe","quit","f03.save.goto","f03.save.directory","f03.save.name",
    "f03.save.confirm","f03.save.receipt","f03.file.open","fixture.complete","sync.complete"]
  var count=0
  for save in [false,true] {for fixture in [false,true] {for sync in [false,true] {for command in commands {
    var expected:UI32IdentityPhase?,actual:UI32IdentityPhase?
    if save {
      if !fixture && !sync,let command,["f03.save.goto","f03.save.directory","f03.save.name","f03.save.confirm","f03.save.receipt","quit"].contains(command) {
        expected=command == "f03.save.receipt" ? .nativePickerCompletion : .nativePicker
      }
    } else if fixture && command == "f03.file.open" {
      if !sync {expected = .nativePicker}
    } else {expected=try? ui32IdentityPhase(command,fixtureActive:fixture,syncActive:sync)}
    actual=try? f03JoinedPhase(command,saveActive:save,fixtureActive:fixture,syncActive:sync)
    try check(actual == expected);count += 1
  }}}}
  // Real initial-stage admission: no native/AX effect is reachable for Save.
  for input in [F03Input.save,.saveAs] {for initial in [false,true] {for generated in [false,true] {for prepared in [false,true] {
    var effects=0,accepted=false
    do {try f03SaveEntryProof(input,initial:initial,generatedCurrent:generated,savePrepared:prepared);effects += 1;accepted=true}catch{}
    let allowed=input == .saveAs && initial && !generated && !prepared
    try check(accepted == allowed && effects == (allowed ? 1 : 0));count += 1
  }}}}
  // Actual response join preserves uncertainty/ACK arrays and receipt-only
  // no-input metadata without turning a generic default into effect evidence.
  let metadata:[[String:Any]]=[
    ["attempted":["press"],"acknowledged":["press"]],
    ["attempted":["press"],"acknowledged":[String]()],
    ["inputAttempted":false,"attempted":[String](),"acknowledged":[String]()]]
  for facts in metadata {
    let result=f03Result(["status":"ok","actionPosted":false],facts)
    try check(result["actionPosted"] == nil && result["status"] as? String == "ok")
    try check(result["attempted"] as? [String] == facts["attempted"] as? [String] && result["acknowledged"] as? [String] == facts["acknowledged"] as? [String])
    try check(result["inputAttempted"] as? Bool == facts["inputAttempted"] as? Bool);count += 1
  }
  return count
}
// The real UI32 selftest caller reports these returned counts; no synthetic pass.
func f03SelfTests() throws -> [String:Int] {
  let opacity=try f03OwnedOpacitySelfTest(),save=try f03OwnedSaveSelfTest(),join=try f03JoinSelfTest()
  try check(opacity == 140 && save == 122 && join == 107)
  return ["opacity":opacity,"save":save,"total":opacity+save,"join":join,"all":opacity+save+join]
}
