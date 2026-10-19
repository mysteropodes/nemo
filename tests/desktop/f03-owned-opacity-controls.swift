// Fault injection uses the exact F03InputKernel called by production AX.
import Foundation

func f03OwnedOpacitySelfTest() throws -> Int {
  var count=0
  let expected:[F03Input:String]=[.opacity40:"40",.opacity60:"60",.frame0:"1",.frame10:"11",.frame20:"21"]
  for input in F03Input.allCases {
    try check(try f03Input(input.rawValue,nil,nil) == input)
    try check(input.value == expected[input]);count += 1
  }
  for command in ["f03.opacity.25","f03.opacity.99","f03.frame.1","f03.frame.21","f03.scrub","f03.file.path","f03.opacity.40 ",""] {
    var refused=false
    do {_ = try f03Input(command,nil,nil)}catch{refused=true}
    try check(refused);count += 1
  }
  for values in [("anything",nil),(nil,"40"),("",nil),(nil,"")] as [(String?,String?)] {
    var refused=false
    do {_ = try f03Input("f03.opacity.40",values.0,values.1)}catch{refused=true}
    try check(refused);count += 1
  }
  // Real static-stage guard rejects keyed7744 even at displayed1/value25,
  // wrong current admission and frame11/21 BEFORE any injected effect.
  for input in [F03Input.opacity40,.opacity60,.undo,.redo] {
    for fixture in ["f03-initial","f03-original"] {
      for confirmed in ["f03-initial","f03-original",nil] as [String?] {
        for frame in ["1","11","21",""] {
          var accepted=false,effects=0
          do {
            try f03StaticInputProof(input,originalKind:fixture,confirmedKind:confirmed,displayedFrame:frame)
            effects += 1;accepted=true
          }catch{}
          let allowed=fixture == "f03-initial" && confirmed == "f03-initial" && frame == "1"
          try check(accepted == allowed && effects == (allowed ? 1 : 0));count += 1
        }
      }
    }
  }
  // Actual menu/path predicate: two authenticated-looking menus do not
  // authorize a detached same-title entry; parent/owner/duplicate faults refuse.
  for fault in ["none","detached","foreign-selected","foreign-witness","foreign-menu",
    "parent-failure","children-failure","missing-child","duplicate-child","cycle","missing-signature","duplicate-signature"] {
    var rows=[0:[1,2,8],1:[3,4,5],2:[9],3:[6],4:[],5:[],6:[],8:[7,10,11],7:[],9:[],10:[],11:[]]
    var parents=[1:0,2:0,8:0,3:1,4:1,5:1,6:3,9:2,7:8,10:8,11:8]
    if fault == "missing-child" {rows[1]=[4,5]}
    if fault == "duplicate-child" {rows[1]=[3,3,4,5]}
    if fault == "cycle" {parents[3]=3;rows[3]=[3,6]}
    if fault == "duplicate-signature" {rows[1]=[3,4,5,10]}
    var accepted=false
    do {
      let menu=try f03MenuBinding(fault == "detached" ? 7 : 6,3,equal:==,
        owned:{node in !(fault == "foreign-selected" && node == 6 || fault == "foreign-witness" && node == 3 || fault == "foreign-menu" && node == 1)},
        parent:{node in
          if fault == "parent-failure" {throw Refusal(code:"parent-control")}
          guard let value=parents[node] else {throw Refusal(code:"parent-control")};return value
        },children:{node in
          if fault == "children-failure" {throw Refusal(code:"children-control")};return rows[node] ?? []
        },identifier:{node in
          if fault == "missing-signature" && node == 5 {return nil}
          return node == 4 || node == 10 ? "ctx-export" : node == 5 || node == 11 ? "ctx-render-manager" : nil
        })
      try check(menu == 1);accepted=true
    }catch{}
    try check(accepted == (fault == "none"));count += 1
  }
  // Inject each proof/effect failure into the real ordered transaction.
  // No downstream query, compensating key-up or retry may follow failure.
  let order=["identity1","scope","identity2","write","identity3","post"]
  for fault in order+["none"] {
    let kernel=F03InputKernel();var events=[String](),ids=0,refused=false
    func port(_ name:String) throws {
      events.append(name)
      if name == fault {throw Refusal(code:"fault-control")}
    }
    do {
      try kernel.effect("native-input",identity:{ids += 1;try port("identity"+String(ids))},
        before:{try port("scope")},write:{try port("write")},after:{try port("post")})
    }catch{refused=true}
    let offset=order.firstIndex(of:fault)
    try check(events == (offset.map {Array(order.prefix($0+1))} ?? order))
    try check(refused == (fault != "none") && kernel.failed == refused)
    let writeAttempted=offset == nil || offset! >= 3
    let writeReturned=offset == nil || offset! > 3
    try check(kernel.attempted == (writeAttempted ? ["native-input"] : []))
    try check(kernel.acknowledged == (writeReturned ? ["native-input"] : []))
    if refused {
      let before=events;var blocked=false
      do {try kernel.effect("retry",identity:{events.append("BAD identity")},
        before:{events.append("BAD scope")},write:{events.append("BAD retry")},after:{events.append("BAD post")})}catch{blocked=true}
      try check(blocked && events == before)
    }
    count += 1
  }
  // Prior successful input remains in the receipt when the next write fails.
  let kernel=F03InputKernel();var reads=0,writes=0
  try kernel.effect("focus",identity:{reads += 1},before:{reads += 1},write:{writes += 1},after:{reads += 1})
  do {try kernel.effect("value",identity:{reads += 1},before:{reads += 1},write:{writes += 1;throw Refusal(code:"value-control")},after:{reads += 100})}catch{}
  try check(kernel.failed && kernel.attempted == ["focus","value"] && kernel.acknowledged == ["focus"])
  try check(writes == 2 && reads == 8);count += 1
  let stopped=F03InputKernel();stopped.stop();var touched=false
  do {try stopped.effect("stopped",identity:{touched=true},before:{touched=true},write:{touched=true},after:{touched=true})}catch{}
  try check(!touched && stopped.failed);count += 1
  return count
}
