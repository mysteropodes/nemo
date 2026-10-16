'use strict';
const test=require('node:test');
// Actual Main session/File/transport and Root private SDK; native IPC is simulated.
const assert=require('node:assert/strict');
const Project=require(process.env.NEMO_POLICY_PROJECT||'../src/js/adapters/native-object-project.js');
const Files=require(process.env.NEMO_POLICY_FILES||'../src/js/adapters/native-object-project-file.js');
const Transport=require('../src/js/adapters/application-mcp.js'),Application=require('../src/js/adapters/native-application.js'),UI=require('../src/js/adapters/native-object-ui.js');
const turn=()=>new Promise(setImmediate);
function deferred(){let resolve;const promise=new Promise(r=>{resolve=r;});return {promise,resolve};}
class Element{
 constructor(doc,tag){this.ownerDocument=doc;this.tagName=tag.toUpperCase();this.children=[];this.parentNode=null;this.dataset={};this.attrs={};this.listeners=new Map();this._text='';this.value='';}
 set textContent(v){this.replaceChildren();this._text=String(v);}get textContent(){return this._text+this.children.map(x=>x.textContent).join('');}
 appendChild(n){if(n.parentNode)n.parentNode.removeChild(n);this.children.push(n);n.parentNode=this;return n;}
 removeChild(n){const i=this.children.indexOf(n);assert.notEqual(i,-1);this.children.splice(i,1);n.parentNode=null;return n;}
 replaceChildren(...nodes){for(const n of this.children)n.parentNode=null;this.children=[];this._text='';for(const n of nodes)this.appendChild(n);}
 setAttribute(k,v){this.attrs[k]=String(v);}addEventListener(k,fn){if(!this.listeners.has(k))this.listeners.set(k,new Set());this.listeners.get(k).add(fn);}
 removeEventListener(k,fn){this.listeners.get(k)?.delete(fn);}focus(){this.ownerDocument.activeElement=this;}
 async fire(type,extra={}){const event={type,target:this,defaultPrevented:false,preventDefault(){this.defaultPrevented=true;},...extra};for(const fn of [...this.listeners.get(type)||[]])await fn(event);return event;}
}
function policyDocument(){const nodes=new Map(),d={activeElement:null,createElement(tag){const n=new Element(d,tag);n.style={};return n;},getElementById:id=>nodes.get(id)||null};for(const id of ['layer-list','frame-hdr','frame-grid','drawing-canvas','btn-shy'])nodes.set(id,d.createElement('div'));const classes=new Set();nodes.get('btn-shy').classList={toggle(k,on){if(on===undefined)on=!classes.has(k);if(on)classes.add(k);else classes.delete(k);return on;},contains:k=>classes.has(k)};nodes.get('drawing-canvas').getBoundingClientRect=()=>({x:0,y:0,left:0,top:0,width:320,height:180});return d;}
function scenario(openFault){
  let active=false,revision=0,generation=0,visible,previewGate=null,replyGate=null,ackGate=null,failure=null;
  let nativePending=null,registered=null,policy={},uiPorts,transportGeneration=0,sdkCalls=0;
  const log=[],listeners=new Map(),oldListeners=[],commands=[],routes=[],consumers=[],diagnostics=[],openCalls=[];
  const openError={code:'unavailable',details:{disposition:'notDispatched'},message:'PRIVATE NATIVE ERROR'};
  let openStatus=0,openMetadata=0,openPreview=0;
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document-'+generation,lifecycleGeneration:generation,
    contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,documentDigest:String(revision%10).repeat(64),
    byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:21,undoDepth:revision,redoDepth:0,reference:null,
    layers:[{layerUid:'legacy layer:alpha',name:'Alpha',...structuredClone(policy)},{layerUid:'sibling',name:'Sibling'}]});
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));
    if(openFault)openCalls.push(command);
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status'){
      if(openFault&&active){openStatus++;if(openStatus===({'installed-status':1,'first-owner':2,'second-owner':3})[openFault.stage])return {...info(),available:true,instanceId:'wrong'};}
      return {...info(),available:active};
    }
    if(command==='nemo_native_object_bootstrap'){
      active=true;generation++;revision=0;visible=undefined;policy={};
      return {...info(),requestId:openFault?.stage==='receipt'?'wrong':r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);
      if(r.action==='binding'){
        if(openFault?.stage==='subscriber'&&openFault.invoke&&!openFault.register)throw openError;
        return {...info(),subscriptionId:openFault?.stage==='subscriber'&&!openFault.register?'INVALID ID':'subscription-'+generation};
      }
      if(r.action==='subscribe'){if(openFault?.register)throw openError;registered=r.binding;return r.binding;}
      if(r.action==='acknowledge'){
        assert.equal(r.event.toRevision,revision);assert.equal(root.state.layers[0].visible,true);for(const flag of ['locked','shy'])assert.equal(root.state.layers[0][flag],policy[flag]===true);
        if(ackGate){const gate=ackGate;ackGate=null;await gate.promise;}
        nativePending=null;
        return null;
      }
      if(r.action==='disconnect'){if(openFault?.register)throw {code:'cancelled',message:'PRIVATE CLEANUP'};return null;}
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(openFault&&++openMetadata===1&&openFault.stage==='metadata')return {...info(),requestId:'wrong'};
      log.push('inspect-'+revision);
      const next={...info(),requestId:r.requestId};if(failure==='metadata')next.layers[0].locked=null;
      return next;
    }
    if(command==='nemo_native_object_preview'){
      if(openFault&&++openPreview===({'first-preview':1,'second-preview':2})[openFault.stage]){
        if(openFault.invoke)throw openError;
        return {status:'refused'};
      }
      if(nativePending){
        if(!r.revisionSync||r.revisionSync.subscriptionId!==registered.subscriptionId||
          JSON.stringify(r.revisionSync.event)!==JSON.stringify(nativePending)||r.contentRevision!==nativePending.toRevision)
          throw Error('Native revision synchronization is pending; untagged preview refused');
      }
      if(previewGate){const gate=previewGate;previewGate=null;await gate.promise;}
      log.push('present-'+r.contentRevision);
      return {status:failure==='preview'?'refused':'presented',workId:'work-'+r.contentRevision,viewGeneration:1};
    }
    if(command==='nemo_native_dispatch'&&r.operation==='query.document.object.layer.timing')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:false,error:{code:'unavailable',message:'Timing is outside this policy fixture'}};
    assert.equal(command,'nemo_native_dispatch');routes.push(command);assert.equal(r.expectedRevision,revision);
    assert.equal(r.documentId,info().documentId);commands.push(r);
    assert.equal(r.operation,'command.document.object.layer.policy.edit');assert.equal(r.payload.command,'object.layer.policy.edit');assert.ok(['locked','shy'].includes(r.payload.flag));
    const applied=(policy[r.payload.flag]===true)!==r.payload.value;
    if(applied){policy[r.payload.flag]=r.payload.value;revision++;}
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(replyGate){const gate=replyGate;replyGate=null;await gate.promise;}
    if(failure==='malformed-reply')response.result.historyEntriesAdded=2;
    return response;
  }
  const root={state:{},selectedPaths:[],userLayers:[],NemoNativeObjectProjectFile:Files,
    NemoNativeApplicationAdapter:{createNativeApplicationAdapter(...args){const client=Application.createNativeApplicationAdapter(...args);return {...client,editLayerPolicy(request){sdkCalls++;return client.editLayerPolicy(request);}};}},NemoApplicationMcpTransport:{createNativeTauriTransport:invoke=>{
      transportGeneration++;const transport=Transport.createNativeTauriTransport(invoke);
      return {...transport,subscribeRevisions:(listen,synchronize,disconnected)=>{
        consumers.push(synchronize);return transport.subscribeRevisions(listen,synchronize,disconnected);
      }};
    }},__TAURI__:{core:{invoke},event:{listen:async(name,callback)=>{
      listeners.set(name,callback);oldListeners.push(callback);return ()=>{if(listeners.get(name)===callback)listeners.delete(name);};
    }}},document:policyDocument(),
    updateUI:()=>{if(openFault?.stage==='publication')throw openError;log.push('publish-'+revision);if(uiPorts)UI.render(root,session.info(),false,uiPorts);}};
  if(openFault)root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>diagnostics.push([event,code]);
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);
  const owner={async edit(payload,pin){const method=session.editLayerPolicy;if(typeof method!=='function')return false;const receipt=await method(payload,pin);return receipt&&receipt.ok===true;}};
  let shyMode=false;root.state.appMode='drawing';root.SM={toggleLayerLock(){throw Error('No legacy lock writer');},toggleLayerShy(){throw Error('No legacy shy writer');},toggleShyMode(){throw Error('No legacy global writer');}};
  uiPorts={currentSession:()=>session,projectionReady:info=>Files.projectionReadyForUI(root,info),selectLayer:()=>true,repaint:()=>true,layerPolicy:()=>typeof session.editLayerPolicy==='function'?owner:null,readTransportGeneration:()=>transportGeneration,readShyMode:()=>shyMode,setShyMode(value){shyMode=value;},clearDrawingSelection(){root.selectedPaths=[];}};

  async function open(){return session.openPath('/native.json',{
    ...(openFault?{reveal:()=>{if(openFault.stage==='reveal')throw openError;}}:{}),
    commit:meta=>{if(openFault?.stage==='ui-commit')throw openError;Files.projectView(root,meta);}});}
  function event(from,to){return {instanceId:'instance',documentId:info().documentId,lifecycleGeneration:generation,
    requestId:'external-'+to,fromRevision:from,toRevision:to};}
  async function external(flag,value){const from=revision;policy[flag]=value;revision++;nativePending=event(from,revision);
    return listeners.get('nemo-native-revision')({payload:nativePending});}
  return {root,session,uiPorts,owner,UI,Files,get sdkCalls(){return sdkCalls;},get shyMode(){return shyMode;},render:()=>UI.render(root,session.info(),false,uiPorts),row:()=>root.document.getElementById('layer-list').children[0],log,commands,routes,open,external,listeners,oldListeners,consumers,diagnostics,openCalls,openError,
    holdPreview:gate=>{previewGate=gate;},holdReply:gate=>{replyGate=gate;},holdAck:gate=>{ackGate=gate;},fail:value=>{failure=value;},
    current:info};
}



