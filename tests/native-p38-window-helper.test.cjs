'use strict';
const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const crypto=require('node:crypto');
const {execFileSync}=require('node:child_process');
const {runOperator,verifyIdentity,verifyWindow,captureStartupDiagnostics,classifyStartupStderr,classifyManifestOutcome}=require('./desktop/p38-installed-driver.cjs');
const {HEAD,HASH,config,fixture}=require('./desktop/p38-operator-model.cjs');
test('retained-main sheet probe is bounded, identity-gated, sanitized and never resumes a refused press',async()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const ownedSheet={index:0,candidatePidQueryError:0,candidatePid:123,candidatePidMatches:true,candidateRoleQueryError:0,
    candidateRoleReadable:true,candidateRole:'AXSheet',candidateParentQueryError:0,candidateParentReadable:true,
    candidateParentPidQueryError:0,candidateParentPidMatches:true,candidateParentIsRetainedMain:true,privatePath:'/private/secret'};
  const scenarios=[
    {name:'query error',queryError:-25205,readable:false,count:0,facts:[],probe:'children-query-error'},
    {name:'foreign child',queryError:0,readable:true,count:1,facts:[{index:0,candidatePidQueryError:0,candidatePid:124,candidatePidMatches:false}],probe:'no-attached-sheet'},
    {name:'multiple sheets',queryError:0,readable:true,count:2,facts:[ownedSheet,{...ownedSheet,index:1}],probe:'multiple-attached-sheets'},
    {name:'parent mismatch',queryError:0,readable:true,count:1,facts:[{...ownedSheet,candidateParentIsRetainedMain:false}],probe:'no-attached-sheet'},
    {name:'exact attached sheet',queryError:0,readable:true,count:1,facts:[ownedSheet],probe:'one-attached-sheet'},
  ];
  for(const scenario of scenarios){
    const diagnostic={stage:'post-action-application-window-membership',elementPath:'2',
      axPidQueryError:0,axPidMatches:true,applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:false,
      minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowListReadable:true,pinnedCGWindowCount:1,
      retainedMainChildrenQueryError:scenario.queryError,retainedMainChildrenReadable:scenario.readable,
      ...(scenario.readable?{retainedMainChildrenCount:scenario.count,retainedMainChildrenCountCapped:false,retainedMainChildrenTruncated:false}:{}),
      retainedMainChildFacts:scenario.facts,retainedMainSheetProbe:scenario.probe,privatePath:'/private/identity'};
    const clean=safeActionDiagnostics(diagnostic);
    assert.equal(clean.retainedMainSheetProbe,scenario.probe,scenario.name);
    if(scenario.probe==='one-attached-sheet')assert.equal(clean.retainedMainChildFacts[0].candidateParentIsRetainedMain,true);
    assert.equal(JSON.stringify(clean).includes('/private/'),false,scenario.name);
    const f=fixture();let requests=0;f.services.next=async()=>{requests++;return{type:'press',element:'2'};};
    f.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
      {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.firstFailure.code,'adapter-refused',scenario.name);
    assert.equal(result.firstFailure.stage,'window-press-main',scenario.name);
    assert.equal(result.firstFailure.actionDiagnostics.retainedMainSheetProbe,scenario.probe,scenario.name);
    assert.equal(result.installedAcceptance,'not-established',scenario.name);
    assert.equal(requests,1,scenario.name);
  }
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainSheetProbe:'AXSheet'}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:Array(65).fill({index:0})}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:[{index:0,candidateRole:'/private'}]}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',retainedMainChildFacts:[{index:0,candidatePid:999999999999}]}));
});
test('bounded post-press transition samples are sanitized and never resume a refused press',async()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const helperSource=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  const samplerStart=helperSource.indexOf('  func transitionObservationDiagnostic()');
  const samplerEnd=helperSource.indexOf('  func verify()',samplerStart);
  const sampler=helperSource.slice(samplerStart,samplerEnd);
  assert.ok(samplerStart>=0&&samplerEnd>samplerStart);
  assert.match(sampler,/let deadline = started \+ 750_000_000/);
  assert.match(sampler,/for index in 0\.\.\<2/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout/);
  assert.match(sampler,/kAXChildrenAttribute/);
  assert.match(sampler,/budgetKind":"parent-supervised-one-shot/);
  assert.match(sampler,/boundedPinnedCGWindowSample\(pid,number,deadline\)/);
  assert.doesNotMatch(sampler,/CGWindowListCopyWindowInfo/);
  assert.match(helperSource,/func cgWindowSampleWorker\(_ arguments:\[String\]\)/);
  assert.match(helperSource,/process\.arguments=\["--p38-cg-sample",String\(pid\),String\(window\)\]/);
  assert.match(helperSource,/process\.environment=\[:\]/);
  assert.match(helperSource,/process\.terminationReason == \.exit && process\.terminationStatus == 0/);
  assert.match(helperSource,/superviseBoundedWorker\(deadline:deadline/);
  assert.match(helperSource,/fakeNow=21\s+return \.completed\(Data\(\[2\]\)\)/);
  assert.match(helperSource,/workerDeadlineCases":3/);
  assert.match(sampler,/budgetScope":"transition-samples-only/);
  assert.match(sampler,/facts\["sampleDeadlineReached"\] = true/);
  assert.match(sampler,/transitionObservationStatus\(samples,overrun\)/);
  assert.match(sampler,/let completed = DispatchTime\.now\(\)\.uptimeNanoseconds/);
  assert.match(sampler,/let overrun = completed >= deadline/);
  assert.doesNotMatch(sampler,/AXUIElementPerformAction|AXUIElementPostKeyboardEvent|CGEvent/);
  const refusalPath=helperSource.slice(helperSource.indexOf('if shouldProbeRetainedMainChildren(error.stage,error.facts)'),
    helperSource.indexOf('throw pressFailure("post-action-"',helperSource.indexOf('if shouldProbeRetainedMainChildren(error.stage,error.facts)')));
  assert.ok(refusalPath.indexOf('transitionObservationDiagnostic()')<refusalPath.indexOf('retainedMainChildrenDiagnostic()'));
  assert.match(refusalPath,/transitionObservationDiagnostic\(\)/);
  assert.match(refusalPath,/retainedMainChildrenDiagnostic\(\)/);
  assert.match(refusalPath,/configured && status != "deadline" && status != "timeout-config-refused"/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout\(application,timeout\)/);
  assert.match(sampler,/AXUIElementSetMessagingTimeout\(main,timeout\)/);
  const stable=(index,extra={})=>({sampleIndex:index,elapsedMs:index?330:0,axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:false,
    minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowListReadable:true,pinnedCGWindowCount:1,
    cgWorkerStatus:'complete',cgWorkerCleanup:'complete',
    retainedMainChildrenQueryError:0,retainedMainChildrenReadable:true,retainedMainChildrenCount:1,
    retainedMainChildrenCountCapped:false,...extra});
  const scenarios=[
    {name:'membership returns',status:'membership-returned',samples:[stable(0),stable(1,{pinnedAXWindowMember:true})]},
    {name:'direct child count changes',status:'persistent-omission',samples:[stable(0),stable(1,{retainedMainChildrenCount:2})]},
    {name:'persistent omission',status:'persistent-omission',samples:[stable(0),stable(1)]},
    {name:'identity drift',status:'inconclusive',samples:[stable(0),stable(1,{axPidMatches:false})]},
    {name:'AX query error',status:'inconclusive',samples:[stable(0),stable(1,{applicationWindowsQueryError:-25205,applicationWindowsReadable:false})]},
    {name:'direct child query error',status:'inconclusive',samples:[stable(0),stable(1,{retainedMainChildrenQueryError:-25205,retainedMainChildrenReadable:false})]},
  ];
  for(const scenario of scenarios){
    const diagnostic={stage:'post-action-application-window-membership',elementPath:'2',transitionObservation:{
      status:scenario.status,budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
      axTimeoutsConfigured:true,elapsedMs:480,samples:scenario.samples},privatePath:'/private/identity'};
    const clean=safeActionDiagnostics(diagnostic);
    assert.equal(clean.transitionObservation.status,scenario.status,scenario.name);
    assert.equal(clean.transitionObservation.samples.length,2,scenario.name);
    assert.equal(JSON.stringify(clean).includes('/private/'),false,scenario.name);
    const f=fixture();let requests=0,presses=0;f.services.next=async()=>{requests++;return{type:'press',element:'2'};};
    f.services.window=async(mode,pid,wid,action)=>{
      f.events.push('window-'+mode);
      if(mode==='discover')return{pid,windowId:45,matchedAX:true};
      if(mode==='press'){presses++;return{status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};}
      return{status:'ok',pid,windowId:45,matchedAX:true};
    };
    const result=await runOperator(f.cfg,f.services);
    assert.equal(result.status,'partial',scenario.name);assert.equal(result.reason,'adapter-refused',scenario.name);
    assert.equal(result.firstFailure.actionDiagnostics.transitionObservation.status,scenario.status,scenario.name);
    assert.equal(result.installedAcceptance,'not-established',scenario.name);
    assert.equal(requests,1,scenario.name);assert.equal(presses,1,scenario.name);
    assert.equal(f.events.filter(event=>event==='window-press').length,1,scenario.name);
  }
  const timeout=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'timeout-config-refused',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:false,elapsedMs:8,samples:[]}});
  assert.deepEqual(timeout.transitionObservation,{status:'timeout-config-refused',budgetMs:750,budgetKind:'parent-supervised-one-shot',
    budgetScope:'transition-samples-only',axTimeoutsConfigured:false,elapsedMs:8,samples:[]});
  const deadline=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:740,samples:[stable(0)]}});
  assert.equal(deadline.transitionObservation.status,'deadline');
  const overrunDeadline=safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:810,
    samples:[stable(0),stable(1,{pinnedAXWindowMember:true,sampleDeadlineReached:true})]}});
  assert.equal(overrunDeadline.transitionObservation.elapsedMs,810);
  assert.equal(overrunDeadline.transitionObservation.status,'deadline');
  const workerDeadline={stage:'post-action-application-window-membership',elementPath:'2',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:812,samples:[{sampleIndex:0,elapsedMs:750,sampleDeadlineReached:true,
      pinnedCGWindowListReadable:false,cgWorkerStatus:'deadline',cgWorkerCleanup:'incomplete'}]}};
  const workerTimeoutClean=safeActionDiagnostics(workerDeadline);
  assert.equal(workerTimeoutClean.transitionObservation.status,'deadline');
  assert.equal(workerTimeoutClean.transitionObservation.samples[0].pinnedCGWindowCount,undefined);
  assert.equal(workerTimeoutClean.transitionObservation.samples[0].cgWorkerCleanup,'incomplete');
  assert.throws(()=>safeActionDiagnostics({...workerDeadline,transitionObservation:{...workerDeadline.transitionObservation,
    samples:[{...workerDeadline.transitionObservation.samples[0],pinnedCGWindowCount:1}]}}));
  const terminal=fixture();let terminalRequests=0,terminalPresses=0;
  terminal.services.next=async()=>{terminalRequests++;return{type:'press',element:'2'};};
  terminal.services.window=async(mode,pid,wid)=>{
    if(mode==='discover'||mode==='check')return{pid,windowId:45,matchedAX:true};
    if(mode==='press'){terminalPresses++;return{status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:workerDeadline};}
    throw Error('unexpected-resumed-action');
  };
  const terminalResult=await runOperator(terminal.cfg,terminal.services);
  assert.equal(terminalResult.firstFailure.actionDiagnostics?.transitionObservation?.status,'deadline',JSON.stringify(terminalResult.firstFailure));
  assert.equal(terminalRequests,1);assert.equal(terminalPresses,1);
  for(const observation of [
    {status:'persistent-omission',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:751,samples:[stable(0),stable(1)]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1),stable(2)]},
    {status:'attached-sheet-observed',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1)]},
    {status:'timeout-config-refused',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:false,elapsedMs:0,samples:[stable(0)]},
    {status:'deadline',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:10001,samples:[stable(0),stable(1)]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,samples:[stable(0),stable(1,{pinnedCGWindowCount:0})]},
    {status:'membership-returned',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:true,elapsedMs:500,
      samples:[stable(0),stable(1,{pinnedAXWindowMember:true,sampleDeadlineReached:true})]},
    {status:'persistent-omission',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',axTimeoutsConfigured:false,
      elapsedMs:500,samples:[stable(0),stable(1)]},
  ])assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:observation}));
  assert.throws(()=>safeActionDiagnostics({stage:'post-action-application-window-membership',transitionObservation:{
    status:'persistent-omission',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:true,elapsedMs:750,
    samples:[stable(0),stable(1,{pinnedAXWindowMember:true})]}}));
  assert.throws(()=>safeActionDiagnostics({stage:'axpress-perform',transitionObservation:{
    status:'deadline',budgetMs:750,budgetKind:'parent-supervised-one-shot',budgetScope:'transition-samples-only',
    axTimeoutsConfigured:false,elapsedMs:0,samples:[]}}));
});
test('helper probe, wait and live-group failures stay sanitized and signal-free',async()=>{
  const {cleanupHelper}=require('./desktop/p38-installed-driver.cjs');
  const child={pid:321,p38DetachedGroup:true,exitCode:0,signalCode:null};
  const cases=[
    {stage:'group-probe-error',services:{alive(){throw Error('/private/probe');}}},
    {stage:'group-remains',services:{alive:()=>true,delay:async()=>{}}},
    {stage:'group-wait-error',services:{alive:()=>true,delay(){throw Error('/private/delay');}}},
  ];
  for(const item of cases){
    let signals=0;
    await assert.rejects(cleanupHelper(child,{...item.services,signal(){signals++;}}),error=>{
      assert.equal(error.cleanupDiagnostics[0].stage,item.stage);
      const {safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
      const sanitized=safeCleanupDiagnostics(error.cleanupDiagnostics);
      assert.equal(JSON.stringify(sanitized).includes('/private/'),false);
      assert.equal(sanitized[0].signalAttempts,0);
      assert.notEqual(sanitized.length,0);
      return true;
    });
    assert.equal(signals,0);
  }
});
test('non-capture helper requires cooperative exit and group absence',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const child={pid:321,p38GroupMayContainDescendants:false,exitCode:null,signalCode:null,
    stdin:{destroyed:false,writableEnded:false,end(){child.exitCode=0;}}};
  let signals=0;
  assert.equal((await cleanupHelper(child,{alive:()=>false,signal(){signals++;}}))[0].stage,'helper-exited');
  assert.equal(signals,0);
  const waiting={pid:321,p38GroupMayContainDescendants:false,exitCode:null,signalCode:null};
  await assert.rejects(cleanupHelper(waiting,{alive:()=>false,signal(){signals++;},delay(){throw Error('/private/wait');}}),error=>{
    assert.deepEqual(safeCleanupDiagnostics(error.cleanupDiagnostics),[{stage:'group-wait-error',helperPid:321,descendantPossible:false,
      childExitObserved:false,childSignalObserved:false,signalAttempts:0,probePoint:'initial'}]);return true;
  });
  const badInput={pid:321,p38GroupMayContainDescendants:false,exitCode:null,signalCode:null,
    stdin:{destroyed:false,writableEnded:false,end(){throw Error('/private/stdin');}}};
  await assert.rejects(cleanupHelper(badInput,{alive:()=>false,signal(){signals++;}}),error=>{
    assert.equal(safeCleanupDiagnostics(error.cleanupDiagnostics)[0].stage,'helper-stdin-error');return true;
  });
  assert.equal(signals,0);
});
test('cleanup loop preserves detailed exception facts or supplies a bounded fallback stage',()=>{
  const {cleanupFailureDiagnostics,safeCleanupDiagnostics,closeOwnedHelpers}=require('./desktop/p38-installed-driver.cjs');
  const prior=[{stage:'group-absent',helperPid:123,descendantPossible:true,groupAliveBefore:false,signalAttempts:0}];
  const fallback=cleanupFailureDiagnostics(Error('/private/close'),prior,99);
  assert.deepEqual(safeCleanupDiagnostics(fallback),[...prior,{stage:'cleanup-loop-error',helpersRemaining:8}]);
  const reported=[{stage:'group-probe-error',helperPid:123,descendantPossible:true,probePoint:'initial',signalAttempts:0,probeError:true}];
  assert.deepEqual(cleanupFailureDiagnostics(Error('ignored'),prior,1),[...prior,{stage:'cleanup-loop-error',helpersRemaining:1}]);
  assert.deepEqual(cleanupFailureDiagnostics(Object.assign(Error('private'),{cleanupDiagnostics:reported}),prior,1),[...prior,...reported]);
});
test('input-close failure still attempts helper cleanup and retains both observations',async()=>{
  const {closeOwnedHelpers,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  let cleaned=0;
  await assert.rejects(closeOwnedHelpers(()=>{throw Error('/private/stdin');},[{pid:12},{pid:13}],async child=>{
    cleaned++; if(child.pid===12)throw Object.assign(Error('/private/helper'),{cleanupDiagnostics:[{stage:'group-signal-error',helperPid:12,descendantPossible:true,
      groupAliveBefore:true,signalAttempts:1,signalKind:'SIGTERM',signalError:true}]});
    return [{stage:'group-absent',helperPid:13,descendantPossible:true,groupAliveBefore:false,signalAttempts:0}];
  }),error=>{
    assert.equal(cleaned,2);
    assert.deepEqual(safeCleanupDiagnostics(error.cleanupDiagnostics),[
      {stage:'group-absent',helperPid:13,descendantPossible:true,groupAliveBefore:false,signalAttempts:0},
      {stage:'input-close-error',helpersRemaining:2},
      {stage:'group-signal-error',helperPid:12,descendantPossible:true,groupAliveBefore:true,signalAttempts:1,signalKind:'SIGTERM',signalError:true},
    ]);
    assert.equal(JSON.stringify(error.cleanupDiagnostics).includes('/private/'),false);return true;
  });
});
test('cleanup exception facts do not replace the saved first refusal',async()=>{
  const {cleanupHelper,safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const diagnostic={stage:'post-action-pinned-cg-window',elementPath:'2',axPidQueryError:0,axPidMatches:true,
    applicationWindowsQueryError:0,applicationWindowsReadable:true,pinnedAXWindowMember:true,
    minimizedQueryError:0,minimizedReadable:true,minimized:false,pinnedCGWindowCount:0};
  const f=fixture();f.services.next=async()=>({type:'press',element:'2'});
  f.services.window=async(mode,pid)=>mode==='discover'?{pid,windowId:45,matchedAX:true}:
    {status:'refused',code:'adapter-refused',pid,windowId:45,matchedAX:true,actionDiagnostics:diagnostic};
  f.services.close=async()=>cleanupHelper({pid:321},{alive(){throw Error('/private/probe');}});
  const result=await runOperator(f.cfg,f.services);
  assert.equal(result.firstFailure.code,'adapter-refused');
  assert.deepEqual(result.firstFailure.actionDiagnostics,safeActionDiagnostics(diagnostic));
  assert.equal(result.helperCleanup,'incomplete');
  assert.deepEqual(result.helperCleanupDiagnostics,[{stage:'group-probe-error',helperPid:321,descendantPossible:true,
    childExitObserved:false,childSignalObserved:false,
    probePoint:'initial',signalAttempts:0,probeError:true,probeErrorCode:'UNKNOWN'}]);
  assert.equal(result.cleanup,'complete');assert.equal(JSON.stringify(result).includes('/private/'),false);
});
test('modeled helper transport preserves sanitized press diagnostics in the first-failure receipt',async()=>{
  const crypto=require('node:crypto'),os=require('node:os'),path=require('node:path');
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs');
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-press-diagnostics-')),helpers=new Set();
  try{
    const reports=safeDirectory(dir,['reports']),fixtureRoot=path.join(dir,'fixtures');fs.mkdirSync(fixtureRoot);
    const fixtures=['A','B'].map(label=>{const file=path.join(fixtureRoot,label+'.json');fs.writeFileSync(file,JSON.stringify({label}));
      return{label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};});
    const helper=path.join(dir,'modeled-helper'),diagnostics={stage:'axpress-perform',elementPath:'2',role:'AXButton',
      axPressQueryError:0,axPressAdvertised:true,performActionError:-25200,privatePath:'/private/secret'};
    fs.writeFileSync(helper,'#!/usr/bin/env node\nconsole.log(JSON.stringify({status:"pinned",pid:123,windowId:45,matchedAX:true}));\n'+
      'require("node:readline").createInterface({input:process.stdin}).on("line",line=>{const r=JSON.parse(line);const out=r.type==="check"?{status:"ok",type:r.type,pid:r.pid,windowId:r.windowId,matchedAX:true}:'+
      '{status:"refused",code:"adapter-refused",pid:r.pid,windowId:r.windowId,matchedAX:true,actionDiagnostics:'+JSON.stringify(diagnostics)+'};console.log(JSON.stringify(out));});\n',{mode:0o700});
    const cfg={windowHelper:helper,grant:{id:'modeled-diagnostics'},fixtures},window=createWindowTransport(cfg,reports.directory,()=>reports,helpers);
    const f=fixture();f.cfg.fixtures=fixtures;f.services.window=window;f.services.next=async()=>({type:'press',element:'2'});
    f.services.close=async()=>{for(const child of [...helpers])await cleanupHelper(child);};
    const receipt=await runOperator(f.cfg,f.services);
    assert.equal(receipt.firstFailure.code,'adapter-refused');
    assert.deepEqual(receipt.firstFailure.actionDiagnostics,{stage:'axpress-perform',elementPath:'2',role:'AXButton',
      axPressQueryError:0,axPressAdvertised:true,performActionError:-25200});
    assert.equal(receipt.startAttempts,1);assert.equal(receipt.cleanup,'complete');assert.equal(receipt.helperCleanup,'complete');
    assert.equal(JSON.stringify(receipt).includes('/private/'),false);
  }finally{for(const child of [...helpers])await cleanupHelper(child);fs.rmSync(dir,{recursive:true,force:true});}
});
test('main inspection refuses an unpinned attached sheet BEFORE any route/attribute traversal',()=>{
  const source=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  const start=source.indexOf('    case "inspect":'),end=source.indexOf('    case "capture":',start),body=source.slice(start,end);
  const sheetStage=body.indexOf('setInspectionContext("main-sheet-state"'),guard=body.indexOf('try noSheet(inspectMain:true)'),traversal=body.indexOf('try node(base,selectedRoot)');
  assert.ok(start>=0&&end>start&&sheetStage>=0&&guard>sheetStage&&traversal>guard);
  const noSheet=source.slice(source.indexOf('  func noSheet('),source.indexOf('  func retainedMainChildrenDiagnostic()',source.indexOf('  func noSheet(')));
  assert.match(noSheet,/sheet == nil/);assert.match(noSheet,/members\(inspectMain:inspectMain\)/);
});
test('main inspect helper reports only finite query stage and class enums',()=>{
  const source=fs.readFileSync('tests/desktop/p38-owned-window.swift','utf8');
  assert.match(source,/struct InspectionRefusal: Error \{ let stage: String; let queryClass: String \}/);
  assert.match(source,/catch let error as InspectionRefusal \{/);
  for(const stage of ['identity-precondition','request-precondition','main-children-query','main-children-value',
    'application-windows-query','application-windows-value','main-sheet-state','root-resolution','projection-budget',
    'ax-owner-query','ax-attribute-query','ax-children-query','ax-projection'])
    assert.ok(source.includes(`"${stage}"`),`missing bounded inspect stage ${stage}`);
  assert.match(source,/func safeInspectionContext\(_ stage: String, _ queryClass: String\)/);
  assert.match(source,/func inspectionDiagnosticForRefusal\(_ code: String, _ stage: String, _ queryClass: String\)/);
  assert.match(source,/guard code == "adapter-refused" else \{ return nil \}/);
  assert.match(source,/if let diagnostic = safeInspectionContext\(error\.stage,error\.queryClass\)/);
  assert.match(source,/activeInspectionContext = action == "inspect"/);
  assert.match(source,/inspectionDiagnosticForRefusal\(code,context\.stage,context\.queryClass\)/);
  assert.doesNotMatch(source,/inspectionDiagnostics[^\n]*(rawValue|title|value|path|ownerPid)/);
});
test('A/B fixture proof uses exact same-descriptor bytes and rejects swap/hash/name/symlink',()=>{
  const {verifyFixtures}=require('./desktop/p38-installed-driver.cjs'),crypto=require('node:crypto'),dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-fixtures-'));
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,JSON.stringify({label}));return{label,path:file,sha256:crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex')};});
    verifyFixtures({fixtures});assert.throws(()=>verifyFixtures({fixtures:[fixtures[0]]}));
    assert.throws(()=>verifyFixtures({fixtures:[{...fixtures[0],sha256:'0'.repeat(64)},fixtures[1]]}),e=>e.code==='fixture-refused');
    fs.writeFileSync(fixtures[1].path,'changed');assert.throws(()=>verifyFixtures({fixtures}),e=>e.code==='fixture-refused');
    fs.unlinkSync(fixtures[1].path);fs.symlinkSync(fixtures[0].path,fixtures[1].path);assert.throws(()=>verifyFixtures({fixtures}),e=>e.code==='fixture-refused');
  }finally{fs.rmSync(dir,{recursive:true,force:true});}
});
test('helper cleanup observes natural exit and never signals a group',async()=>{
  const {cleanupHelper}=require('./desktop/p38-installed-driver.cjs');
  for(const mode of ['already','stdin-exit','group-settles','no-exit','descendant-live']){
    let live=mode==='group-settles'||mode==='descendant-live',signals=0;
    const child={pid:123,exitCode:['already','group-settles','descendant-live'].includes(mode)?0:null,signalCode:null,
      stdin:{destroyed:false,writableEnded:false,end(){if(mode==='stdin-exit')child.exitCode=0;}}};
    const work=cleanupHelper(child,{alive:()=>live,signal(){signals++;},delay:async()=>{if(mode==='group-settles')live=false;}});
    if(mode==='no-exit'||mode==='descendant-live')await assert.rejects(work,e=>e.code==='helper-cleanup-failed'&&
      e.cleanupDiagnostics[0].stage===(mode==='no-exit'?'group-identity-uncertain':'group-remains'));
    else {const observations=await work;assert.equal(observations[0].stage,'group-absent');
      assert.equal(observations[0].childExitObserved,true);assert.equal(observations[0].signalAttempts,0);}
    assert.equal(signals,0,mode);
  }
  await assert.rejects(cleanupHelper({pid:-1}),e=>e.code==='helper-cleanup-invalid');
});

for(const mode of ['positive','EOF','malformed','foreign','refused','oversized','timeout'])test('real Node-only modeled helper pipe '+mode+' is framed/tracked/cleaned',async()=>{
  // Disposable Node stub only. Never Nemo, macOS AX, a native window or installed evidence.
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs'),crypto=require('node:crypto');
  const dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-node-')),helpers=new Set();
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,'{}');return{label,path:file,sha256:crypto.createHash('sha256').update('{}').digest('hex')};});
    const script=path.join(dir,'modeled-helper');
    const before=mode==='EOF'?'process.exit(0);':mode==='timeout'?'':mode==='malformed'?"console.log('not-json');":
      mode==='oversized'?"console.log('x'.repeat(1048577));":mode==='refused'?"console.log(JSON.stringify({status:'refused',code:'confirm-unsupported'}));":
      "console.log(JSON.stringify({status:'pinned',pid:"+(mode==='foreign'?124:123)+",windowId:45,matchedAX:true}));";
    fs.writeFileSync(script,'#!/usr/bin/env node\n'+before+"\nrequire('node:readline').createInterface({input:process.stdin}).on('line',line=>{const r=JSON.parse(line);console.log(JSON.stringify({...r,status:'ok',matchedAX:true}));});\n",{mode:0o700});
    const guard=safeDirectory(dir,['reports']),window=createWindowTransport({windowHelper:script,grant:{id:'model-only'},fixtures},guard.directory,()=>guard,helpers,mode==='timeout'?20:10000);
    if(mode==='positive'){
      assert.equal((await window('discover',123)).windowId,45);
      assert.equal((await window('check',123,45)).type,'check');assert.equal(helpers.size,1);
      await assert.rejects(window('check',124,45),e=>e.code==='owned-window-refused');
    }else await assert.rejects(window('discover',123),e=>['helper-input-ended','helper-malformed','owned-window-refused','confirm-unsupported'].includes(e.code));
  }finally{
    for(const child of [...helpers])await cleanupHelper(child);
    fs.rmSync(dir,{recursive:true,force:true});
  }
});

