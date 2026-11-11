// SPDX-License-Identifier: Apache-2.0
// Host pixels map to the logical extent published with this exact scene.
export function layoutViewport(width,height,policy={kind:'fit'}){
  if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw Error('Invalid available viewport');
  if(policy?.kind==='fit')return width>=height?[1280,800]:[800,1280];
  if(policy?.kind!=='responsive'||!Array.isArray(policy.minimum)||policy.minimum.length!==2||policy.minimum.some(n=>!Number.isFinite(n)||n<1||n>8192))throw Error('Invalid signed viewport policy');
  const minimumScale=Math.max(policy.minimum[0]/width,policy.minimum[1]/height),maximumScale=8192/Math.max(width,height);
  if(minimumScale>maximumScale)throw Error('Available aspect ratio exceeds responsive viewport bounds');
  const scale=Math.min(maximumScale,Math.max(1,minimumScale));
  return [Math.min(8192,width*scale),Math.min(8192,height*scale)];
}
// A new mounted/presented policy must be applied even when its size matches the
// last screen. Otherwise a fixed screen can leave a responsive mount unmeasured.
export function viewportRequestNeeded(previous,next,previousContext,nextContext){
  return previousContext!==nextContext||next.some((n,i)=>n!==previous[i]);
}
// Chat reflows vertically into the visual viewport. Keyboard height must never
// select a landscape design or reduce the horizontal UI scale.
export function chatViewport(width,height){
  layoutViewport(width,height); // Validate the available CSS dimensions.
  // Phones need a readable density: 800 / 534 is approximately 1.5x.
  const logicalWidth=width<600?534:width>=1100?1280:800;
  return [logicalWidth,Math.max(300,Math.min(8192,Math.floor(logicalWidth*height/width)))];
}
export function pointerPosition(x,y,bounds,extent){
  return [(x-bounds.left)*extent[0]/bounds.width,(y-bounds.top)*extent[1]/bounds.height];
}
export function semanticBounds([x,y,width,height],[logicalWidth,logicalHeight]){
  return {left:`${100*x/logicalWidth}%`,top:`${100*y/logicalHeight}%`,width:`${100*width/logicalWidth}%`,height:`${100*height/logicalHeight}%`};
}