const payload=(flag,value)=>({command:'object.layer.policy.edit',layerUid:'legacy layer:alpha',flag,value});
async function open(){const h=scenario();await h.open();h.render();return h;}
test('actual private SDK and existing native session queue commit a lock into current file projection and row UI',async()=>{const h=await open(),pin=h.session.info();assert.equal(typeof h.session.editLayerPolicy,'function');const response=await h.session.editLayerPolicy(payload('locked',true),pin);assert.equal(response.ok,true);assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.root.state.layers[0].locked,true);h.render();assert.equal(h.row().children[2].textContent,'Unlock');assert.ok(h.log.includes('inspect-1'));assert.ok(h.log.includes('present-1'));assert.deepEqual(h.routes,['nemo_native_dispatch']);});
test('accepted UI port composes actual SDK/session ACK and enables shy only after final presented refresh',async()=>{const h=await open();assert.equal(await h.UI.setLayerPolicy(h.root,'legacy layer:alpha','shy',true,h.session.info(),h.uiPorts),true);assert.equal(h.session.info().layers[0].shy,true);assert.equal(h.root.state.layers[0].shy,true);assert.equal(h.shyMode,true);assert.equal(h.row().hidden,true);assert.equal(h.row().style.display,'none');});
test('native caller false no-op preserves absent metadata and creates no new revision or history',async()=>{const h=await open(),pin=h.session.info();const receipt=await h.session.editLayerPolicy(payload('locked',false),pin);assert.equal(receipt.result.applied,false);assert.equal(receipt.contentRevision,0);assert.equal(Object.hasOwn(h.session.info().layers[0],'locked'),false);assert.equal(h.root.state.layers[0].locked,false);assert.equal(h.session.info().undoDepth,0);});
test('actual external ACK barrier makes policy unavailable until the real transport finishes acknowledgement',async()=>{const h=await open(),gate=deferred();h.holdAck(gate);const external=h.external('locked',true);for(let i=0;i<30&&!h.log.includes('acknowledge');i++)await turn();assert.ok(h.log.includes('acknowledge'));assert.equal(typeof h.session.editLayerPolicy,'undefined');assert.equal(h.session.readPresentedObjectSession(),null);assert.equal(await h.UI.setLayerPolicy(h.root,'legacy layer:alpha','shy',true,h.session.info(),h.uiPorts),false);assert.equal(h.commands.length,0);assert.equal(h.shyMode,false);gate.resolve();await external;h.render();assert.equal(typeof h.session.editLayerPolicy,'function');assert.equal(await h.UI.setLayerPolicy(h.root,'legacy layer:alpha','shy',true,h.session.info(),h.uiPorts),true);assert.equal(h.shyMode,true);});
test('external revision subscription updates authored policy flags through existing metadata and projection',async()=>{const h=await open();await h.external('locked',true);assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.root.state.layers[0].locked,true);assert.ok(h.session.readPresentedObjectSession());assert.equal(h.commands.length,0);h.render();assert.equal(h.row().children[2].textContent,'Unlock');});
test('flag absence on an Undo-like revision clears stale detached UI flags without authoring native false',async()=>{const h=await open();await h.external('locked',true);const next={...h.session.info(),contentRevision:2,documentSnapshotId:'undo-like-2',layers:h.session.info().layers.map(l=>{const next={...l};delete next.locked;return next;})};assert.equal(h.Files.refreshLayerTable(h.root,next),true);assert.equal(h.root.state.layers[0].locked,false);assert.equal(Object.hasOwn(next.layers[0],'locked'),false);assert.equal(h.Files.projectionReadyForUI(h.root,next),true);});
test('malformed native policy metadata fences the session rather than becoming effective unlocked',async()=>{const h=await open();h.fail('metadata');await assert.rejects(h.session.editLayerPolicy(payload('locked',true),h.session.info()),/uncorrelated/);assert.equal(h.session.isIndeterminate(),true);assert.equal(h.session.readPresentedObjectSession(),null);});
test('stale caller pin cannot enter a second policy command after an external committed revision',async()=>{const h=await open(),old=h.session.info();await h.external('locked',true);await assert.rejects(h.session.editLayerPolicy(payload('shy',true),old),/stale/);assert.equal(h.commands.length,0);});
test('projection readiness detects stale effective flags even when native field is absent',async()=>{const h=await open();assert.equal(h.Files.projectionReadyForUI(h.root,h.session.info()),true);h.root.state.layers[0].locked=true;assert.equal(h.Files.projectionReadyForUI(h.root,h.session.info()),false);});