for(const stalled of [false,true])test('press helper cleanup refuses a live process group after helper exit'+(stalled?' (event loop stalled past any fixed worker lifetime)':''),async()=>{
  // Disposable Node processes only. This models the Swift sampler outliving its supervisor.
  // The modeled sampler descendant stays alive until this test kills it: no assertion depends
  // on a wall-clock lifetime, so a loaded host cannot let it exit before the group is probed.
  const {createWindowTransport,cleanupHelper,safeDirectory}=require('./desktop/p38-installed-driver.cjs');
  const {once}=require('node:events'),crypto=require('node:crypto'),dir=fs.mkdtempSync(path.join(os.tmpdir(),'p38-v2-worker-group-')),
    helpers=new Set(),workerPidFile=path.join(dir,'worker.pid');
  let workerPid=0;
  try{
    const fixtures=['A','B'].map(label=>{const file=path.join(dir,label+'.json');fs.writeFileSync(file,'{}');
      return{label,path:file,sha256:crypto.createHash('sha256').update('{}').digest('hex')};});
    const script=path.join(dir,'modeled-helper');
    fs.writeFileSync(script,'#!/usr/bin/env node\nconsole.log(JSON.stringify({status:"pinned",pid:123,windowId:45,matchedAX:true}));\n'+
      'require("node:readline").createInterface({input:process.stdin}).on("line",()=>{'+
      // Safety bound only (2 min) so an orphaned worker can never outlive a crashed test run.
      'const worker=require("node:child_process").spawn(process.execPath,["-e","setTimeout(()=>process.exit(0),120000)"],{stdio:"ignore"});'+
      'require("node:fs").writeFileSync('+JSON.stringify(workerPidFile)+',String(worker.pid));process.exit(0);});\n',{mode:0o700});
    // Generous helper reply bound (the maximum): a cold node start under load must not look like a dead helper.
    const guard=safeDirectory(dir,['reports']),window=createWindowTransport({windowHelper:script,grant:{id:'worker-group-model'},fixtures},
      guard.directory,()=>guard,helpers,10000);
    assert.equal((await window('discover',123)).windowId,45);
    const helper=[...helpers][0],closed=once(helper,'close');
    assert.equal(helper.p38GroupMayContainDescendants,false);
    await assert.rejects(window('press',123,45,{element:'2'}),error=>error.code==='helper-input-ended');
    await closed;
    workerPid=Number(fs.readFileSync(workerPidFile,'utf8'));
    assert.ok(Number.isSafeInteger(workerPid)&&workerPid>0);
    // Simulate a starved host: the whole test process stalls longer than the old fixed 1.5 s worker lifetime.
    if(stalled)Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,1800);
    assert.equal(helper.p38GroupMayContainDescendants,true);
    assert.equal(helpers.has(helper),true,'helper remains tracked while its sampler descendant occupies the group');
    assert.doesNotThrow(()=>process.kill(-helper.pid,0),'the modeled sampler descendant must still occupy the helper group');
    await assert.rejects(cleanupHelper(helper,{delay:async()=>{}}),error=>error.code==='helper-cleanup-failed'&&
      error.cleanupDiagnostics?.[0]?.stage==='group-remains'&&
      error.cleanupDiagnostics[0].signalAttempts===0);
    assert.equal(helpers.size,1,'the caller owns set removal after cleanup');
  }finally{
    // Reap the modeled descendant first, then require the real cleanup to observe an empty group.
    if(workerPid>0)try{process.kill(workerPid,'SIGKILL');}catch(e){if(e.code!=='ESRCH')throw e;}
    for(const child of [...helpers])await cleanupHelper(child).catch(()=>{});
    fs.rmSync(dir,{recursive:true,force:true});
  }
});

