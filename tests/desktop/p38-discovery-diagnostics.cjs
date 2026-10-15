'use strict';
// Initial owned-window refusal facts only; never expose native strings or identities.
const SCHEMA='nemo.p38-discovery/1';
const BOOLEAN_FIELDS=Object.freeze({arguments:'valid',pid:'available','process-start':'available',
  'cg-window-id':'available','report-path':'valid','report-kind':'valid',
  'cg-second-query':'available','cg-bounds':'valid','ax-trust':'trusted'});
const STAGES=Object.freeze([...Object.keys(BOOLEAN_FIELDS),'scope','report-attributes','cg-first','cg-second-count','ax-match']);
function refused() {throw Object.assign(Error('helper-malformed'),{code:'helper-malformed'});}
function safeDiscoveryDiagnostics(value) {
  if(value===undefined)return undefined; // Older helpers still refuse without diagnostic credit.
  if(!value||typeof value!=='object'||Array.isArray(value))refused();
  const descriptors=Object.getOwnPropertyDescriptors(value);
  if(Reflect.ownKeys(descriptors).some(k=>typeof k!=='string'||!('value' in descriptors[k])))refused();
  const plain=Object.fromEntries(Object.entries(descriptors).map(([k,d])=>[k,d.value]));
  if(plain.schema!==SCHEMA||!STAGES.includes(plain.stage))refused();
  const {stage}=plain,boolean=BOOLEAN_FIELDS[stage];
  const fields=boolean?[boolean]:stage==='cg-first'?['available','countBucket']:
    stage==='cg-second-count'?['countBucket']:stage==='ax-match'?['queryClass','valueClass','candidates','matches']:[];
  if(Object.keys(plain).sort().join('\0')!==['schema','stage',...fields].sort().join('\0'))refused();
  if(boolean&&plain[boolean]!==false)refused();
  if(stage==='cg-first'&&(typeof plain.available!=='boolean'||!Number.isInteger(plain.countBucket)||
    ![0,2].includes(plain.countBucket)||!plain.available&&plain.countBucket!==0))refused();
  if(stage==='cg-second-count'&&(!Number.isInteger(plain.countBucket)||![0,2].includes(plain.countBucket)))refused();
  if(stage==='ax-match') {
    if(!['success','unsupported','no-value','cannot-complete','other-error'].includes(plain.queryClass)||
      !['array','missing','wrong-type'].includes(plain.valueClass)||
      !Number.isInteger(plain.candidates)||plain.candidates<0||plain.candidates>2||
      !Number.isInteger(plain.matches)||![0,2].includes(plain.matches)||plain.matches>plain.candidates)refused();
    if(plain.queryClass!=='success'&&plain.valueClass!=='missing'||
      plain.valueClass!=='array'&&(plain.candidates!==0||plain.matches!==0))refused();
  }
  return Object.freeze(plain);
}
module.exports={SCHEMA,STAGES,safeDiscoveryDiagnostics};