test('ordinary owner calls the explicit real SDK policy method once, not an undeclared generic fallback',async()=>{const h=await open();await h.session.editLayerPolicy(payload('locked',true),h.session.info());assert.equal(h.sdkCalls,1);assert.equal(h.commands.length,1);});
test('missing explicit SDK policy method refuses without sending a native mutation',async()=>{const h=await open(),factory=h.root.NemoNativeApplicationAdapter.createNativeApplicationAdapter;h.root.NemoNativeApplicationAdapter={createNativeApplicationAdapter(...args){const client=factory(...args);const {editLayerPolicy,...without}=client;return without;}};assert.equal(typeof h.session.editLayerPolicy,'undefined');assert.equal(h.commands.length,0);assert.equal(h.session.info().contentRevision,0);});
test('same-revision changed policy cannot silently update detached rows at a fixed snapshot',async()=>{const h=await open(),info=h.session.info(),before=h.root.state.layers[0].locked;const invalid={...info,layers:info.layers.map((l,i)=>i?l:{...l,locked:true})};assert.equal(h.Files.refreshLayerTable(h.root,invalid),false);assert.equal(h.root.state.layers[0].locked,before);});
test('known true policy flags survive native read and readonly projection with authored presence intact',async()=>{const h=await open();await h.external('locked',true);await h.external('shy',true);assert.equal(Object.isFrozen(h.session.info().layers[0]),true);assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.session.info().layers[0].shy,true);assert.equal(h.root.state.layers[0].locked,true);assert.equal(h.root.state.layers[0].shy,true);assert.throws(()=>{h.session.info().layers[0].locked=false;},TypeError);});
test('native policy intent rejects unsupported fields and flags before SDK/native transport',async()=>{const h=await open();for(const value of [{...payload('locked',true),holder:{}},{...payload('solo',true)},{...payload('locked',true),value:1},{...payload('locked',true),layerUid:'foreign'}]){await assert.rejects(h.session.editLayerPolicy(value,h.session.info()),/stale or invalid/);}assert.equal(h.sdkCalls,0);assert.equal(h.commands.length,0);});

