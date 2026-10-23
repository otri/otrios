// SPDX-License-Identifier: Apache-2.0
// Host pixels map to the logical extent published with this exact scene.
export function layoutViewport(width,height){
  if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw Error('Invalid available viewport');
  return width>=height?[1280,800]:[800,1280];
}
export function pointerPosition(x,y,bounds,extent){
  return [(x-bounds.left)*extent[0]/bounds.width,(y-bounds.top)*extent[1]/bounds.height];
}
export function semanticBounds([x,y,width,height],[logicalWidth,logicalHeight]){
  return {left:`${100*x/logicalWidth}%`,top:`${100*y/logicalHeight}%`,width:`${100*width/logicalWidth}%`,height:`${100*height/logicalHeight}%`};
}
