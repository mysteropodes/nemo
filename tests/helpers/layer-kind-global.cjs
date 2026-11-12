'use strict';
// Loads the real src/js/layer-kind.js (a browser global IIFE) against a given window object and returns window.SMLayerKind.
const fs=require('node:fs'),path=require('node:path'),vm=require('node:vm');
module.exports=function(win){
  const code=fs.readFileSync(path.join(__dirname,'../../src/js/layer-kind.js'),'utf8');
  vm.runInNewContext(code,{window:win});return win.SMLayerKind;
};