test('extracted metadata contract rejects every nonboolean authored policy flag before UI projection',async()=>{const h=await open(),Metadata=require(process.env.NEMO_POLICY_METADATA||'../src/js/adapters/native-object-project-metadata.js');const request={requestId:'policy-read',instanceId:'instance',documentId:h.current().documentId};for(const flag of ['locked','shy'])for(const value of [null,0,'true',[],{}]){const raw={...h.current(),requestId:request.requestId,layers:[{layerUid:'layer',[flag]:value}]};assert.throws(()=>Metadata.decode(raw,request),/uncorrelated/);}});
test('extracted metadata preserves authored false versus absence while copying no document payload',async()=>{const h=await open(),Metadata=require('../src/js/adapters/native-object-project-metadata.js'),request={requestId:'policy-read',instanceId:'instance',documentId:h.current().documentId},raw={...h.current(),requestId:request.requestId,documentJson:'PRIVATE FULL DOCUMENT',layers:[{layerUid:'explicit',locked:false,shy:false},{layerUid:'absent'}]};const read=Metadata.decode(raw,request);assert.deepEqual(read.layers,[{layerUid:'explicit',locked:false,shy:false},{layerUid:'absent'}]);assert.equal('documentJson'in read,false);assert.equal(Object.isFrozen(read.layers[0]),true);});

test('ordinary rendered shy button reaches real SDK/session path and returns a confirmed hidden row',async()=>{const h=await open();await h.row().children[3].fire('click');assert.equal(h.sdkCalls,1);assert.equal(h.session.info().layers[0].shy,true);assert.equal(h.row().hidden,true);assert.equal(h.shyMode,true);});
test('ordinary SM lock entry reaches the actual SDK/session while legacy writer stays unreachable',async()=>{const h=await open(),binding=h.UI.bindLayerPolicy(h.root,h.uiPorts);assert.equal(await h.root.SM.toggleLayerLock(0),true);assert.equal(h.sdkCalls,1);assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.row().children[2].textContent,'Unlock');binding.dispose();});
test('actual native reply latency cannot optimistically change projected policy or row controls',async()=>{const h=await open(),gate=deferred(),before=h.session.info();h.holdReply(gate);const pending=h.UI.setLayerPolicy(h.root,'legacy layer:alpha','locked',true,before,h.uiPorts);for(let i=0;i<30&&!h.commands.length;i++)await turn();assert.equal(h.commands.length,1);assert.equal(h.session.info(),before);assert.equal(h.root.state.layers[0].locked,false);assert.equal(h.row().children[2].textContent,'Lock');gate.resolve();assert.equal(await pending,true);assert.equal(h.root.state.layers[0].locked,true);});

test('retained visibility projection cannot change native policy at the same revision and snapshot',async()=>{const h=await open(),info=h.session.info(),before=JSON.stringify(h.root.state.layers),wrong={...info,layers:info.layers.map((l,i)=>i?l:{...l,locked:true})};assert.equal(h.Files.refreshVisibility(h.root,wrong),false);assert.equal(JSON.stringify(h.root.state.layers),before);assert.equal(h.Files.projectionReadyForUI(h.root,info),true);});
test('retained visibility projection cannot roll back policy from a stale native revision',async()=>{const h=await open(),old=h.session.info();await h.external('locked',true);const before=JSON.stringify(h.root.state.layers);assert.equal(h.Files.refreshVisibility(h.root,old),false);assert.equal(JSON.stringify(h.root.state.layers),before);assert.equal(h.root.state.layers[0].locked,true);});
test('retained visibility projection refuses a changed snapshot at an unchanged revision',async()=>{const h=await open(),info=h.session.info(),before=JSON.stringify(h.root.state.layers);assert.equal(h.Files.refreshVisibility(h.root,{...info,documentSnapshotId:'foreign-snapshot'}),false);assert.equal(JSON.stringify(h.root.state.layers),before);});

async function openAvailability(){const h=scenario();h.root.NemoNativeObjectUI=h.UI;await h.open();h.render();return h;}
async function drain(){await new Promise(r=>setTimeout(r,0));await turn();}
test('real external ACK completion restores policy buttons without a manual render or a new mutation',async()=>{const h=await openAvailability(),gate=deferred();h.holdAck(gate);const pending=h.external('locked',true);for(let i=0;i<30&&!h.log.includes('acknowledge');i++)await turn();assert.ok(h.log.includes('acknowledge'));assert.equal(h.row().children[2].disabled,true);const button=h.row().children[2],before=h.commands.length;gate.resolve();await pending;await drain();assert.equal(h.row().children[2],button);assert.equal(button.disabled,false);assert.equal(h.row().children[3].disabled,false);assert.equal(h.commands.length,before);});

