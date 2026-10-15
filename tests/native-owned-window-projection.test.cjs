'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const assert=require('node:assert/strict'),test=require('node:test'),{spawnSync}=require('node:child_process');
const B=require('./desktop/p38-owned-window-build.cjs');
function projectionCore(){
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-projection.swift'),'utf8');
  const marker='func ownedControl(',start=source.indexOf(marker);
  assert.ok(start>=0,'actual pure projection declaration must exist');
  assert.equal(source.indexOf(marker,start+marker.length),-1,'actual pure projection declaration must be unique');
  return source.slice(start);
}
function fixture(){
  const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'owned-projection-')));
  for(const name of B.INPUTS)fs.writeFileSync(path.join(root,name),'// '+name+'\n');
  return root;
}
test('helper compilation identity covers all three ordered files and refuses altered, missing, reordered or aliased input',()=>{
  const root=fixture();try{
    const m=B.sourceManifest(root),p={sourceSha256:m.sourceSha256,inputs:m.inputs};
    assert.equal(B.verifySourceProof(p,root),m.sourceSha256);assert.equal(m.inputs.length,3);
    assert.throws(()=>B.verifySourceProof({...p,inputs:[...p.inputs].reverse()},root));
    assert.throws(()=>B.verifySourceProof({...p,inputs:p.inputs.slice(0,1)},root));
    assert.throws(()=>B.verifySourceProof({...p,sourceSha256:'f'.repeat(64)},root));
    const file=path.join(root,B.INPUTS[1]);fs.appendFileSync(file,'// changed\n');
    assert.throws(()=>B.verifySourceProof(p,root));fs.unlinkSync(file);
    assert.throws(()=>B.sourceManifest(root));
    const foreign=path.join(root,'foreign');fs.writeFileSync(foreign,'// foreign\n');fs.symlinkSync(foreign,file);
    assert.throws(()=>B.sourceManifest(root));
  }finally{fs.rmSync(root,{recursive:true,force:true});}
});
test('actual extracted Swift projects owned late controls with strict retained pages and honest completeness',()=>{
  if(process.platform!=='darwin')return;
  const module=projectionCore();
  const code=`import Foundation
import CoreFoundation
struct Refusal:Error {let code:String}
func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
`+module+`
var childrenByNode=[1:Array(2...301)]
var owners=[Int:Int](),foreignReads=0
for n in 1...400 {owners[n]=10}
var failRead:Set<Int>=[],makeForeign:Int?=nil,mutateAfterQuery=false,rootQueries=0
var cursor:OwnedProjectionCursor<Int>?=nil
func execute(_ request:[String:Any],_ depth:Int=1,_ cap:Int=128,_ root:Int=1,_ base:String="web")throws->[String:Any]{
  return try ownedProjection(root,base:base,depth:depth,maxNodes:cap,expectedOwner:10,
    request:request,cursor:&cursor,equal:==,hashKey:{UInt($0)},owner:{owners[$0]},
    children:{ n in
      if owners[n] != 10 {foreignReads+=1;return OwnedChildren(query:.failed,values:nil)}
      if failRead.contains(n){return OwnedChildren(query:.failed,values:nil)}
      if n==1 {rootQueries+=1;if mutateAfterQuery && rootQueries>1{return OwnedChildren(query:.success,values:Array((childrenByNode[1] ?? []).reversed()))}}
      return OwnedChildren(query:.success,values:childrenByNode[n] ?? [])
    },isWindow:{_ in false},fact:{ n,p in
      if owners[n] != 10 {foreignReads+=1}
      if makeForeign==n {owners[n]=99}
      var row:[String:Any]=["path":p,"role":"AXButton"]
      if n==301 {row["control"]="Open Project…"};return row
    },phase:{_,_ in})
}
func refuses(_ f:()throws->Void){var hit=false;do{try f()}catch{hit=true};precondition(hit)}
let first=try execute(["childOffset":0])
precondition((first["nodes"] as! [[String:Any]]).count==128 && first["nextChildOffset"] as? Int==127)
precondition(first["truncated"] as? Bool==true && first["complete"] as? Bool==false)
let token=first["pageToken"] as! String
let second=try execute(["childOffset":127,"pageToken":token])
precondition(second["nextChildOffset"] as? Int==254)
let third=try execute(["childOffset":254,"pageToken":token])
let rows=third["nodes"] as! [[String:Any]]
precondition(rows.contains{$0["control"] as? String=="Open Project…"})
precondition(rows.last?["path"] as? String=="web.299")
precondition(third["childrenComplete"] as? Bool==true && third["complete"] as? Bool==true && cursor==nil)
refuses{_=try execute(["childOffset":254,"pageToken":token])}
owners[2]=99;_=try execute(["childOffset":0]);let foreignToken=cursor!.token;owners[2]=10
_=try execute(["childOffset":127,"pageToken":foreignToken])
let foreignEnd=try execute(["childOffset":254,"pageToken":foreignToken])
precondition(foreignEnd["foreignChildren"] as? Bool==true && foreignEnd["complete"] as? Bool==false && foreignEnd["childrenComplete"] as? Bool==false)
for request:[String:Any] in [["childOffset":true],["childOffset":"0"],["childOffset":1.5],["childOffset":-1],["childOffset":4096],
  ["childOffset":0,"pageToken":token],["pageToken":token]]{refuses{_=try execute(request)}}
refuses{_=try execute(["childOffset":0],2)}
refuses{_=try execute(["childOffset":0],1,1)}
_=try execute(["childOffset":0])
refuses{_=try execute(["childOffset":128,"pageToken":cursor!.token])}
_=try execute(["childOffset":0])
refuses{_=try execute(["childOffset":127,"pageToken":"00000000-0000-0000-0000-000000000000"])}
_=try execute(["childOffset":0]);let sameToken=cursor!.token
childrenByNode[1]!.swapAt(0,1)
refuses{_=try execute(["childOffset":127,"pageToken":sameToken])}
childrenByNode[1]=Array(2...301)
_=try execute(["childOffset":0]);let rootToken=cursor!.token
refuses{_=try execute(["childOffset":127,"pageToken":rootToken],1,128,1,"other")}
childrenByNode[1]=[2,2]
refuses{_=try execute(["childOffset":0])}
childrenByNode[1]=[2,3];mutateAfterQuery=true;rootQueries=0
refuses{_=try execute(["childOffset":0])};mutateAfterQuery=false
childrenByNode[1]=[2];childrenByNode[2]=[3]
let clipped=try execute([:])
precondition(clipped["depthClipped"] as? Bool==true && clipped["complete"] as? Bool==false)
let full=try execute([:],2)
precondition(full["complete"] as? Bool==true)
let capped=try execute([:],2,1)
precondition(capped["truncated"] as? Bool==true && capped["complete"] as? Bool==false)
owners[2]=99;let foreign=try execute([:],2)
precondition(foreign["foreignChildren"] as? Bool==true && foreignReads==0)
precondition((foreign["nodes"] as! [[String:Any]]).last?["blocked"] as? String=="child-owner")
owners[2]=10;makeForeign=2;let changed=try execute([:],2)
precondition(changed["foreignChildren"] as? Bool==true && foreignReads==0)
precondition((changed["nodes"] as! [[String:Any]]).last?["blocked"] as? String=="child-owner")
makeForeign=nil;owners[2]=10;failRead=[2];let unreadable=try execute([:],2)
precondition(unreadable["childrenUnreadable"] as? Bool==true && unreadable["complete"] as? Bool==false)
failRead=[1];refuses{_=try execute(["childOffset":0])};failRead=[]
childrenByNode[1]=Array(repeating:2,count:4096);refuses{_=try execute(["childOffset":0])}
precondition(foreignReads==0)
print("actual owned projection pages and refusal controls; runtimeAX=false")
`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:16384});
  assert.equal(r.status,0,r.stderr);assert.match(r.stdout,/actual owned projection/);
});
test('actual Swift retains semantic region identity through generic pages and refuses changed proofs',()=>{
  if(process.platform!=='darwin')return;
  const module=projectionCore();
  const code=`import Foundation
import CoreFoundation
struct Refusal:Error {let code:String};func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
`+module+`
var cursor:OwnedProjectionCursor<Int>?=nil
let full:[String:Any]=["complete":true,"truncated":false,"depthClipped":false,"childrenUnreadable":false,"foreignChildren":false]
func pin(_ node:Int=7,_ path:String="0.4",_ owner:Int=10,_ control:String?="Project tabs",_ token:String?=nil,
  _ proof:[String:Any]=full)throws->[String:Any]{
  var request:[String:Any]=["projectTabs":true];if let token{request["projectTabsToken"]=token}
  return try retainedProjectTabs(node,path:path,owner:owner,control:control,request:request,projection:proof,cursor:&cursor,equal:==)
}
func refuses(_ f:()throws->Void){var hit=false;do{try f()}catch{hit=true};precondition(hit)}
refuses{_=try pin(7,"0.4",10,nil)}
var clipped=full;clipped["depthClipped"]=true;refuses{_=try pin(7,"0.4",10,"Project tabs",nil,clipped)}
let first=try pin(),token=first["token"] as! String
precondition(UUID(uuidString:token) != nil && first["path"] as? String=="0.4")
let page=try ownedProjection(1,base:"",depth:1,maxNodes:2,expectedOwner:10,request:["childOffset":0],
  cursor:&cursor,equal:==,hashKey:{UInt($0)},owner:{_ in 10},children:{OwnedChildren(query:.success,values:$0==1 ? [2]:[])},isWindow:{_ in false},
  fact:{n,p in ["path":p,"role":"AXGroup"]},phase:{_,_ in})
precondition(page["complete"] as? Bool==true && cursor?.region?.token==token)
_=try ownedProjection(1,base:"",depth:1,maxNodes:2,expectedOwner:10,request:[:],cursor:&cursor,equal:==,
  hashKey:{UInt($0)},owner:{_ in 10},children:{_ in OwnedChildren(query:.success,values:[])},isWindow:{_ in false},fact:{_,p in ["path":p]},phase:{_,_ in})
let refreshed=try pin(7,"0.4",10,"Project tabs",token)
precondition(refreshed["token"] as? String==token)
refuses{_=try pin(8,"0.4",10,"Project tabs",token)}
refuses{_=try pin(7,"0.5",10,"Project tabs",token)}
refuses{_=try pin(7,"0.4",99,"Project tabs",token)}
refuses{_=try pin(7,"0.4",10,"Cancel",token)}
refuses{_=try pin(7,"0.4",10,"Project tabs","wrong")}
refuses{_=try pin()}
for key in ["complete","truncated","depthClipped","childrenUnreadable","foreignChildren"]{
  var proof=full;proof[key]=nil;refuses{_=try pin(7,"0.4",10,"Project tabs",token,proof)}
}
print("actual retained semantic region; runtimeAX=false")
`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:8192});
  assert.equal(r.status,0,r.stderr);assert.match(r.stdout,/actual retained semantic region/);
});
test('actual fresh Swift role/control guard refuses stale, forged and malformed targets before effect',()=>{
  if(process.platform!=='darwin')return;
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-window.swift'),'utf8');
  const start=source.indexOf('        if args[1] == "--png-persistent" || request["expectedControl"] != nil'),end=source.indexOf('        if location != nil {',start);
  assert.ok(start>=0&&end>start);const guard=source.slice(start,end);
  const begin=source.lastIndexOf('        if let control = ownedControl',start),finish=source.indexOf('\n',begin);
  assert.ok(begin>=0);const classify=source.slice(begin,finish);
  const module=projectionCore();
  const classifier=module.slice(0,module.indexOf('struct OwnedProjectionCursor'));
  const code=`import Foundation
struct Refusal:Error{let code:String};func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
`+classifier+`
let kAXDescriptionAttribute="description"
var effects=0
func press(_ request:[String:Any],_ rawRole:String="AXButton",_ title:String="",_ value:String="",_ description:String="Open Project…",_ mode:String="--png-persistent")throws{
  let args=["helper",mode]
  let scope=(reference:mode != "--persistent",report:"");var facts=[String:Any]()
  let selected=1;func attr(_ n:Int,_ key:String)->Any?{description}
`+classify+"\n"+guard+`
  effects+=1
}
try press(["expectedControl":"Open Project…","expectedRole":"AXButton"])
precondition(effects==1)
for request:[String:Any] in [[:],["expectedControl":"Open Project…","expectedRole":"AXGroup"],
  ["expectedControl":"private-secret","expectedRole":"AXButton"],["expectedControl":3,"expectedRole":"AXButton"],
  ["expectedControl":"Open Project…"],["expectedRole":"AXButton"]]{
  var hit=false;do{try press(request)}catch{hit=true};precondition(hit && effects==1)
}
var changed=false;do{try press(["expectedControl":"Open Project…","expectedRole":"AXButton"],"AXButton","","","Cancel")}catch{changed=true}
precondition(changed && effects==1)
for (expected,title,value,description) in [("Open Project…","Cancel","","Open Project…"),("Open Project…","","Cancel","Open Project…"),
  ("Open Project…","Cancel","Open Project…",""),("Cancel","Open Project…","","Cancel"),("Cancel","","Open Project…","Cancel")]{
  var hit=false;do{try press(["expectedControl":expected,"expectedRole":"AXButton"],"AXButton",title,value,description)}catch{hit=true}
  precondition(hit && effects==1)
}
try press(["expectedControl":"Cancel","expectedRole":"AXButton"],"AXButton","Cancel","","Open Project…")
precondition(effects==2)
try press([:],"AXButton","","","","--persistent")
try press([:],"AXButton","","","","--reference-persistent")
precondition(effects==4)
for mode in ["--persistent","--reference-persistent","--png-persistent"] {
  var hit=false;do{try press(["expectedControl":"Project tabs","expectedRole":"AXButton"],"AXButton","Project tabs","","Project tabs",mode)}catch{hit=true}
  precondition(hit && effects==4)
}
precondition(ownedControl(false,"Project tabs","",{nil},inspection:true)=="Project tabs")
precondition(ownedControl(true,"Project tabs","",{nil})==nil)
var descriptionReads=0
precondition(ownedControl(true,"Cancel","Open Project…",{descriptionReads+=1;return "Open Project…"})=="Cancel" && descriptionReads==0)
precondition(ownedControl(false,"Save","",{nil})==nil && ownedControl(true,"Save","",{nil})=="Save")
print("actual semantic guard, no refused effects; runtimeAX=false")
`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:8192});assert.equal(r.status,0,r.stderr);
  assert.match(r.stdout,/no refused effects/);
});
test('compiled actual owner and children adapters keep typed failures blocked without foreign reads',()=>{
  if(process.platform!=='darwin')return;
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-window.swift'),'utf8');
  const start=source.indexOf('        owner:{ value in var actual:pid_t'),end=source.indexOf('        isWindow:',start);
  assert.ok(start>=0&&end>start);const adapters=source.slice(start,end);
  const childrenAdapter=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-children-query.swift'),'utf8').replace(/^import .*$/gm,'');
  const module=projectionCore();
  const code=`import Foundation
import CoreFoundation
typealias AXUIElement=Int
enum AXError {case success,attributeUnsupported,noValue,cannotComplete}
let kAXChildrenAttribute="children"
struct Refusal:Error{let code:String}
func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
`+module+childrenAdapter+`
var fault="",foreignReads=0,attributeReads=[Int](),queryReads=[Int](),owners=[1:10,2:10],ownerCalls=[Int:Int]()
func AXUIElementGetPid(_ n:Int,_ out:inout pid_t)->AXError{
  ownerCalls[n,default:0]+=1
  if n==1 && fault=="root-owner-failed" {return .cannotComplete}
  if n==1 && fault=="root-owner-mismatch" {out=99;return .success}
  if n==2 && fault=="owner-failed" {return .cannotComplete}
  if n==2 && fault=="owner-mismatch" {out=99;return .success}
  if n==2 && fault=="owner-lost-after-fact" && ownerCalls[n,default:0]>=2 {return .cannotComplete}
  out=pid_t(owners[n]!);return .success
}
func AXUIElementCopyAttributeValue(_ n:Int,_ key:CFString,_ out:inout CFTypeRef?)->AXError{
  if owners[n] != 10 {foreignReads+=1};queryReads.append(n)
  if n==1 {if fault=="root-owner-lost-after-children"{owners[n]=99};out=[2] as NSArray;return .success}
  if fault=="owner-lost-after-children" {owners[n]=99;out=nil;return .noValue}
  if fault=="unsupported" {out=[3] as NSArray;return .attributeUnsupported}
  if fault=="no-value" {return .noValue}
  if fault=="failed" {return .cannotComplete}
  if fault=="wrong-type" {out="private-secret" as NSString;return .success}
  if fault=="success-nil" {return .success}
  out=[] as NSArray;return .success
}
func AXUIElementGetAttributeValueCount(_ n:Int,_ key:CFString,_ out:inout CFIndex)->AXError{
  if owners[n] != 10 {foreignReads+=1};return .noValue
}
func run(_ kind:String,_ depth:Int=2)throws->[String:Any]{
  let inspectOwner=10
  fault=kind;owners=[1:10,2:10];ownerCalls=[:];attributeReads=[];queryReads=[]
  var cursor:OwnedProjectionCursor<Int>?=nil
  return try ownedProjection(1,base:"",depth:depth,maxNodes:128,expectedOwner:10,request:[:],cursor:&cursor,equal:==,hashKey:{UInt($0)},
`+adapters+`
    isWindow:{ n in if owners[n] != 10 {foreignReads+=1};attributeReads.append(n);return false},
    fact:{n,p in
      if n==2 && fault.hasPrefix("fact-owner-"){return ["path":p,"role":"other","blocked":"child-owner","ownerQuery":fault=="fact-owner-failed" ? "query-failed":"mismatch"]}
      if owners[n] != 10 {foreignReads+=1};attributeReads.append(n);return ["path":p,"role":"AXButton","control":"Open","press":true]
    },phase:{_,_ in})
}
let empty=try run("none")
precondition(empty["complete"] as? Bool==true)
precondition((empty["nodes"] as! [[String:Any]]).last?["childrenQuery"] as? String=="success-empty")
for kind in ["unsupported","no-value","failed","wrong-type","success-nil"] {
  for depth in [1,2] {
    let result=try run(kind,depth),row=(result["nodes"] as! [[String:Any]]).last!
    precondition(row["childrenQuery"] as? String==(["wrong-type","success-nil"].contains(kind) ? "wrong-type":kind))
    precondition(row["ownerQuery"] as? String=="matched" && row["childCount"]==nil)
    precondition(result["childrenUnreadable"] as? Bool==true && result["complete"] as? Bool==false)
    precondition(!String(describing:result).contains("private-secret"))
  }
}
for kind in ["owner-failed","owner-mismatch","owner-lost-after-fact","owner-lost-after-children","fact-owner-failed","fact-owner-mismatch"] {
  let result=try run(kind),row=(result["nodes"] as! [[String:Any]]).last!
  precondition(result["foreignChildren"] as? Bool==true && result["complete"] as? Bool==false)
  precondition(row["blocked"] as? String=="child-owner" && row["role"] as? String=="other")
  precondition(row["ownerQuery"] as? String==(["owner-failed","owner-lost-after-fact","fact-owner-failed"].contains(kind) ? "query-failed":"mismatch"))
  precondition(row["control"]==nil && row["childrenQuery"]==nil && row["press"]==nil)
  if ["owner-failed","owner-mismatch"].contains(kind){precondition(!attributeReads.contains(2) && !queryReads.contains(2))}
  if kind=="owner-lost-after-fact"{precondition(!queryReads.contains(2))}
  if kind.hasPrefix("fact-owner-"){precondition(!queryReads.contains(2))}
}
precondition(foreignReads==0)
// Preserve the existing fatal root refusal; descendant categories cannot be
// substituted for the driver's separately scoped refused-root receipt path.
for kind in ["root-owner-failed","root-owner-mismatch","root-owner-lost-after-children"] {
  var refused=false;do{_=try run(kind)}catch let error as Refusal{refused=error.code=="child-owner"}
  precondition(refused && attributeReads.isEmpty)
  precondition(queryReads==(kind=="root-owner-lost-after-children" ? [1]:[]))
}
print("actual typed query adapters; foreignReads=0; runtimeAX=false")`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:16384});
  assert.equal(r.status,0,r.stderr);assert.match(r.stdout,/actual typed query adapters/);
});
test('compiled actual AX count binding admits only fresh same-node owned noValue SUCCESS0',()=>{
  if(process.platform!=='darwin')return;
  const projection=projectionCore();
  const adapter=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-children-query.swift'),'utf8').replace(/^import .*$/gm,'');
  const code=`import Foundation
import CoreFoundation
typealias AXUIElement=Int
enum AXError {case success,attributeUnsupported,noValue,cannotComplete}
let kAXChildrenAttribute="children"
struct Refusal:Error {let code:String}
func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
`+projection+adapter+`
var mode="",ownerCalls=0,arrayCalls=[Int](),countCalls=[Int](),order=[String]()
func AXUIElementGetPid(_ node:Int,_ out:inout pid_t)->AXError{
  precondition(node==7);ownerCalls+=1;order.append("owner")
  if mode=="foreign" {out=99;return .success}
  if mode=="owner-failed" || mode=="before-count-failed" && ownerCalls==3 || mode=="after-count-failed" && ownerCalls==4 {return .cannotComplete}
  out=mode=="after-value-mismatch" && ownerCalls==2 || mode=="after-count-mismatch" && ownerCalls==4 ? 99:10
  return .success
}
func AXUIElementCopyAttributeValue(_ node:Int,_ key:CFString,_ out:inout CFTypeRef?)->AXError{
  precondition(node==7);arrayCalls.append(node);order.append("array")
  if mode=="unsupported-array" {return .attributeUnsupported}
  if mode=="wrong-array" {out="private-secret" as NSString;return .success}
  if mode=="success-nil" {return .success}
  if mode=="inconsistent" {out=[] as NSArray;return .noValue}
  return .noValue
}
func AXUIElementGetAttributeValueCount(_ node:Int,_ key:CFString,_ out:inout CFIndex)->AXError{
  precondition(node==7);countCalls.append(node);order.append("count")
  if mode=="count-failed" {return .cannotComplete}
  if mode=="count-unsupported" {return .attributeUnsupported}
  if mode=="count-no-value" {return .noValue}
  if mode=="count-positive" {out=1;return .success}
  if mode=="count-invalid" {out = -1;return .success}
  if mode=="count-unwritten" {return .success}
  out=0;return .success
}
func run(_ value:String)->OwnedChildren<Int>{
  mode=value;ownerCalls=0;arrayCalls=[];countCalls=[];order=[]
  return ownedAXChildren(7,expectedOwner:10)
}
let good=run("zero")
precondition(good.query == .noValue && good.values?.isEmpty==true && good.witness=="empty-count-confirmed" && good.countQuery == .zero)
precondition(arrayCalls==[7] && countCalls==[7] && order==["owner","array","owner","owner","count","owner"])
for fault in ["count-positive","count-failed","count-unsupported","count-no-value","count-invalid","count-unwritten",
 "foreign","owner-failed","after-value-mismatch","before-count-failed","after-count-failed","after-count-mismatch",
 "unsupported-array","wrong-array","success-nil","inconsistent"]{
  let result=run(fault);precondition(result.values==nil && result.witness==nil)
  if ["foreign","owner-failed"].contains(fault){precondition(arrayCalls.isEmpty && countCalls.isEmpty)}
  if ["after-value-mismatch","before-count-failed","unsupported-array","wrong-array","success-nil","inconsistent"].contains(fault){precondition(countCalls.isEmpty)}
}
// An old value/count can never be reused for the next observation.
_ = run("zero");let changed=run("count-positive");precondition(changed.values==nil && changed.witness==nil && countCalls==[7])
var cursor:OwnedProjectionCursor<Int>?=nil,unownedFacts=0
do { _ = try ownedProjection(7,base:"",depth:2,maxNodes:128,expectedOwner:10,request:[:],cursor:&cursor,
 equal:==,hashKey:{UInt($0)},owner:{_ in 10},children:{_ in OwnedChildren(query:.failed,values:nil,ownerRefusal:.failed)},
 isWindow:{_ in unownedFacts+=1;return false},fact:{_,_ in unownedFacts+=1;return [:]},phase:{_,_ in})
  preconditionFailure("stale root accepted")
} catch let error as Refusal { precondition(error.code=="child-owner") }
precondition(unownedFacts==0)
print("actual owned count witness, 16 negatives, runtimeAX=false")
`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:8192});
  assert.equal(r.status,0,r.stderr);assert.match(r.stdout,/16 negatives, runtimeAX=false/);
});
