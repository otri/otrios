// SPDX-License-Identifier: Apache-2.0
// Uses the installed Monocle toolchain; never modifies its source workspace.
import {createRequire} from 'node:module';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import {dirname,resolve} from 'node:path';
import {createHash} from 'node:crypto';

const [input,output,toolchain,ratioText='0.18']=process.argv.slice(2);
if(!input||!output||!toolchain)throw Error('Usage: node scripts/process-relay-model.mjs input.glb output.glb /path/to/monocle3d [mesh-ratio]');
if(resolve(input)===resolve(output))throw Error('Input must remain unchanged');
const ratio=Number(ratioText);
if(!Number.isFinite(ratio)||ratio<=0||ratio>1)throw Error('Invalid mesh ratio');
const require=createRequire(resolve(toolchain,'package.json'));
const {NodeIO,VertexLayout}=require('@gltf-transform/core');
const {flatten,transformMesh,join,weld,simplify,prune}=require('@gltf-transform/functions');
const {MeshoptSimplifier}=require('meshoptimizer');
const sharp=require('sharp');
const io=new NodeIO().setVertexLayout(VertexLayout.SEPARATE);
const source=await readFile(input),doc=await io.readBinary(source);
if(doc.getRoot().listAnimations().length||doc.getRoot().listSkins().length)throw Error('Expected static model');
await doc.transform(flatten());
for(const node of doc.getRoot().listNodes()){
  const mesh=node.getMesh();if(!mesh)continue;
  transformMesh(mesh,node.getWorldMatrix());
  node.setMatrix([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]);
}
// Bake original AO into base colour; runtime directional lighting uses normals.
// Normal and ORM maps are not shipped as unused bytes in this initial subset.
for(const material of doc.getRoot().listMaterials()){
  const color=material.getBaseColorTexture(),ao=material.getOcclusionTexture();
  if(!color)throw Error('Expected a base-colour texture');
  const size=512;
  const pixels=await sharp(color.getImage()).resize(size,size,{fit:'fill'}).removeAlpha().raw().toBuffer();
  if(ao){
    const occlusion=await sharp(ao.getImage()).resize(size,size,{fit:'fill'}).removeAlpha().raw().toBuffer();
    // Multiply in linear light, preserving the original texture's sRGB encoding.
    const decode=v=>v<=0.04045?v/12.92:((v+0.055)/1.055)**2.4;
    const encode=v=>v<=0.0031308?v*12.92:1.055*v**(1/2.4)-0.055;
    for(let i=0;i<pixels.length;i++)pixels[i]=Math.round(255*encode(decode(pixels[i]/255)*(0.35+0.65*occlusion[i-i%3]/255)));
  }
  color.setImage(await sharp(pixels,{raw:{width:size,height:size,channels:3}}).jpeg({quality:84,chromaSubsampling:'4:4:4',mozjpeg:true}).toBuffer()).setMimeType('image/jpeg').setName('Relay hat base colour with baked AO');
  material.setNormalTexture(null).setOcclusionTexture(null).setMetallicRoughnessTexture(null).setMetallicFactor(0).setRoughnessFactor(0.85);
}
await MeshoptSimplifier.ready;
await doc.transform(weld(),simplify({simplifier:MeshoptSimplifier,ratio,error:0.002}),join({keepNamed:false}),prune());
const bytes=await io.writeBinary(doc);
if(bytes.length>=300000)throw Error(`Model budget exceeded: ${bytes.length} bytes (try a smaller ratio)`);
await mkdir(dirname(resolve(output)),{recursive:true});await writeFile(output,bytes);
const report={inputBytes:source.length,outputBytes:bytes.length,sourceSha256:createHash('sha256').update(source).digest('hex'),outputSha256:createHash('sha256').update(bytes).digest('hex'),ratio,textureSize:512,textureFormat:'JPEG',textureQuality:84,occlusionBaked:true,meshes:doc.getRoot().listMeshes().length,primitives:doc.getRoot().listMeshes().reduce((n,m)=>n+m.listPrimitives().length,0),triangles:doc.getRoot().listMeshes().reduce((n,m)=>n+m.listPrimitives().reduce((n,p)=>n+p.getIndices().getCount()/3,0),0),vertices:doc.getRoot().listMeshes().reduce((n,m)=>n+m.listPrimitives().reduce((n,p)=>n+p.getAttribute('POSITION').getCount(),0),0)};
await writeFile(output+'.json',JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report,null,2));