test('post-ACK availability cache preserves control identity and focus on the next frame-only paint',async()=>{const h=await openAvailability();await h.external('locked',true);await drain();const button=h.row().children[2];button.focus();assert.equal(button.disabled,false);h.UI.render(h.root,h.session.info(),true,h.uiPorts);assert.equal(h.row().children[2]===button,true,'frame-only paint must retain the policy control');assert.equal(h.root.document.activeElement===button,true,'focus must remain on the retained control');});
test('ordinary in-flight edit disables policy controls until the latest ticket finishes',async()=>{const h=await openAvailability(),gate=deferred();h.holdReply(gate);const pending=h.UI.setLayerPolicy(h.root,'legacy layer:alpha','locked',true,h.session.info(),h.uiPorts);assert.equal(h.row().children[2].disabled,true);assert.equal(h.row().children[3].disabled,true);for(let i=0;i<30&&!h.commands.length;i++)await turn();assert.equal(h.commands.length,1);assert.equal(h.root.state.layers[0].locked,false);gate.resolve();assert.equal(await pending,true);assert.equal(h.row().children[2].disabled,false);assert.equal(h.row().children[3].disabled,false);assert.equal(h.row().children[2].textContent,'Unlock');});
test('late external ACK callback cannot revive controls after presenter retirement',async()=>{const h=await openAvailability(),gate=deferred();h.holdAck(gate);const pending=h.external('locked',true);for(let i=0;i<30&&!h.log.includes('acknowledge');i++)await turn();const button=h.row().children[2];assert.equal(button.disabled,true);h.UI.retirePolicy(h.root);gate.resolve();await pending;await drain();assert.equal(h.UI.refreshPolicy(h.root),false);assert.equal(button.disabled,true);assert.equal(h.row().children[2],button);});
test('current-frame mismatch keeps controls disabled until actual presentation matches again',async()=>{const h=await openAvailability(),button=h.row().children[2];h.root.state.currentFrame=1;assert.equal(h.UI.refreshPolicy(h.root),false);assert.equal(button.disabled,true);h.root.state.currentFrame=0;assert.equal(h.UI.refreshPolicy(h.root),true);assert.equal(button.disabled,false);assert.equal(h.commands.length,0);});
test('optional policy-provider removal and restoration update availability without rebuilding native rows',async()=>{const h=await openAvailability(),port=h.uiPorts.layerPolicy,button=h.row().children[2];h.uiPorts.layerPolicy=()=>null;assert.equal(h.UI.refreshPolicy(h.root),false);assert.equal(button.disabled,true);h.uiPorts.layerPolicy=port;assert.equal(h.UI.refreshPolicy(h.root),true);assert.equal(button.disabled,false);assert.equal(h.row().children[2],button);assert.equal(h.commands.length,0);});

