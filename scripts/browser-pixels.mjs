// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import {inflateSync} from 'node:zlib';
export function pngPixels(bytes){
  let width,height,channels;const parts=[];
  for(let at=8;at<bytes.length;){const length=bytes.readUInt32BE(at),kind=bytes.toString('ascii',at+4,at+8),data=bytes.subarray(at+8,at+8+length);at+=length+12;
    if(kind==='IHDR'){width=data.readUInt32BE(0);height=data.readUInt32BE(4);assert.equal(data[8],8);channels={2:3,6:4}[data[9]];assert(channels);assert.equal(data[12],0);}
    if(kind==='IDAT')parts.push(data);
  }
  const packed=inflateSync(Buffer.concat(parts)),stride=width*channels,pixels=Buffer.alloc(height*stride);
  const paeth=(a,b,c)=>{const p=a+b-c,da=Math.abs(p-a),db=Math.abs(p-b),dc=Math.abs(p-c);return da<=db&&da<=dc?a:db<=dc?b:c;};
  for(let y=0,at=0;y<height;y++){const filter=packed[at++];assert(filter<=4);for(let x=0;x<stride;x++){const i=y*stride+x,a=x>=channels?pixels[i-channels]:0,b=y?pixels[i-stride]:0,c=y&&x>=channels?pixels[i-stride-channels]:0;pixels[i]=(packed[at++]+[0,a,b,Math.floor((a+b)/2),paeth(a,b,c)][filter])&255;}}
  return {width,height,channels,pixels};
}
