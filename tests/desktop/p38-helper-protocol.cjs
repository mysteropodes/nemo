'use strict';
// Bounded helper diagnostics and exact-owned cleanup shared by P38 and Reference.
function check(value, code) { if (!value) throw Object.assign(new Error(code), { code }); }
function safeActionDiagnostics(value) {
  check(value && typeof value === 'object' && !Array.isArray(value), 'ax-evidence-refused');
  const stages = new Set(['window-identity','window-identity-ax-owner','window-identity-application-window-membership','window-identity-pinned-cg-window','window-identity-minimized',
    'window-identity-pinned-cg-window-list','sheet-state','route-resolution','axpress-support','axpress-perform','post-action-identity','post-action-ax-owner',
    'post-action-application-window-membership','post-action-pinned-cg-window','post-action-pinned-cg-window-list','post-action-minimized',
    'axtext-request','axtext-identity','axtext-role','axtext-confirm-support','axtext-value-settable','axtext-write','axtext-readback','axtext-confirm',
    'goto-state','goto-context','goto-layout','goto-discovery','goto-deadline','goto-dismissal','goto-ready']);
  check(stages.has(value.stage), 'ax-evidence-refused');
  const clean = { stage:value.stage };
  if(value.gotoPhase!==undefined || value.commandPosted!==undefined){
    const before=['request','pre-tree','pre-context'],after=['posted-pair','post-context','discovery','retained-verification','complete'];
    check(value.stage.startsWith('goto-') && [...before,...after].includes(value.gotoPhase) && typeof value.commandPosted==='boolean' &&
      value.commandPosted===after.includes(value.gotoPhase),'ax-evidence-refused');
    clean.gotoPhase=value.gotoPhase;clean.commandPosted=value.commandPosted;
  }
  if(value.stage==='goto-ready' || value.gotoPhase==='complete')
    check(value.stage==='goto-ready' && value.gotoPhase==='complete' && value.commandPosted===true,'ax-evidence-refused');
  if(value.gotoReason!==undefined){
    check(value.stage.startsWith('goto-') && ['unexpected-refusal','walk-deadline','depth-limit','node-limit','node-owner-query','node-owner-mismatch',
      'cycle','standalone-window','timeout-config','children-query','children-shape','children-limit','child-owner-query','child-owner-mismatch',
      'parent-query','parent-shape','parent-owner-query','parent-owner-mismatch','parent-edge','retained-modal-count','retained-modal-identity',
      'retained-ancestry','retained-field','preexisting-modal','candidate-count','field-count','candidate-proof','candidate-deadline',
      'context-identity','context-shape','context-main-count','context-windows-owner','context-focused-window','context-focus-ancestry',
      'outer-open-control','event-allocation','layout-query','layout-unsupported'].includes(value.gotoReason),'ax-evidence-refused');
    clean.gotoReason=value.gotoReason;
  }
  if(value.gotoRoute!==undefined){
    check(typeof value.gotoRoute==='string' && value.gotoRoute.length<=64 && /^(?:\d{1,3}(?:\.\d{1,3}){0,12})?$/.test(value.gotoRoute) &&
      (value.gotoRoute==='' || value.gotoRoute.split('.').every(n=>Number(n)<=511)),'ax-evidence-refused');clean.gotoRoute=value.gotoRoute;
  }
  for(const [key,max] of [['gotoDepth',13],['gotoNodeCount',512],['gotoChildCount',513],['gotoCandidateCount',512],['gotoFieldCount',512]])
    if(value[key]!==undefined){check(Number.isInteger(value[key])&&value[key]>=0&&value[key]<=max,'ax-evidence-refused');clean[key]=value[key];}
  if(value.gotoQueryError!==undefined){check(Number.isInteger(value.gotoQueryError)&&value.gotoQueryError>=-32768&&value.gotoQueryError<=0,'ax-evidence-refused');clean.gotoQueryError=value.gotoQueryError;}
  for(const key of ['ownerMatches','parentMatches','candidateFresh','candidateOwned','semanticMatched'])
    if(value[key]!==undefined){check(typeof value[key]==='boolean','ax-evidence-refused');clean[key]=value[key];}
  if (value.elementPath !== undefined) { check(typeof value.elementPath === 'string' && /^(?:\d+(?:\.\d+)*)$/.test(value.elementPath) && value.elementPath.length <= 128, 'ax-evidence-refused'); clean.elementPath=value.elementPath; }
  if (value.role !== undefined) { check(['AXButton','AXCheckBox','AXMenuItem','AXRow','AXTextField','other'].includes(value.role), 'ax-evidence-refused'); clean.role=value.role; }
  for (const key of ['axPressQueryError','performActionError','subroleQueryError','focusSettableQueryError','focusedQueryError',
    'valueSettableQueryError','valueWriteError','valueReadbackError','confirmQueryError','confirmError']) {
    if (value[key] !== undefined) { check(Number.isInteger(value[key]) && value[key] >= -32768 && value[key] <= 0, 'ax-evidence-refused'); clean[key]=value[key]; }
  }
  for(const key of ['enabled','focusSettable','focusedMatches','valueSettable','valueMatches','confirmAdvertised'])
    if(value[key]!==undefined){check(typeof value[key]==='boolean','ax-evidence-refused');clean[key]=value[key];}
  if(value.textFieldKind!==undefined){check(['plain','search','other','unreadable'].includes(value.textFieldKind),'ax-evidence-refused');clean.textFieldKind=value.textFieldKind;}
  if (value.axPressAdvertised !== undefined) { check(typeof value.axPressAdvertised === 'boolean', 'ax-evidence-refused'); clean.axPressAdvertised=value.axPressAdvertised; }
  if (value.control !== undefined) { check(['Open','Open Project…','Cancel','A','B','A.json','B.json','Motion','Opacity','R08 rectangle'].includes(value.control), 'ax-evidence-refused'); clean.control=value.control; }
  if (value.routeRefusal !== undefined) { check(['adapter-refused','child-owner','standalone-window'].includes(value.routeRefusal), 'ax-evidence-refused'); clean.routeRefusal=value.routeRefusal; }
  for (const key of ['axPidQueryError','applicationWindowsQueryError','minimizedQueryError']) {
    if (value[key] !== undefined) { check(Number.isInteger(value[key]) && value[key] >= -32768 && value[key] <= 0, 'ax-evidence-refused'); clean[key]=value[key]; }
  }
  for (const key of ['axPidMatches','applicationWindowsReadable','pinnedAXWindowMember','minimizedReadable','minimized','pinnedCGWindowListReadable'])
    if (value[key] !== undefined) { check(typeof value[key] === 'boolean', 'ax-evidence-refused'); clean[key]=value[key]; }
  if (value.pinnedCGWindowCount !== undefined) { check(Number.isInteger(value.pinnedCGWindowCount) && value.pinnedCGWindowCount >= 0 && value.pinnedCGWindowCount <= 128, 'ax-evidence-refused'); clean.pinnedCGWindowCount=value.pinnedCGWindowCount; }
  for (const key of ['retainedMainChildrenQueryError'])
    if (value[key] !== undefined) { check(Number.isInteger(value[key]) && value[key] >= -32768 && value[key] <= 0, 'ax-evidence-refused'); clean[key]=value[key]; }
  for (const key of ['retainedMainChildrenReadable','retainedMainChildrenCountCapped','retainedMainChildrenTruncated'])
    if (value[key] !== undefined) { check(typeof value[key] === 'boolean', 'ax-evidence-refused'); clean[key]=value[key]; }
  if (value.retainedMainChildrenCount !== undefined) {
    check(Number.isInteger(value.retainedMainChildrenCount) && value.retainedMainChildrenCount >= 0 && value.retainedMainChildrenCount <= 4096, 'ax-evidence-refused');
    clean.retainedMainChildrenCount=value.retainedMainChildrenCount;
  }
  if (value.retainedMainSheetProbe !== undefined) {
    check(['children-query-error','children-unreadable','children-truncated','candidate-probe-incomplete','no-attached-sheet','one-attached-sheet','multiple-attached-sheets'].includes(value.retainedMainSheetProbe), 'ax-evidence-refused');
    clean.retainedMainSheetProbe=value.retainedMainSheetProbe;
  }
  if (value.retainedMainChildFacts !== undefined) {
    check(Array.isArray(value.retainedMainChildFacts) && value.retainedMainChildFacts.length <= 64, 'ax-evidence-refused');
    clean.retainedMainChildFacts=value.retainedMainChildFacts.map(fact=>{
      check(fact && typeof fact === 'object' && !Array.isArray(fact), 'ax-evidence-refused');
      const safe={};
      if(fact.index!==undefined){check(Number.isInteger(fact.index)&&fact.index>=0&&fact.index<64,'ax-evidence-refused');safe.index=fact.index;}
      for(const key of ['candidateTimeoutConfigError','candidatePidQueryError','candidateRoleQueryError',
        'candidateParentQueryError','candidateParentTimeoutConfigError','candidateParentPidQueryError'])
        if(fact[key]!==undefined){check(Number.isInteger(fact[key])&&fact[key]>=-32768&&fact[key]<=0,'ax-evidence-refused');safe[key]=fact[key];}
      if(fact.candidatePid!==undefined){check(Number.isSafeInteger(fact.candidatePid)&&fact.candidatePid>=0&&fact.candidatePid<=2147483647,'ax-evidence-refused');safe.candidatePid=fact.candidatePid;}
      for(const key of ['candidatePidMatches','candidateRoleReadable','candidateParentReadable','candidateParentPidMatches','candidateParentIsRetainedMain'])
        if(fact[key]!==undefined){check(typeof fact[key]==='boolean','ax-evidence-refused');safe[key]=fact[key];}
      if(fact.candidateRole!==undefined){check(['AXSheet','other'].includes(fact.candidateRole),'ax-evidence-refused');safe.candidateRole=fact.candidateRole;}
      return safe;
    });
  }
  if (value.transitionObservation !== undefined) {
    check(value.stage==='post-action-application-window-membership', 'ax-evidence-refused');
    const observation=value.transitionObservation;
    const statuses=new Set(['timeout-config-refused','deadline','inconclusive','membership-returned','persistent-omission']);
    check(observation&&typeof observation==='object'&&!Array.isArray(observation)&&statuses.has(observation.status), 'ax-evidence-refused');
    check(observation.budgetMs===750&&observation.budgetKind==='parent-supervised-one-shot'&&
      observation.budgetScope==='transition-samples-only'&&typeof observation.axTimeoutsConfigured==='boolean', 'ax-evidence-refused');
    check(Number.isInteger(observation.elapsedMs)&&observation.elapsedMs>=0&&observation.elapsedMs<=10000, 'ax-evidence-refused');
    if(observation.status!=='deadline')check(observation.elapsedMs<=750,'ax-evidence-refused');
    check(Array.isArray(observation.samples)&&observation.samples.length<=2, 'ax-evidence-refused');
    if(observation.status==='timeout-config-refused')check(observation.samples.length===0, 'ax-evidence-refused');
    if(observation.status==='timeout-config-refused')check(observation.axTimeoutsConfigured===false, 'ax-evidence-refused');
    if(observation.status==='inconclusive'||observation.status==='membership-returned'||observation.status==='persistent-omission')
      check(observation.samples.length===2&&observation.axTimeoutsConfigured===true, 'ax-evidence-refused');
    if(observation.status==='deadline')check(observation.samples.length<=2, 'ax-evidence-refused');
    const sampleFacts=observation.samples.map((sample,index)=>{
      check(sample&&typeof sample==='object'&&!Array.isArray(sample)&&sample.sampleIndex===index, 'ax-evidence-refused');
      check(Number.isInteger(sample.elapsedMs)&&sample.elapsedMs>=0&&sample.elapsedMs<=observation.elapsedMs, 'ax-evidence-refused');
      const cleanSample={sampleIndex:index,elapsedMs:sample.elapsedMs};
      if(sample.sampleDeadlineReached!==true||sample.pinnedCGWindowListReadable!==undefined||sample.pinnedCGWindowCount!==undefined)
        check(['complete','failed','deadline'].includes(sample.cgWorkerStatus),'ax-evidence-refused');
      for(const key of ['axPidQueryError','applicationWindowsQueryError','minimizedQueryError','retainedMainChildrenQueryError'])
        if(sample[key]!==undefined){check(Number.isInteger(sample[key])&&sample[key]>=-32768&&sample[key]<=0,'ax-evidence-refused');cleanSample[key]=sample[key];}
      for(const key of ['axPidMatches','applicationWindowsReadable','pinnedAXWindowMember','minimizedReadable','minimized',
        'pinnedCGWindowListReadable','retainedMainChildrenReadable','retainedMainChildrenCountCapped','sampleDeadlineReached'])
        if(sample[key]!==undefined){check(typeof sample[key]==='boolean','ax-evidence-refused');cleanSample[key]=sample[key];}
      if(sample.cgWorkerStatus!==undefined){check(['complete','failed','deadline'].includes(sample.cgWorkerStatus),'ax-evidence-refused');cleanSample.cgWorkerStatus=sample.cgWorkerStatus;}
      if(sample.cgWorkerCleanup!==undefined){check(['complete','incomplete','not-started'].includes(sample.cgWorkerCleanup),'ax-evidence-refused');cleanSample.cgWorkerCleanup=sample.cgWorkerCleanup;}
      if(sample.cgWorkerStatus==='complete'){
        check(sample.cgWorkerCleanup==='complete'&&typeof sample.pinnedCGWindowListReadable==='boolean','ax-evidence-refused');
        if(sample.pinnedCGWindowListReadable)check(Number.isInteger(sample.pinnedCGWindowCount)&&sample.pinnedCGWindowCount>=0&&sample.pinnedCGWindowCount<=128,'ax-evidence-refused');
        else check(sample.pinnedCGWindowCount===undefined,'ax-evidence-refused');
      }
      if(sample.cgWorkerStatus==='failed')check(sample.cgWorkerCleanup==='complete'&&sample.pinnedCGWindowListReadable===false,'ax-evidence-refused');
      if(sample.cgWorkerStatus==='deadline')check(observation.status==='deadline'&&sample.sampleDeadlineReached===true&&
        sample.pinnedCGWindowListReadable===false&&sample.pinnedCGWindowCount===undefined&&
        ['complete','incomplete','not-started'].includes(sample.cgWorkerCleanup),'ax-evidence-refused');
      if(sample.cgWorkerCleanup==='incomplete')check(sample.cgWorkerStatus==='deadline','ax-evidence-refused');
      for(const key of ['pinnedCGWindowCount','retainedMainChildrenCount'])
        if(sample[key]!==undefined){const max=key==='retainedMainChildrenCount'?4096:128;check(Number.isInteger(sample[key])&&sample[key]>=0&&sample[key]<=max,'ax-evidence-refused');cleanSample[key]=sample[key];}
      check(sample.retainedMainChildFacts===undefined&&sample.retainedMainSheetProbe===undefined,'ax-evidence-refused');
      return cleanSample;
    });
    if(observation.status!=='deadline')
      check(sampleFacts.every(sample=>sample.sampleDeadlineReached!==true), 'ax-evidence-refused');
    if(sampleFacts.length===2&&observation.status!=='deadline'){
      const usable=sampleFacts.every(sample=>sample.axPidMatches===true&&sample.applicationWindowsQueryError===0&&
        sample.applicationWindowsReadable===true&&sample.minimizedQueryError===0&&sample.minimizedReadable===true&&
        sample.minimized===false&&sample.pinnedCGWindowListReadable===true&&sample.pinnedCGWindowCount===1&&
        sample.retainedMainChildrenQueryError===0&&sample.retainedMainChildrenReadable===true);
      const derived=!usable?'inconclusive':sampleFacts.some(sample=>sample.pinnedAXWindowMember===true)?'membership-returned':
        sampleFacts.every(sample=>sample.pinnedAXWindowMember===false)?'persistent-omission':'inconclusive';
      check(observation.status===derived,'ax-evidence-refused');
    }
    clean.transitionObservation={status:observation.status,budgetMs:750,budgetKind:'parent-supervised-one-shot',
      budgetScope:'transition-samples-only',axTimeoutsConfigured:observation.axTimeoutsConfigured,
      elapsedMs:observation.elapsedMs,samples:sampleFacts};
  }
  return clean;
}
function safeCleanupDiagnostics(values) {
  check(Array.isArray(values) && values.length <= 8, 'cleanup-evidence-refused');
  const stages = new Set(['helper-exited','exact-helper-terminated','exact-helper-remains','group-absent','group-terminated','group-remains',
    'helper-pid-refused','helper-stdin-error','exact-helper-signal-error','exact-helper-wait-error','group-probe-error','group-identity-uncertain','group-signal-error','group-wait-error','cleanup-loop-error','input-close-error']);
  return values.map(value => {
    check(value && typeof value === 'object' && stages.has(value.stage), 'cleanup-evidence-refused');
    const clean = { stage:value.stage };
    if (value.helperPid !== undefined) { check(Number.isSafeInteger(value.helperPid) && value.helperPid > 0 && value.helperPid <= 2147483647, 'cleanup-evidence-refused'); clean.helperPid=value.helperPid; }
    for (const key of ['descendantPossible','groupAliveBefore','groupAliveAfterTerm','groupAliveAfterKill','childExitObserved','childSignalObserved','transientProbeDenied']) {
      if (value[key] !== undefined) { check(typeof value[key] === 'boolean', 'cleanup-evidence-refused'); clean[key]=value[key]; }
    }
    if (value.signalAttempts !== undefined) { check(Number.isInteger(value.signalAttempts) && value.signalAttempts >= 0 && value.signalAttempts <= 2, 'cleanup-evidence-refused'); clean.signalAttempts=value.signalAttempts; }
    if (value.exitObserved !== undefined) { check(typeof value.exitObserved === 'boolean', 'cleanup-evidence-refused'); clean.exitObserved=value.exitObserved; }
    for (const key of ['helperPidValid','signalError','probeError']) {
      if (value[key] !== undefined) { check(typeof value[key] === 'boolean', 'cleanup-evidence-refused'); clean[key]=value[key]; }
    }
    if (value.signalKind !== undefined) { check(['SIGTERM','SIGKILL'].includes(value.signalKind), 'cleanup-evidence-refused'); clean.signalKind=value.signalKind; }
    if (value.probePoint !== undefined) { check(['initial','wait-term','after-term','wait-kill','after-kill'].includes(value.probePoint), 'cleanup-evidence-refused'); clean.probePoint=value.probePoint; }
    for(const key of ['probeErrorCode','signalErrorCode']) if(value[key]!==undefined){
      check(['EPERM','EACCES','EINVAL','ENOSYS','ERR_INVALID_PROBE_RESULT','UNKNOWN'].includes(value[key]),'cleanup-evidence-refused');clean[key]=value[key];}
    if (value.helpersRemaining !== undefined) { check(Number.isInteger(value.helpersRemaining) && value.helpersRemaining >= 0 && value.helpersRemaining <= 8, 'cleanup-evidence-refused'); clean.helpersRemaining=value.helpersRemaining; }
    return clean;
  });
}
function cleanupFailureDiagnostics(error, completed = [], helpersRemaining = 0) {
  const reported = Array.isArray(error?.cleanupDiagnostics) ? error.cleanupDiagnostics : [];
  if (reported.length > 0) {
    const room=Math.max(0,8-Math.min(reported.length,8));
    return [...(room===0?[]:completed.slice(-room)),...reported.slice(-8)];
  }
  return [...completed.slice(-7),{stage:'cleanup-loop-error',helpersRemaining:Math.min(Math.max(helpersRemaining,0),8)}];
}
async function closeOwnedHelpers(closeInput, children, cleanup) {
  const completed=[], failures=[];
  try { closeInput(); } catch { failures.push({stage:'input-close-error',helpersRemaining:Math.min(children.length,8)}); }
  for (let index=0;index<children.length;index++) {
    try { completed.push(...(await cleanup(children[index]) || [])); }
    catch (error) {
      failures.push(...cleanupFailureDiagnostics(error,[],children.length-index));
    }
  }
  const successBudget=Math.max(0,8-Math.min(failures.length,8));
  const diagnostics=[...completed.slice(0,successBudget),...failures.slice(-8)];
  if (failures.length>0)
    throw Object.assign(Error('helper-cleanup-failed'),{code:'helper-cleanup-failed',cleanupDiagnostics:diagnostics});
  return {cleanupDiagnostics:diagnostics};
}
async function cleanupHelper(child, services = {}) {
  if(!Number.isSafeInteger(child.pid))return; // Spawn failed before any process was created.
  const observation = { helperPid:child.pid, descendantPossible:child.p38GroupMayContainDescendants !== false };
  if(child.pid<=0||child.pid>2147483647)throw Object.assign(Error('helper-cleanup-invalid'),{code:'helper-cleanup-invalid',cleanupDiagnostics:[{stage:'helper-pid-refused',helperPidValid:false,descendantPossible:observation.descendantPossible}]});
  const failed=(stage,facts={})=>Object.assign(Error('helper-cleanup-failed'),{code:'helper-cleanup-failed',cleanupDiagnostics:[{...observation,stage,
    childExitObserved:Number.isInteger(child.exitCode),childSignalObserved:typeof child.signalCode==='string',...facts}]});
  const errorCode=error=>['EPERM','EACCES','EINVAL','ENOSYS','ERR_INVALID_PROBE_RESULT'].includes(error?.code)?error.code:'UNKNOWN';
  const exited=()=>Number.isInteger(child.exitCode)||typeof child.signalCode==='string';
  try { if(child.stdin&&!child.stdin.destroyed&&!child.stdin.writableEnded)child.stdin.end(); }
  catch { throw failed('helper-stdin-error'); }
  // The helper may own a CG sampler after a refused press or an explicit
  // capture. Once a non-descendant helper exits, do not signal a recycled PGID.
  if(child.p38GroupMayContainDescendants===false){
    for(const kind of ['SIGTERM','SIGKILL']){
      if(child.exitCode!==null||child.signalCode!==null)return [{...observation,stage:'helper-exited',exitObserved:true}];
      try { child.kill(kind); } catch { throw failed('exact-helper-signal-error',{signalKind:kind,signalAttempts:kind==='SIGTERM'?1:2,signalError:true}); }
      for(let i=0;i<40&&child.exitCode===null&&child.signalCode===null;i++)try { await (services.delay?services.delay():new Promise(resolve=>setTimeout(resolve,50))); }
        catch { throw failed('exact-helper-wait-error',{signalKind:kind,signalAttempts:kind==='SIGTERM'?1:2,probePoint:kind==='SIGTERM'?'wait-term':'wait-kill'}); }
    }
    const exited=child.exitCode!==null||child.signalCode!==null;
    const diagnostics=[{...observation,stage:exited?'exact-helper-terminated':'exact-helper-remains',exitObserved:exited,signalAttempts:2}];
    if(!exited)throw Object.assign(Error('helper-group-remains'),{code:'helper-group-remains',cleanupDiagnostics:diagnostics});
    return diagnostics;
  }
  const alive = services.alive || (()=>{try{process.kill(-child.pid,0);return true;}catch(e){if(e.code==='ESRCH')return false;throw e;}});
  const signal = services.signal || (kind=>{try{process.kill(-child.pid,kind);}catch(e){if(e.code!=='ESRCH')throw e;}});
  const delay = services.delay || (()=>new Promise(resolve=>setTimeout(resolve,50)));
  const exactAlive = services.exactAlive || (()=>child.exitCode===null&&child.signalCode===null&&
    typeof child.kill==='function'&&child.kill(0)===true);
  const verifiedGroup=()=>{try{return child.p38DetachedGroup===true&&exactAlive()===true;}catch{return false;}};
  if(child.stdin)for(let i=0;i<4&&!exited();i++)try { await delay(); }
    catch { throw failed('group-wait-error',{probePoint:'initial',signalAttempts:0}); }
  const probe=(point,attempts,signalKind)=>{
    try { const result=alive(); if(typeof result!=='boolean')throw {code:'ERR_INVALID_PROBE_RESULT'}; return result; }
    catch(error) { throw failed('group-probe-error',{probePoint:point,signalAttempts:attempts,...(signalKind?{signalKind}:{}),probeError:true,probeErrorCode:errorCode(error)}); }
  };
  let groupAliveBefore,transientProbeDenied=false;
  try { groupAliveBefore=probe('initial',0); }
  catch(error) {
    if(error.cleanupDiagnostics?.[0]?.probeErrorCode!=='EPERM')throw error;
    transientProbeDenied=true;
    // An inaccessible group is never signaled. Only an observed child exit
    // followed by ESRCH can turn this transient refusal into completion.
    for(let i=0;i<20;i++){
      try { await delay(); } catch { throw failed('group-wait-error',{probePoint:'initial',signalAttempts:0}); }
      if(!exited())continue;
      try { groupAliveBefore=probe('initial',0); }
      catch(next) { if(next.cleanupDiagnostics?.[0]?.probeErrorCode==='EPERM')continue; throw next; }
      if(groupAliveBefore===false)return [{...observation,stage:'group-absent',groupAliveBefore:false,
        childExitObserved:true,transientProbeDenied,signalAttempts:0}];
    }
    if(groupAliveBefore===true)throw failed('group-remains',{groupAliveBefore:true,transientProbeDenied,signalAttempts:0});
    throw failed('group-probe-error',{probePoint:'initial',signalAttempts:0,probeError:true,probeErrorCode:'EPERM',transientProbeDenied});
  }
  if(!groupAliveBefore){
    for(let i=0;i<20&&!exited();i++)try { await delay(); }
      catch { throw failed('group-wait-error',{probePoint:'initial',signalAttempts:0}); }
    if(!exited())throw failed('group-identity-uncertain',{probePoint:'initial',signalAttempts:0});
    if(probe('initial',0))throw failed('group-remains',{groupAliveBefore:true,signalAttempts:0});
    return [{...observation,stage:'group-absent',groupAliveBefore:false,childExitObserved:true,signalAttempts:0}];
  }
  // A dead leader may leave descendants or a recycled PGID. Never signal it.
  if(exited())throw failed('group-remains',{groupAliveBefore:true,signalAttempts:0});
  if(!verifiedGroup())
    throw failed('group-identity-uncertain',{probePoint:'initial',signalAttempts:0});
  let groupAliveAfterTerm,groupAliveAfterKill,signalAttempts=0;
  for(const kind of ['SIGTERM','SIGKILL']){
    if(exited())throw failed('group-remains',{groupAliveBefore,groupAliveAfterTerm,signalAttempts});
    if(!verifiedGroup())
      throw failed('group-identity-uncertain',{probePoint:signalAttempts?'after-term':'initial',signalAttempts});
    signalAttempts++;
    try { signal(kind); } catch(error) { throw failed('group-signal-error',{groupAliveBefore,signalAttempts,signalKind:kind,signalError:true,signalErrorCode:errorCode(error)}); }
    const waitPoint=kind==='SIGTERM'?'wait-term':'wait-kill', afterPoint=kind==='SIGTERM'?'after-term':'after-kill';
    for(let i=0;i<40;i++){
      if(!probe(waitPoint,signalAttempts,kind)&&exited())break;
      try { await delay(); }catch { throw failed('group-wait-error',{groupAliveBefore,signalAttempts,signalKind:kind,probePoint:waitPoint}); }
    }
    const remains=probe(afterPoint,signalAttempts,kind);
    if(kind==='SIGTERM')groupAliveAfterTerm=remains;
    else groupAliveAfterKill=remains;
    if(!remains&&exited())return [{...observation,stage:'group-terminated',groupAliveBefore,groupAliveAfterTerm,groupAliveAfterKill,
      childExitObserved:true,signalAttempts}];
    if(!remains)throw failed('group-identity-uncertain',{probePoint:afterPoint,signalAttempts});
    if(exited())throw failed('group-remains',{groupAliveBefore,groupAliveAfterTerm,groupAliveAfterKill,signalAttempts});
  }
  throw Object.assign(Error('helper-group-remains'),{code:'helper-group-remains',cleanupDiagnostics:[{...observation,stage:'group-remains',groupAliveBefore,groupAliveAfterTerm,groupAliveAfterKill,signalAttempts}]});
}
module.exports={safeActionDiagnostics,safeCleanupDiagnostics,cleanupFailureDiagnostics,closeOwnedHelpers,cleanupHelper};