function scenarioDesktop(openFault){
  let active=false,revision=0,generation=0,visible,previewGate=null,replyGate=null,ackGate=null,failure=null;
  let nativePending=null,registered=null,policy=new Map(),uiPorts,transportGeneration=0,sdkCalls=0;
  const log=[],listeners=new Map(),oldListeners=[],commands=[],routes=[],consumers=[],diagnostics=[],openCalls=[];
  const openError={code:'unavailable',details:{disposition:'notDispatched'},message:'PRIVATE NATIVE ERROR'};
  let openStatus=0,openMetadata=0,openPreview=0;
  const info=()=>({apiVersion:2,instanceId:'instance',documentId:'document-'+generation,lifecycleGeneration:generation,
    contentRevision:revision,documentSnapshotId:'snapshot-'+generation+'-'+revision,documentDigest:String(revision%10).repeat(64),
    byteCount:1000,canvasW:320,canvasH:180,fps:24,totalFrames:21,undoDepth:revision,redoDepth:0,reference:null,
    layers:[{layerUid:'legacy layer:alpha',name:'Alpha',...structuredClone(policy.get('legacy layer:alpha')||{})},{layerUid:'sibling',name:'Sibling',...structuredClone(policy.get('sibling')||{})}]});
  async function invoke(command,args){
    const r=args&&(args.request||JSON.parse(args.requestJson));
    if(openFault)openCalls.push(command);
    if(command==='nemo_native_status'||command==='nemo_native_object_client_status'){
      if(openFault&&active){openStatus++;if(openStatus===({'installed-status':1,'first-owner':2,'second-owner':3})[openFault.stage])return {...info(),available:true,instanceId:'wrong'};}
      return {...info(),available:active};
    }
    if(command==='nemo_native_object_bootstrap'){
      active=true;generation++;revision=0;visible=undefined;policy=new Map();
      return {...info(),requestId:openFault?.stage==='receipt'?'wrong':r.requestId,resourceCount:0,viewportAvailable:false};
    }
    if(command==='nemo_native_revision_sync'){
      log.push(r.action);
      if(r.action==='binding'){
        if(openFault?.stage==='subscriber'&&openFault.invoke&&!openFault.register)throw openError;
        return {...info(),subscriptionId:openFault?.stage==='subscriber'&&!openFault.register?'INVALID ID':'subscription-'+generation};
      }
      if(r.action==='subscribe'){if(openFault?.register)throw openError;registered=r.binding;return r.binding;}
      if(r.action==='acknowledge'){
        assert.equal(r.event.toRevision,revision);assert.equal(root.state.layers[0].visible,true);for(const row of root.state.layers)for(const flag of ['locked','shy'])assert.equal(row[flag],(policy.get(row.layerUid)||{})[flag]===true);
        if(ackGate){const gate=ackGate;ackGate=null;await gate.promise;}
        nativePending=null;
        return null;
      }
      if(r.action==='disconnect'){if(openFault?.register)throw {code:'cancelled',message:'PRIVATE CLEANUP'};return null;}
    }
    if(command==='nemo_native_object_reference'&&r.action.kind==='inspect'){
      if(openFault&&++openMetadata===1&&openFault.stage==='metadata')return {...info(),requestId:'wrong'};
      log.push('inspect-'+revision);
      const next={...info(),requestId:r.requestId};if(failure==='metadata')next.layers[0].locked=null;
      return next;
    }
    if(command==='nemo_native_object_preview'){
      if(openFault&&++openPreview===({'first-preview':1,'second-preview':2})[openFault.stage]){
        if(openFault.invoke)throw openError;
        return {status:'refused'};
      }
      if(nativePending){
        if(!r.revisionSync||r.revisionSync.subscriptionId!==registered.subscriptionId||
          JSON.stringify(r.revisionSync.event)!==JSON.stringify(nativePending)||r.contentRevision!==nativePending.toRevision)
          throw Error('Native revision synchronization is pending; untagged preview refused');
      }
      if(previewGate){const gate=previewGate;previewGate=null;await gate.promise;}
      log.push('present-'+r.contentRevision);
      return {status:failure==='preview'?'refused':'presented',workId:'work-'+r.contentRevision,viewGeneration:1};
    }
    if(command==='nemo_native_dispatch'&&r.operation==='query.document.object.layer.timing')return {apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,contentRevision:revision,ok:false,error:{code:'unavailable',message:'Timing is outside this policy fixture'}};
    assert.equal(command,'nemo_native_dispatch');routes.push(command);assert.equal(r.expectedRevision,revision);
    assert.equal(r.documentId,info().documentId);commands.push(r);
    assert.equal(r.operation,'command.document.object.layer.policy.edit');assert.equal(r.payload.command,'object.layer.policy.edit');assert.ok(['locked','shy'].includes(r.payload.flag));
    const before=policy.get(r.payload.layerUid)||{},applied=(before[r.payload.flag]===true)!==r.payload.value;
    if(applied){policy.set(r.payload.layerUid,{...before,[r.payload.flag]:r.payload.value});revision++;}
    const response={apiVersion:2,requestId:r.requestId,instanceId:r.instanceId,documentId:r.documentId,
      contentRevision:revision,ok:true,result:{applied,historyEntriesAdded:applied?1:0}};
    if(replyGate){const gate=replyGate;replyGate=null;await gate.promise;}
    if(failure==='malformed-reply')response.result.historyEntriesAdded=2;
    return response;
  }
  const root={state:{},selectedPaths:[],userLayers:[],NemoNativeObjectProjectFile:Files,
    NemoNativeApplicationAdapter:{createNativeApplicationAdapter(...args){const client=Application.createNativeApplicationAdapter(...args);return {...client,editLayerPolicy(request){sdkCalls++;return client.editLayerPolicy(request);}};}},NemoApplicationMcpTransport:{createNativeTauriTransport:invoke=>{
      transportGeneration++;const transport=Transport.createNativeTauriTransport(invoke);
      return {...transport,subscribeRevisions:(listen,synchronize,disconnected)=>{
        consumers.push(synchronize);return transport.subscribeRevisions(listen,synchronize,disconnected);
      }};
    }},__TAURI__:{core:{invoke},event:{listen:async(name,callback)=>{
      listeners.set(name,callback);oldListeners.push(callback);return ()=>{if(listeners.get(name)===callback)listeners.delete(name);};
    }}},document:policyDocument(),
    updateUI:()=>{if(openFault?.stage==='publication')throw openError;log.push('publish-'+revision);Project.paintUI(root,false);}};
  if(openFault)root.__NEMO_P38_FIXTURE_OPEN_DIAGNOSTIC__=(event,code)=>diagnostics.push([event,code]);
  require('./helpers/native-viewport-underlay.cjs').install(root);
  const session=Project.desktop(root);
  root.state.appMode='drawing';root.SM={toggleLayerLock(){throw Error('Existing native guard: no legacy lock write');},toggleLayerShy(){throw Error('Existing native guard: no legacy shy write');},toggleShyMode(){throw Error('Existing native guard: no legacy global write');}};
  root.NemoNativeObjectUI={...UI,render(...args){uiPorts=args[3];return UI.render(...args);}};
  async function open(){return session.openPath('/native.json',{
    ...(openFault?{reveal:()=>{if(openFault.stage==='reveal')throw openError;}}:{}),
    commit:meta=>{if(openFault?.stage==='ui-commit')throw openError;Files.projectView(root,meta);}});}
  function event(from,to){return {instanceId:'instance',documentId:info().documentId,lifecycleGeneration:generation,
    requestId:'external-'+to,fromRevision:from,toRevision:to};}
  async function external(flag,value){const from=revision;policy.set('legacy layer:alpha',{...policy.get('legacy layer:alpha'),[flag]:value});revision++;nativePending=event(from,revision);
    return listeners.get('nemo-native-revision')({payload:nativePending});}
  return {root,session,get uiPorts(){return uiPorts;},UI,Files,get sdkCalls(){return sdkCalls;},get shyMode(){return root.state.shyEnabled===true;},render:()=>Project.paintUI(root,false),row:()=>root.document.getElementById('layer-list').children[0],log,commands,routes,open,external,listeners,oldListeners,consumers,diagnostics,openCalls,openError,
    holdPreview:gate=>{previewGate=gate;},holdReply:gate=>{replyGate=gate;},holdAck:gate=>{ackGate=gate;},fail:value=>{failure=value;},
    current:info};
}