test('text-entry diagnostics keep only categorical capabilities and readback equality',()=>{
  const {safeActionDiagnostics,safeWindowEvidence}=require('./desktop/p38-installed-driver.cjs');
  const facts={stage:'axtext-readback',elementPath:'0.7',role:'AXTextField',textFieldKind:'plain',enabled:true,
    focusSettableQueryError:-25205,focusSettable:false,focusedQueryError:0,focusedMatches:false,
    valueSettableQueryError:0,valueSettable:true,valueWriteError:0,valueReadbackError:0,valueMatches:false,
    confirmQueryError:0,confirmAdvertised:true};
  assert.deepEqual(safeActionDiagnostics({...facts,value:'/private/secret',focusedTitle:'secret',message:'secret'}),facts);
  for(const patch of [{textFieldKind:'/private/search'},{valueReadbackError:1},{valueMatches:'true'},{stage:'anything'}])
    assert.throws(()=>safeActionDiagnostics({...facts,...patch}));
  const projection=safeWindowEvidence({pid:123,windowId:45,matchedAX:true,nodes:[
    {path:'0.7',role:'AXTextField',textFieldKind:'search',value:'/private/secret'},
    {path:'1',role:'AXButton',textFieldKind:'plain'}]});
  assert.deepEqual(projection.nodes,[{path:'0.7',role:'AXTextField',textFieldKind:'search'},{path:'1',role:'AXButton'}]);
});
test('unknown group probes never become absence and expose only fixed error codes',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  for(const code of ['EPERM','EACCES','EINVAL','ENOSYS','private-error']){
    let signalled=0;
    await assert.rejects(cleanupHelper({pid:321},{alive(){throw Object.assign(Error('/private/path'),{code});},
      signal(){signalled++;},delay:async()=>{}}),e=>{
      const d=safeCleanupDiagnostics(e.cleanupDiagnostics)[0];
      assert.equal(d.stage,'group-probe-error');assert.equal(d.probePoint,'initial');
      assert.equal(d.probeErrorCode,code==='private-error'?'UNKNOWN':code);assert.equal('groupAliveBefore' in d,false);
      assert.equal(JSON.stringify(d).includes('/private/'),false);return true;
    });assert.equal(signalled,0);
  }
  for(const value of [undefined,null,0,1,'false',{},Promise.resolve(false)])
    await assert.rejects(cleanupHelper({pid:321},{alive:()=>value}),e=>e.cleanupDiagnostics[0].probeErrorCode==='ERR_INVALID_PROBE_RESULT');
  assert.throws(()=>safeCleanupDiagnostics([{stage:'group-probe-error',probeErrorCode:'ESRCH'}]));
  let signals=0;
  await assert.rejects(cleanupHelper({pid:321,p38DetachedGroup:true,exitCode:0,signalCode:null},
    {alive:()=>true,signal(){signals++;},delay:async()=>{}}),
    e=>e.cleanupDiagnostics[0].stage==='group-remains'&&e.cleanupDiagnostics[0].groupAliveBefore===true);
  assert.equal(signals,0);
});
test('Go-to-Folder diagnostics preserve bounded categories and reject impossible posted phases',()=>{
  const {safeActionDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const facts={stage:'goto-discovery',gotoPhase:'pre-tree',commandPosted:false,gotoReason:'child-owner-query',
    gotoRoute:'0.1',gotoDepth:2,gotoNodeCount:5,gotoQueryError:-25204,ownerMatches:false};
  assert.deepEqual(safeActionDiagnostics({...facts,title:'private',value:'/private/file',ownerPid:999,rawAX:{secret:true}}),facts);
  assert.deepEqual(safeActionDiagnostics({...facts,gotoRoute:'',gotoDepth:0}),{...facts,gotoRoute:'',gotoDepth:0});
  for(const phase of ['posted-pair','post-context','discovery','retained-verification'])
    assert.equal(safeActionDiagnostics({...facts,gotoPhase:phase,commandPosted:true}).commandPosted,true);
  assert.deepEqual(safeActionDiagnostics({stage:'goto-ready',gotoPhase:'complete',commandPosted:true}),
    {stage:'goto-ready',gotoPhase:'complete',commandPosted:true});
  for(const patch of [{gotoPhase:'private'},{commandPosted:true},{commandPosted:'false'},{commandPosted:undefined},
    {gotoPhase:undefined},{gotoPhase:'discovery'},{gotoReason:'/private/reason'},{gotoRoute:'/private/path'},
    {gotoRoute:'512'},{gotoRoute:Array(14).fill('0').join('.')},{gotoDepth:14},{gotoDepth:-1},
    {gotoNodeCount:513},{gotoCandidateCount:513},{gotoFieldCount:513},{gotoChildCount:514},{gotoChildCount:1.2},
    {gotoQueryError:1},{gotoQueryError:-32769},{ownerMatches:1},{parentMatches:'false'},
    {candidateFresh:0},{candidateOwned:null},{semanticMatched:'yes'},{stage:'axpress-perform'},{stage:'goto-ready'},
    {gotoPhase:'complete',commandPosted:true}])
    assert.throws(()=>safeActionDiagnostics({...facts,...patch}),/ax-evidence-refused/);
  assert.equal(safeActionDiagnostics({...facts,gotoChildCount:513}).gotoChildCount,513); // Saturated over-limit count.
});
test('cleanup child exit observations never turn a refused group probe into absence',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  for(const child of [{pid:321},{pid:321,exitCode:null,signalCode:null},{pid:321,exitCode:1,signalCode:null},
    {pid:321,exitCode:null,signalCode:'SIGTERM'}]){
    let signals=0;
    await assert.rejects(cleanupHelper(child,{alive(){throw Object.assign(Error('private'),{code:'EPERM'});},
      delay:async()=>{},signal(){signals++;}}),error=>{
      const [d]=safeCleanupDiagnostics(error.cleanupDiagnostics);
      assert.equal(d.stage,'group-probe-error');assert.equal(d.probeErrorCode,'EPERM');assert.equal(d.signalAttempts,0);
      assert.equal(d.childExitObserved,Number.isInteger(child.exitCode));assert.equal(d.childSignalObserved,typeof child.signalCode==='string');
      assert.equal('groupAliveBefore' in d,false);return true;
    });assert.equal(signals,0);
  }
  assert.throws(()=>safeCleanupDiagnostics([{stage:'group-probe-error',childExitObserved:'yes'}]));
  assert.throws(()=>safeCleanupDiagnostics([{stage:'group-probe-error',childSignalObserved:1}]));
});
test('transient EPERM settles only after exact helper exit and ESRCH',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  const child={pid:321,p38DetachedGroup:true,exitCode:null,signalCode:null,
    stdin:{destroyed:false,writableEnded:false,end(){child.exitCode=0;}}};
  let probes=0,signals=0;
  const observations=await cleanupHelper(child,{alive(){if(++probes===1)throw Object.assign(Error('private'),{code:'EPERM'});return false;},
    signal(){signals++;},delay:async()=>{}});
  assert.deepEqual(safeCleanupDiagnostics(observations),[{stage:'group-absent',helperPid:321,descendantPossible:true,
    groupAliveBefore:false,childExitObserved:true,transientProbeDenied:true,signalAttempts:0}]);
  assert.equal(probes,2);assert.equal(signals,0);
});
test('EPERM, absent exit, live descendants and stale group identity never authorize a group signal',async()=>{
  const {cleanupHelper,safeCleanupDiagnostics}=require('./desktop/p38-installed-driver.cjs');
  for(const mode of ['persistent-denial','no-exit','descendant-live','unverified-group','recycled-leader']){
    const child={pid:321,p38DetachedGroup:mode!=='unverified-group',
      exitCode:['persistent-denial','descendant-live'].includes(mode)?0:null,signalCode:null};
    let probes=0,signals=0;
    await assert.rejects(cleanupHelper(child,{alive(){probes++;if(mode==='persistent-denial'||mode==='no-exit'&&probes===1)
      throw Object.assign(Error('private'),{code:'EPERM'});return mode!=='no-exit';},
      signal(){signals++;},delay:async()=>{}}),error=>{
      const [fact]=safeCleanupDiagnostics(error.cleanupDiagnostics);
      assert.equal(fact.stage,mode==='persistent-denial'?'group-probe-error':
        mode==='no-exit'?'group-identity-uncertain':'group-remains');
      return true;
    });
    assert.equal(signals,0,mode);
  }
});
