'use strict';
/* Controlled endpoint only. Reuses the real SDK/read/item/caller modules and the
 * accepted item session/replay double. Actual Rust ownership is tested separately. */
const SDK=require('../src/js/adapters/native-application.js');
const Tool=require('../src/js/adapters/native-text-tool.js');
const Reader=require('../src/js/adapters/native-text-read.js'),Writer=require('../src/js/adapters/native-object-text-layout.js');
const {createFixture}=require('./native-object-item-support.cjs');
const clone=v=>JSON.parse(JSON.stringify(v));
const deferred=()=>{let resolve,reject;const promise=new Promise((a,b)=>{resolve=a;reject=b;});return {promise,resolve,reject};};
const block=(text='Existing text')=>({text,font:{kind:'bundled',fontKey:'Roboto-Regular'},size:48,color:'#112233',align:'left',anchorTopLeft:{x:10,y:20},bold:true,letterSpacing:2});
const target=(id='native-text:1:0',frame=0)=>({contextId:'scene-root',layerUid:'L0',frameScope:{kind:'authored',frame},strokeId:id});
function fixture(options={}){
  const f=createFixture(options),targets=new Map(),reads=[],drafts=[],committed=[],states=[];
  let frame=0,layer='L0',selection='selection0',view='view0',presented=true,hold=null;
  const sdk=SDK.createNativeApplicationAdapter('native-text-fixture',{dispatch:async q=>{
    if(q.operation!=='query.document.object.text'){
      try{return await f.owner.dispatch(q);}finally{
        // The owner double allocates the map key. Never synthesize an item id in this helper.
        if(q.payload.command==='object.text.create')for(const[id,item]of f.owner.items)if(item===q.payload.block)targets.set(id,{...clone(q.payload.target),strokeId:id});
        if(q.payload.command==='object.text.delete')for(const id of targets.keys())if(!f.owner.items.has(id))targets.delete(id);
      }
    }
    reads.push(clone(q));const p=f.session.info(),v=q.payload.view;
    const base={instanceId:p.instanceId,documentId:p.documentId,atRevision:p.contentRevision,documentSnapshotId:p.documentSnapshotId};let result;
    if(v.kind==='targets'){
      const all=[...targets.values()].filter(t=>t.contextId===v.scope.contextId&&t.layerUid===v.scope.layerUid&&JSON.stringify(t.frameScope)===JSON.stringify(v.scope.frameScope));
      const items=all.slice(v.offset,v.offset+v.limit).map(t=>({target:clone(t)}));const end=v.offset+items.length;
      result={...base,kind:'targets',scope:clone(v.scope),offset:v.offset,total:all.length,items,nextOffset:end<all.length?end:null};
    }else{
      const found=targets.get(v.target.strokeId),item=f.owner.items.get(v.target.strokeId);
      if(!found||!item||JSON.stringify(found)!==JSON.stringify(v.target))return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:p.contentRevision,ok:false,error:{code:'not_found',message:'No native text'}};
      const textBytes=Buffer.byteLength(item.text);
      if(v.kind==='inspect'){const meta=clone(item);delete meta.text;result={...base,kind:'inspect',target:clone(found),textBytes,block:meta};}
      else{const bytes=Buffer.from(item.text);let end=Math.min(bytes.length,v.offset+v.limit);while(end>v.offset&&(bytes[end]&0xc0)===0x80)end--;
        result={...base,kind:'text',target:clone(found),textBytes,offset:v.offset,text:bytes.subarray(v.offset,end).toString(),nextOffset:end<bytes.length?end:null};}
    }
    return {apiVersion:2,requestId:q.requestId,instanceId:q.instanceId,documentId:q.documentId,contentRevision:p.contentRevision,ok:true,result};
  }});
  const session={...f.session,runResourceCommand:action=>f.session.runResourceCommand(async ctx=>{if(hold)await hold;return action({...ctx,dispatch:q=>sdk.dispatch(options.browserJsonBoundary?clone(q):q)});})};
  const context=wantPresented=>!f.state.active||wantPresented&&!presented?null:{pin:clone(session.info()),layerUid:layer,frame,selectionKey:selection,viewKey:view};
  const ports={session,reader:Reader,writer:Writer,context,defaultColor:()=> '#123456',draft:d=>drafts.push(d),committed:r=>committed.push(r),state:s=>states.push(s),cancelled:()=>{}};
  return {...f,session,ports,context,core:Tool.create(ports),reads,drafts,committed,states,targets,
    seed(t=target(),value=block()){targets.set(t.strokeId,clone(t));f.owner.items.set(t.strokeId,clone(value));},
    frame:v=>{frame=v;},layer:v=>{layer=v;},selection:v=>{selection=v;},view:v=>{view=v;},presented:v=>{presented=v;},hold:p=>{hold=p;}};
}
module.exports={fixture,block,target,deferred,clone};