async function openDesktop(){const h=scenarioDesktop();await h.open();return h;}
async function drainDesktop(){await new Promise(r=>setTimeout(r,0));await turn();}
test('actual Project paint composes all policy ports and enables initial native rows without a manual render',async()=>{const h=await openDesktop();assert.ok(Object.isFrozen(h.uiPorts));assert.equal(typeof h.uiPorts.layerPolicy,'function');assert.equal(h.uiPorts.readTransportGeneration(),h.session.readSubscriptionGeneration());assert.equal(h.row().children[2].disabled,false);assert.equal(h.row().children[3].disabled,false);assert.equal(h.commands.length,0);});

test('ordinary native lock button uses real composition and clears only confirmed active Drawing selection',async()=>{const h=await openDesktop(),selection=[{id:'active-ui-selection'}];h.root.selectedPaths=selection;h.root.state.selectedStrokeIndices=[3];await h.row().children[2].fire('click');assert.equal(h.sdkCalls,1);assert.equal(h.session.info().layers[0].locked,true);assert.deepEqual(h.root.selectedPaths,[]);assert.deepEqual(h.root.state.selectedStrokeIndices,[]);assert.equal(h.row().children[2].disabled,false);});
test('locking a sibling layer preserves active Drawing selection and layer-selection identity',async()=>{const h=await openDesktop(),paths=[{id:'active-ui-selection'}],indices=[2],layers=h.root._layerSel;h.root.selectedPaths=paths;h.root.state.selectedStrokeIndices=indices;const row=h.root.document.getElementById('layer-list').children[1];await row.children[2].fire('click');assert.equal(h.session.info().layers[1].locked,true);assert.equal(h.root.selectedPaths,paths);assert.equal(h.root.state.selectedStrokeIndices,indices);assert.equal(h.root._layerSel,layers);});
test('Motion lock through actual ports never clears Drawing selection or applies a generic edit ban',async()=>{const h=await openDesktop(),paths=[{id:'motion-selection'}];h.root.state.appMode='motion';h.root.selectedPaths=paths;h.root.state.selectedStrokeIndices=[4];await h.row().children[2].fire('click');assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.root.selectedPaths,paths);assert.deepEqual(h.root.state.selectedStrokeIndices,[4]);});
test('actual ordinary SM entry is installed by Project paint and uses the native SDK owner',async()=>{const h=await openDesktop();assert.equal(await h.root.SM.toggleLayerLock(0),true);assert.equal(h.sdkCalls,1);assert.equal(h.commands[0].payload.command,'object.layer.policy.edit');assert.equal(h.session.info().layers[0].locked,true);});
test('confirmed shy updates existing UI mode and actual toolbar active class without changing native visibility',async()=>{const h=await openDesktop();await h.row().children[3].fire('click');assert.equal(h.session.info().layers[0].shy,true);assert.equal(h.root.state.shyEnabled,true);assert.equal(h.root.document.getElementById('btn-shy').classList.contains('active'),true);assert.equal(h.row().hidden,true);assert.equal(h.session.info().layers[0].visible,undefined);});
test('the real baseline shy toolbar listener reaches the composed ordinary UI mode port with no native write',async()=>{const h=await openDesktop();await h.row().children[3].fire('click');const before=h.session.info(),commands=h.commands.length,fs=require('node:fs'),vm=require('node:vm'),source=fs.readFileSync(__dirname+'/../src/js/timeline.js','utf8');const start=source.indexOf('(function bindShy(){'),end=source.indexOf('  })();',start);assert.ok(start>=0&&end>start);vm.runInNewContext(source.slice(start,end+7),{window:h.root,state:h.root.state,document:h.root.document});await h.root.document.getElementById('btn-shy').fire('click');assert.equal(h.root.state.shyEnabled,false);assert.equal(h.root.document.getElementById('btn-shy').classList.contains('active'),false);assert.equal(h.row().hidden,false);assert.equal(h.session.info(),before);assert.equal(h.commands.length,commands);});
test('external ACK re-enables actual composed controls automatically without manually rendering',async()=>{const h=await openDesktop(),gate=deferred();h.holdAck(gate);const pending=h.external('locked',true);for(let i=0;i<30&&!h.log.includes('acknowledge');i++)await turn();const button=h.row().children[2];assert.equal(button.disabled,true);assert.equal(h.uiPorts.layerPolicy()===null,true);gate.resolve();await pending;await drainDesktop();assert.equal(h.row().children[2]===button,true);assert.equal(button.disabled,false);assert.ok(h.uiPorts.layerPolicy());});
test('pending actual native reply leaves both UI selection and shy mode unchanged until confirmation',async()=>{const h=await openDesktop(),gate=deferred(),before=h.session.info();h.holdReply(gate);const action=h.row().children[3].fire('click');for(let i=0;i<30&&!h.commands.length;i++)await turn();assert.equal(h.session.info(),before);assert.equal(h.root.state.shyEnabled,undefined);assert.equal(h.row().hidden,false);assert.equal(h.row().children[3].disabled,true);gate.resolve();await action;assert.equal(h.root.state.shyEnabled,true);assert.equal(h.row().children[3].disabled,false);});
test('actual owner subscription counter changes on reconnect and is never substituted with document revision',async()=>{const h=await openDesktop(),generation=h.uiPorts.readTransportGeneration(),before=h.session.info().contentRevision;await h.external('locked',true);assert.equal(h.uiPorts.readTransportGeneration(),generation);assert.equal(h.session.info().contentRevision,before+1);await h.open();assert.ok(h.uiPorts.readTransportGeneration()>generation);assert.equal(h.session.info().contentRevision,0);});
test('stale expected pin cannot mutate transient shy mode or clear current native selection',async()=>{const h=await openDesktop(),old=h.session.info();await h.external('locked',true);await drainDesktop();const paths=[{id:'current'}];h.root.selectedPaths=paths;assert.equal(h.uiPorts.setShyMode(true,old),false);assert.equal(h.uiPorts.clearDrawingSelection('legacy layer:alpha',old),false);assert.equal(h.root.selectedPaths,paths);assert.equal(h.root.state.shyEnabled,undefined);});

