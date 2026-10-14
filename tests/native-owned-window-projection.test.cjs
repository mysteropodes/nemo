'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),crypto=require('node:crypto');
const assert=require('node:assert/strict'),test=require('node:test'),{spawnSync}=require('node:child_process');
const B=require('./desktop/p38-owned-window-build.cjs');
function fixture(){
  const root=fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(),'owned-projection-')));
  for(const name of B.INPUTS)fs.writeFileSync(path.join(root,name),'// '+name+'\n');
  return root;
}
test('helper compilation identity covers both ordered files and refuses altered, missing, reordered or aliased input',()=>{
  const root=fixture();try{
    const m=B.sourceManifest(root),p={sourceSha256:m.sourceSha256,inputs:m.inputs};
    assert.equal(B.verifySourceProof(p,root),m.sourceSha256);assert.equal(m.inputs.length,2);
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
  const module=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-projection.swift'),'utf8');
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
      if owners[n] != 10 {foreignReads+=1;return nil}
      if failRead.contains(n){return nil}
      if n==1 {rootQueries+=1;if mutateAfterQuery && rootQueries>1{return Array((childrenByNode[1] ?? []).reversed())}}
      return childrenByNode[n] ?? []
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
test('actual fresh Swift role/control guard refuses stale, forged and malformed targets before effect',()=>{
  if(process.platform!=='darwin')return;
  const source=fs.readFileSync(path.join(__dirname,'desktop/p38-owned-window.swift'),'utf8');
  const start=source.indexOf('        if args[1] == "--png-persistent" || request["expectedControl"] != nil'),end=source.indexOf('        if location != nil {',start);
  assert.ok(start>=0&&end>start);const guard=source.slice(start,end);
  const code=`import Foundation
struct Refusal:Error{let code:String};func check(_ ok:Bool)throws{if !ok{throw Refusal(code:"adapter-refused")}}
let kAXDescriptionAttribute="description"
var effects=0
func press(_ request:[String:Any],_ rawRole:String="AXButton",_ title:String="",_ value:String="",_ description:String="Open Project…",_ mode:String="--png-persistent")throws{
  let args=["helper",mode]
  let selected=1;func attr(_ n:Int,_ key:String)->Any?{description}
`+guard+`
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
try press([:],"AXButton","","","","--persistent")
try press([:],"AXButton","","","","--reference-persistent")
precondition(effects==3)
print("actual semantic guard, no refused effects; runtimeAX=false")
`;
  const r=spawnSync('/usr/bin/swift',['-e',code],{encoding:'utf8',timeout:30000,maxBuffer:8192});assert.equal(r.status,0,r.stderr);
  assert.match(r.stdout,/no refused effects/);
});
