'use strict';
// Controlled DOM compositing regression, not AppKit/GPU or R24 acceptance.
const {test,expect}=require('@playwright/test');
const fs=require('node:fs'),path=require('node:path');
const root=path.resolve(__dirname,'../..');
async function pixel(page,x,y){
 const bytes=await page.screenshot();
 return page.evaluate(async({bytes,x,y})=>{const image=await createImageBitmap(new Blob([new Uint8Array(bytes)],{type:'image/png'}));const c=new OffscreenCanvas(image.width,image.height),ctx=c.getContext('2d');ctx.drawImage(image,0,0);const rgba=Array.from(ctx.getImageData(x,y,1,1).data);image.close();return rgba;},{bytes:Array.from(bytes),x,y});
}
test('correlated native presentation clears only the Paper backdrop and preserves Paper and DOM overlays',async({page})=>{
 await page.setViewportSize({width:500,height:350});
 await page.setContent(`<style>:root{--gutter:#202020}html,body{margin:0;background:#202020}#native-reference{position:absolute;left:40px;top:40px;width:320px;height:180px;background:rgb(20,150,80);z-index:0}#app{position:absolute;inset:0;z-index:1;background:#202020}#canvas-area{position:absolute;inset:0;background:#202020}#drawing-canvas{position:absolute;left:40px;top:40px;width:320px;height:180px}#overlay{all:unset;position:absolute;left:60px;top:60px;width:32px;height:32px;background:rgb(220,30,40);z-index:2}</style><div id="native-reference"></div><div id="app"><div id="canvas-area"><canvas id="drawing-canvas" width="320" height="180"></canvas><button id="overlay" aria-label="Native overlay control"></button></div></div>`);
 await page.addStyleTag({content:fs.readFileSync(path.join(root,'src/css/native-viewport-underlay.css'),'utf8')});
 await page.addScriptTag({content:fs.readFileSync(path.join(root,'src/paper-full.min.js'),'utf8')});
 await page.addScriptTag({content:fs.readFileSync(path.join(root,'src/js/adapters/native-viewport-underlay.js'),'utf8')});
 await page.evaluate(()=>{const c=document.getElementById('drawing-canvas');paper.setup(c);window.stageLayer=new paper.Layer({name:'stage'});new paper.Path.Rectangle({point:[0,0],size:[320,180],fillColor:'#ffffff'});window.guideLayer=new paper.Layer({name:'ui-guide'});new paper.Path.Rectangle({point:[70,70],size:[16,16],fillColor:'#0a1ec8'});paper.view.update();window.canvasHits=0;window.overlayHits=0;c.addEventListener('pointerdown',()=>window.canvasHits++);document.getElementById('overlay').onclick=()=>window.overlayHits++;window.pin={instanceId:'owned-instance',documentId:'owned-document',lifecycleGeneration:1};window.desktop=NemoNativeViewportUnderlay.desktop(window);window.bounds={x:40,y:40,width:320,height:180};});
 expect(await pixel(page,300,150),'ordinary Paper bitmap stays visible before native presentation').toEqual([255,255,255,255]);
 await page.evaluate(()=>desktop.surfaceChanged(pin,bounds));
 expect(await pixel(page,300,150),'opaque Paper bitmap must not mask the native plane after correlated presentation').toEqual([20,150,80,255]);
 expect(await pixel(page,112,112),'justified Paper UI guide must remain visible over native pixels').toEqual([10,30,200,255]);
 expect(await pixel(page,65,65),'ordinary DOM overlay must remain above native pixels').toEqual([220,30,40,255]);
 await page.mouse.click(300,150);expect(await page.evaluate(()=>canvasHits),'invisible bitmap retains the existing native pointer entry').toBe(1);
 await page.getByRole('button',{name:'Native overlay control'}).click();expect(await page.evaluate(()=>overlayHits)).toBe(1);
 await page.evaluate(()=>desktop.surfaceChanged(null));expect(await pixel(page,300,150),'clearing the confirmed owner restores ordinary Paper pixels').toEqual([255,255,255,255]);
 expect(await pixel(page,112,112),'Paper guide survives owner cleanup').toEqual([10,30,200,255]);
 expect(await page.evaluate(()=>stageLayer.visible)).toBe(true);
});