test('missing actual composition helper leaves native policy controls unavailable with no substitute ports',async()=>{const h=scenarioDesktop();delete h.root.NemoNativeObjectUI.createPolicyPorts;await h.open();assert.equal(h.uiPorts.layerPolicy,undefined);assert.equal(h.row().children[2].disabled,true);await h.row().children[2].fire('click');assert.equal(h.sdkCalls,0);assert.equal(h.commands.length,0);});
test('actual cached Project UI ports and policy owner stay stable across normal paints',async()=>{const h=await openDesktop(),ports=h.uiPorts,owner=ports.layerPolicy();h.render();assert.equal(h.uiPorts,ports);assert.equal(h.uiPorts.layerPolicy(),owner);assert.ok(Object.isFrozen(owner));});
test('actual native binding does not replace an existing foreign handler when its lifecycle is disposed',async()=>{const h=await openDesktop(),foreign=()=>true;h.root.SM.toggleLayerShy=foreign;h.uiPorts.disposePolicyEntries();assert.equal(h.root.SM.toggleLayerShy,foreign);assert.throws(()=>h.root.SM.toggleLayerLock(0),/no legacy lock write/);});
test('current-frame mismatch makes real composed policy provider unavailable and prevents mutation',async()=>{const h=await openDesktop();h.root.state.currentFrame=1;assert.equal(h.uiPorts.layerPolicy()===null,true);h.UI.refreshPolicy(h.root);assert.equal(h.row().children[2].disabled,true);await h.row().children[2].fire('click');assert.equal(h.commands.length,0);});
test('real stale selection change while SDK reply waits cannot clear the new UI selection',async()=>{const h=await openDesktop(),gate=deferred();h.root.selectedPaths=[{id:'before'}];h.holdReply(gate);const pending=h.row().children[2].fire('click');for(let i=0;i<30&&!h.commands.length;i++)await turn();const fresh=[{id:'new-selection'}];h.root._layerSel=[1];h.root.selectedPaths=fresh;gate.resolve();await pending;assert.equal(h.session.info().layers[0].locked,true);assert.equal(h.root.selectedPaths,fresh);});
test('missing public SDK refuses through actual owner without any legacy or native mutation',async()=>{const h=await openDesktop(),before=h.session.info();delete h.root.NemoNativeApplicationAdapter;h.UI.refreshPolicy(h.root);assert.equal(h.uiPorts.layerPolicy()===null,true);assert.equal(h.row().children[2].disabled,true);await h.row().children[2].fire('click');assert.equal(h.commands.length,0);assert.equal(h.session.info().contentRevision,before.contentRevision);assert.equal(h.root.state.layers[0].locked,false);});
test('real composition preserves native absence when asked for effective-false UI no-op',async()=>{const h=await openDesktop();assert.equal(await h.UI.setLayerPolicy(h.root,'legacy layer:alpha','locked',false,h.session.info(),h.uiPorts),true);assert.equal(h.commands.length,0);assert.equal(Object.hasOwn(h.session.info().layers[0],'locked'),false);assert.equal(h.root.state.layers[0].locked,false);});

test('actual native paint installs ordinary policy wrappers once and disposes them through the real UI lifecycle',async()=>{const h=scenarioDesktop(),original=h.root.SM.toggleLayerLock;await h.open();const installed=h.root.SM.toggleLayerLock;assert.equal(installed===original,false);h.render();assert.equal(h.root.SM.toggleLayerLock===installed,true);delete h.root.__TAURI__;assert.equal(h.UI.refreshPolicy(h.root),false);const Project=require('../src/js/adapters/native-object-project.js');assert.equal(Project.paintUI(h.root,false),false);assert.equal(h.root.SM.toggleLayerLock===original,true);assert.equal(installed(0),false);assert.equal(h.commands.length,0);});
test('composition has no Paper selection lookup and leaves native layer fields exclusively owner-controlled',async()=>{const h=await openDesktop(),info=h.session.info();Object.defineProperty(h.root,'userLayers',{get(){throw Error('No Paper selection authority');},configurable:true});h.root.selectedPaths=[{id:'ui-only'}];h.root.state.selectedStrokeIndices=[2];assert.equal(h.uiPorts.clearDrawingSelection('legacy layer:alpha',info),false);assert.equal(h.root.selectedPaths.length,1);assert.equal(Object.hasOwn(info.layers[0],'locked'),false);});

test('actual frame-only Project paint preserves policy control identity and focus without provider-driven rebuilds',async()=>{const h=await openDesktop(),button=h.row().children[2],Project=require('../src/js/adapters/native-object-project.js');button.focus();Project.paintUI(h.root,true);assert.equal(h.row().children[2]===button,true);assert.equal(h.root.document.activeElement===button,true);});

