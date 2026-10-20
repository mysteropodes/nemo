/* Public v2 contract of the lane-6 object operations: raster/text item edits and
 * the embedded-image asset catalog. Pure validation: no state, no transport, no
 * document writer. Joined by the common SDK through compose(); the native owner
 * stays the only authority (strict original-byte admission repeats every rule). */
(function(root,factory){var api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.NemoNativeObjectItemContract=api;
}(typeof globalThis==='object'?globalThis:this,function(){
  'use strict';
  const ITEM='command.document.object.item.edit',ATTACH='command.document.object.asset.attach',REMOVE='command.document.object.asset.remove';
  const OPS=new Set([ITEM,ATTACH,REMOVE]);
  const FONTS=['Roboto-Regular','Roboto-Bold','DejaVuSans-Regular','DejaVuSans-Bold','DejaVuSerif-Regular','DejaVuSerif-Bold','Manrope-Regular','Inter-Regular'];
  const MEDIA=['image/png','image/jpeg','image/gif','image/webp','image/bmp','image/svg+xml','image/avif'];
  const ALIGN=['left','center','right'],CASES=['none','upper','lower','capitalize'];
  const RASTER=['create','move','resize','rotate','opacity','delete'].map(v=>'raster.'+v),TEXT=['create','edit','delete'].map(v=>'text.'+v);
  const BOOLS=['bold','italic','underline','strike'],NUMS=['letterSpacing','wordSpacing','lineHeightMult'];
  const TEXT_OPTIONAL=['fixedWidth','fixedHeight'].concat(BOOLS,NUMS,['textCase']);
  const ITEM_OPTIONAL=['opacity','rotation','isBitmapBrush','brushGroupId','isBrushTextureCopy','meshId'];
  function fail(message){throw new TypeError(message);}
  function plain(v){return !!v&&typeof v==='object'&&!Array.isArray(v);}
  function exact(v,required,optional,label){
    if(!plain(v))fail(label+' must be an object');
    const allowed=new Set(required.concat(optional||[]));
    if(!required.every(k=>Object.prototype.hasOwnProperty.call(v,k))||!Object.keys(v).every(k=>allowed.has(k)))fail(label+' fields invalid');
  }
  function int(v,min,max){if(!Number.isSafeInteger(v)||v<min||v>(max===undefined?Number.MAX_SAFE_INTEGER:max))fail('integer invalid');}
  function num(v){if(typeof v!=='number'||!Number.isFinite(v))fail('number invalid');}
  function bool(v){if(typeof v!=='boolean')fail('boolean invalid');}
  function id(v){if(typeof v!=='string'||!/^[A-Za-z0-9][A-Za-z0-9._:/-]{0,127}$/.test(v))fail('identifier invalid');}
  function bytes(v){return new TextEncoder().encode(v).length;}
  // Opaque authored strings: non-blank (the native and font-contract rule), no NUL, bounded UTF-8 length. A host file path (raw) is
  // only non-empty: the native validator does not trim it.
  function opaque(v,max,raw){if(typeof v!=='string'||!v.length||(!raw&&!v.trim())||v.includes('\0')||bytes(v)>(max||255))fail('opaque string invalid');}
  function oneOf(v,list){if(!list.includes(v))fail('enumerated value invalid');}
  function frameScope(v){exact(v,['kind','frame'],[],'frameScope');oneOf(v.kind,['authored','reference']);int(v.frame,0,4294967295);}
  function target(v,create){
    exact(v,['contextId','frameScope','layerUid','strokeId'],[],'target');
    if(v.contextId!=='scene-root')fail('contextId invalid');frameScope(v.frameScope);opaque(v.layerUid);
    if(create){if(v.strokeId!=='')fail('a created item names no stroke id; the owner allocates it');}else opaque(v.strokeId);
  }
  function rasterItem(v){
    exact(v,['assetId','x','y','width','height'],ITEM_OPTIONAL,'raster item');opaque(v.assetId);['x','y','width','height'].forEach(k=>num(v[k]));
    if('opacity' in v)num(v.opacity);if('rotation' in v)num(v.rotation);
    ['isBitmapBrush','isBrushTextureCopy'].forEach(k=>{if(k in v)bool(v[k]);});['brushGroupId','meshId'].forEach(k=>{if(k in v)opaque(v[k]);});
  }
  function font(v){
    if(plain(v)&&v.kind==='bundled'){exact(v,['kind','fontKey'],[],'font');oneOf(v.fontKey,FONTS);return;}
    exact(v,['kind','assetId'],[],'font');if(v.kind!=='asset')fail('font kind invalid');opaque(v.assetId);
  }
  function textBody(v,block){
    if('text' in v){if(typeof v.text!=='string'||!v.text.length||v.text.includes('\0'))fail('text invalid');}
    if('font' in v)font(v.font);
    if('size' in v)num(v.size);
    if('color' in v&&(typeof v.color!=='string'||!/^#(?:[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/.test(v.color)))fail('color invalid');
    if('align' in v)oneOf(v.align,ALIGN);
    ['fixedWidth','fixedHeight'].forEach(k=>{if(k in v&&v[k]!==null)num(v[k]);});
    BOOLS.forEach(k=>{if(k in v)bool(v[k]);});NUMS.forEach(k=>{if(k in v)num(v[k]);});
    if('textCase' in v)oneOf(v.textCase,CASES);
    if(block){exact(v.anchorTopLeft,['x','y'],[],'anchorTopLeft');num(v.anchorTopLeft.x);num(v.anchorTopLeft.y);}
  }
  function textBlock(v){exact(v,['text','font','size','color','align','anchorTopLeft'],TEXT_OPTIONAL,'text block');textBody(v,true);}
  function textPatch(v){
    // Same members as a block minus the anchor: a patch can never re-anchor (baseline feedback #37).
    const members=['text','font','size','color','align'].concat(TEXT_OPTIONAL);
    if(!plain(v)||!Object.keys(v).length||!Object.keys(v).every(k=>members.includes(k)))fail('text patch invalid');textBody(v,false);
  }
  const SELECTOR_KEYS=['start','end','offset','amount','easeHigh','easeLow','smooth','wiggleMin','wiggleMax','wigglesPerSec','correlation'];
  function animator(v){
    exact(v,['id','selector'],['enabled','props','properties'],'text animator');opaque(v.id);
    if('enabled' in v)bool(v.enabled);
    exact(v.selector,[],SELECTOR_KEYS.concat(['selectorType','units','basedOn','shape','seed']),'animator selector');
    Object.keys(v.selector).forEach(k=>{var n=v.selector[k];
      if(k==='selectorType')oneOf(n,['range','wiggly']);else if(k==='units')oneOf(n,['percent','index']);else if(k==='basedOn')oneOf(n,['chars','words','lines']);else if(n!==null)num(n);
    });
    if('props' in v){exact(v.props,[],['position','scale','rotation','opacity','fillColor','strokeColor'],'animator properties');
      Object.keys(v.props).forEach(k=>{var n=v.props[k];if(k==='rotation'||k==='opacity'){if(n!==null)num(n);return;}
        if(!Array.isArray(n)||(['position','scale'].includes(k)?n.length!==2:![3,4].includes(n.length)))fail('animator vector invalid');
        n.forEach((x,i)=>{if(x===null){if(['fillColor','strokeColor'].includes(k)&&i<3)fail('animator color invalid');}else num(x);});});
    }
    if('properties' in v){if(!Array.isArray(v.properties)||v.properties.length>11)fail('selector tracks invalid');var names=new Set();
      v.properties.forEach(p=>{exact(p,['property'],['static','track','expression'],'selector property');oneOf(p.property,SELECTOR_KEYS);if(names.has(p.property))fail('duplicate selector property');names.add(p.property);
        if('static' in p){if(!Array.isArray(p.static)||p.static.length!==1)fail('selector static invalid');num(p.static[0]);}
        if('expression' in p){exact(p.expression,['enabled','code'],[],'selector expression');bool(p.expression.enabled);if(typeof p.expression.code!=='string'||bytes(p.expression.code)>4096)fail('selector expression invalid');}
        if('track' in p){exact(p.track,['keys'],[],'selector track');if(!Array.isArray(p.track.keys))fail('selector keys invalid');var previous=-Infinity;
          p.track.keys.forEach(k=>{exact(k,['frame','v'],['hold','lockTo','curvePoints','hIn','hOut'],'selector key');num(k.frame);if(k.frame<=previous)fail('selector keys unordered');previous=k.frame;
            if(!Array.isArray(k.v)||k.v.length!==1)fail('selector key vector invalid');num(k.v[0]);if('hold' in k)bool(k.hold);if('lockTo' in k&&k.lockTo!==null)oneOf(k.lockTo,['in','out','layer']);
            ['hIn','hOut'].forEach(n=>{if(n in k){if(!Array.isArray(k[n])||k[n].length!==2)fail('selector handle invalid');k[n].forEach(num);}});
            if('curvePoints' in k&&k.curvePoints!==null){if(!Array.isArray(k.curvePoints))fail('selector curve invalid');var x=-Infinity;
              k.curvePoints.forEach(c=>{exact(c,['x','y'],['tx','ty'],'selector curve point');Object.values(c).forEach(num);if(c.x<=x)fail('selector curve unordered');x=c.x;});
              if(k.curvePoints.length>=2&&(k.curvePoints[0].x!==0||x!==1))fail('selector curve endpoints invalid');}
          });}
      });}
  }
  const SHAPES={
    'object.raster.create':['target','item'],'object.raster.move':['stableTarget','x','y'],'object.raster.resize':['stableTarget','width','height'],
    'object.raster.rotate':['stableTarget','rotation'],'object.raster.opacity':['stableTarget','opacity'],'object.raster.delete':['stableTarget'],
    'object.text.create':['target','block'],'object.text.edit':['stableTarget','patch'],'object.text.delete':['stableTarget'],
    'object.text.animator.upsert':['layerUid','animator'],'object.text.animator.remove':['layerUid','animatorId'],
  };
  function itemPayload(p){
    if(!plain(p)||!Object.prototype.hasOwnProperty.call(SHAPES,p.command))fail('item command invalid');
    exact(p,['command'].concat(SHAPES[p.command]),[],'item command');
    if('layerUid' in p)opaque(p.layerUid);if('animator' in p)animator(p.animator);if('animatorId' in p)opaque(p.animatorId);
    if('target' in p)target(p.target,true);if('stableTarget' in p)target(p.stableTarget,false);
    if('item' in p)rasterItem(p.item);if('block' in p)textBlock(p.block);if('patch' in p)textPatch(p.patch);
    ['x','y','width','height','rotation','opacity'].forEach(k=>{if(k in p)num(p[k]);});
  }
  function imagePlacement(v){
    if(v&&v.kind==='newLayer'){exact(v,['kind'],[],'image placement');return;}
    exact(v,['kind','layerUid','frame'],[],'image placement');
    if(v.kind!=='authoredFrame')fail('image placement kind invalid');
    opaque(v.layerUid);int(v.frame,0,4294967295);
  }
  function imageResult(v){
    exact(v,['applied','historyEntriesAdded','assetId','naturalWidth','naturalHeight','displayWidth','displayHeight','target','newLayer'],[],'image result');
    if(v.applied!==true||v.historyEntriesAdded!==1)fail('image placement must apply once');
    opaque(v.assetId);int(v.naturalWidth,1,8192);int(v.naturalHeight,1,8192);
    num(v.displayWidth);num(v.displayHeight);
    if(v.displayWidth<=0||v.displayWidth>v.naturalWidth||v.displayHeight<=0||v.displayHeight>v.naturalHeight)fail('image fit invalid');
    target(v.target,false);if(v.target.frameScope.kind!=='authored')fail('image scope invalid');bool(v.newLayer);
  }
  function payload(op,p){
    if(op===ITEM)return itemPayload(p);
    if(op===ATTACH){
      const placed=plain(p)&&Object.prototype.hasOwnProperty.call(p,'placement');
      exact(p,['documentSnapshotId','name','mediaType','source'].concat(placed?['placement','lifecycleGeneration']:[]),[],'asset attach');
      id(p.documentSnapshotId);opaque(p.name);oneOf(p.mediaType,MEDIA);
      if(placed){imagePlacement(p.placement);int(p.lifecycleGeneration,1);if(p.mediaType!=='image/png')fail('native image placement supports PNG only');}
      exact(p.source,['kind','path'],[],'asset source');if(p.source.kind!=='file')fail('asset source kind invalid');opaque(p.source.path,4096,true);return;
    }
    exact(p,['documentSnapshotId','assetId'],[],'asset remove');id(p.documentSnapshotId);opaque(p.assetId);
  }
  function result(op,v){
    if(op===ATTACH&&plain(v)&&Object.prototype.hasOwnProperty.call(v,'assetId'))return imageResult(v);
    exact(v,op===ITEM?['applied','historyEntriesAdded','operation','strokeId']:['applied','historyEntriesAdded'],[],'result');
    bool(v.applied);if(v.historyEntriesAdded!==(v.applied?1:0))fail('history disposition invalid');
    if(op===ITEM){oneOf(v.operation,RASTER.concat(TEXT,['text.animator.upsert','text.animator.remove']));opaque(v.strokeId);}
  }
  function correlate(q,r){
    if(r.requestId!==q.requestId)fail('response request mismatch');
    const placed=q.operation===ATTACH&&plain(q.payload)&&q.payload.mediaType==='image/png'&&plain(q.payload.placement)&&
      ['newLayer','authoredFrame'].includes(q.payload.placement.kind);
    const refusal=placed&&r.ok===false&&r.result===undefined&&plain(r.error);
    const ownerRefusal=refusal&&(r.error.code==='wrong_instance'&&r.instanceId!==q.instanceId||r.error.code==='wrong_document'&&r.instanceId===q.instanceId);
    if((r.instanceId!==q.instanceId||r.documentId!==q.documentId)&&!ownerRefusal)fail('response owner mismatch');if(!r.ok)return;
    if(r.contentRevision!==q.expectedRevision+(r.result.applied?1:0))fail('receipt revision mismatch');
    if(q.operation===ATTACH){
      const placed=Object.prototype.hasOwnProperty.call(q.payload,'placement');
      if(!placed){if(Object.prototype.hasOwnProperty.call(r.result,'assetId'))fail('catalog receipt cannot carry a placement');return;}
      imageResult(r.result);
      const placement=q.payload.placement,t=r.result.target,next=q.expectedRevision+1;
      if(r.result.newLayer!==(placement.kind==='newLayer')||!t.strokeId.startsWith('native-raster:'+next+':'))fail('image target disposition mismatch');
      if(placement.kind==='newLayer'){
        if(!t.layerUid.startsWith('native-layer:'+next+':')||t.frameScope.frame!==0)fail('image layer target mismatch');
      }else if(t.layerUid!==placement.layerUid||t.frameScope.frame!==placement.frame)fail('image authored target mismatch');
      return;
    }
    if(q.operation!==ITEM)return;
    const p=q.payload,created=p.command.endsWith('.create');
    if(p.command.startsWith('object.text.animator.')){if(r.result.operation!==p.command.slice(7)||r.result.strokeId!==(p.animator?p.animator.id:p.animatorId))fail('animator receipt mismatch');return;}
    if(r.result.operation!==p.command.slice('object.'.length))fail('item receipt operation mismatch');
    const prefix=p.command.startsWith('object.raster.')?'native-raster:':'native-text:';
    // A created item reports the id the owner allocated; every other edit reports the stable target it named.
    if(created?!r.result.strokeId.startsWith(prefix):r.result.strokeId!==p.stableTarget.strokeId)fail('item receipt stroke mismatch');
  }
  function paths(op){
    if(op===ITEM)return new Set(['payload.layerUid','payload.animatorId','payload.animator.id','payload.target.layerUid','payload.stableTarget.layerUid','payload.target.strokeId','payload.stableTarget.strokeId','payload.item.assetId',
      'payload.item.brushGroupId','payload.item.meshId','payload.block.text','payload.block.font.assetId','payload.patch.text','payload.patch.font.assetId','result.strokeId']);
    return op===ATTACH?new Set(['payload.name','payload.source.path','payload.placement.layerUid','result.assetId','result.target.layerUid','result.target.strokeId']):new Set(['payload.assetId']);
  }
  const own=Object.freeze({admitted:op=>OPS.has(op),requiresRevision:op=>OPS.has(op),payload:payload,result:result,correlate:correlate,paths:paths});
  // The common SDK consumes one contract object; this composes it in front of the font contract without a second SDK seam.
  function compose(base){
    if(!base)return own;
    return Object.freeze({admitted:op=>own.admitted(op)||base.admitted(op),requiresRevision:op=>own.admitted(op)?true:base.requiresRevision(op),
      payload:(op,p,e)=>own.admitted(op)?own.payload(op,p):base.payload(op,p,e),result:(op,v)=>own.admitted(op)?own.result(op,v):base.result(op,v),
      correlate:(q,r)=>own.admitted(q.operation)?own.correlate(q,r):base.correlate(q,r),paths:op=>own.admitted(op)?own.paths(op):base.paths(op)});
  }
  return Object.freeze(Object.assign({compose:compose,ITEM:ITEM,ATTACH:ATTACH,REMOVE:REMOVE,FONTS:FONTS,MEDIA:MEDIA,animator:animator},own));
}));
