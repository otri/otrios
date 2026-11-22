<!-- SPDX-License-Identifier: Apache-2.0 -->
# Relay model assets

`relay-hat.glb` is derived from the user-supplied `hat_v18.glb` (original GLB
copyright: **Steampunk Digital. Co. LTD**). The asset retains that attribution;
the Apache license on tooling and this document does not relicense the model.
The source SHA-256 and resulting size/digest are in `relay-hat.glb.json`.

The 249,264-byte static GLB includes one 512 × 512 JPEG base-colour texture,
5,277 vertices and 8,989 triangles. Original ambient occlusion is multiplied
into the colour texture in linear light. Geometry is simplified at a 0.18
target ratio with a 0.002 relative error limit. Unused normal and ORM textures
are removed; the runtime lights the retained vertex normals. Node transforms
are baked, and the single material remains double-sided. No runtime meshopt or
AVIF extension is needed for this native-friendly asset.

Reproduce using the existing Monocle `monocle3d` dependencies (glTF Transform
4.4.1, meshoptimizer 1.2.0 and sharp) without editing that workspace:

```sh
node scripts/process-relay-model.mjs /path/to/hat_v18.glb \
  assets/models/relay-hat.glb /path/to/monocle-v3-server/monocle3d 0.18
```

The recovered progressive processor is
`monocle3d/scripts/process-progressive-glb.mjs`; its corresponding viewer is
`monocle3d/components/gltf/ProgressiveGlbViewer.client.vue`, used by
`monocle3d/components/hello/AaronExperience.client.vue`. With 512-pixel AVIF
textures (colour quality 55, data quality 65) and original geometry, that
pipeline produced 307,556 bytes. The initial shared native renderer uses JPEG
to avoid a new platform AV1 decoder and meshopt runtime dependency. Progressive
range fetching and AVIF are not requirements of the static model API.
