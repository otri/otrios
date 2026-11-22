<!-- SPDX-License-Identifier: Apache-2.0 -->
# Static model rendering

The shared Canvas model path separates validated immutable model resources,
instance transforms and camera projection from application animation. The
Brass Relay hat is a client of that API; the loader and GPU shader contain no
hat geometry or choreography. The same GLSL ES 1.00 source is used by WebGL1
and the native GLES2 build.

`Model::from_glb` admits a bounded static glTF 2 binary scene. Multiple meshes,
triangle primitives, materials, embedded JPEG base-colour textures and node
matrix/TRS hierarchies are supported. Node transforms are applied once; decoded
textures are shared. `Instance` supplies model, pose and opacity; `Camera` and
`Frame::project` produce draw batches. GPU hosts retain textures by content hash,
use perspective-correct UV interpolation and depth testing, and apply shared
directional lighting to interpolated normals. The initial material path uses
base colour and a fixed highlight; it is not full metallic/roughness PBR. Scalar
roughness and metallic factors do not drive lighting yet. Texture colour is
sRGB; glTF base-colour factors are linear. OPAQUE material alpha is ignored,
while instance opacity supplies the effect fade.

The initial subset uses float32 positions, normals and UVs, with u16 triangle
indices. Limits are 1 MiB per GLB, 12,000 triangles per loaded/projected scene,
16 primitive batches, four textures and 1024 pixels per texture edge. Resource
loading rejects external URIs, unrecognized extensions, malformed buffer ranges,
invalid numeric values and cyclic/excessive node graphs. Skins, morph targets,
animation clips and additional PBR texture slots are outside this subset.
These are explicit admission errors, not silently ignored approximations.

The chat adapter caches the included hat resource once, normalizes its bounds,
and supplies a short affirmative tip. Playback remains local, follows the recorded
event, respects reduced motion, and is suppressed by enclosing modal barriers.
There is no model data or per-frame animation in the replicated chat protocol.
The signed client binaries include the asset, so this first path requires neither
an unsigned model fetch nor a new server-side resource authority.

The compact asset and reproduction steps are documented in
[assets/models](../assets/models/README.md). The recovered Monocle progressive
loader remains a useful later source for range-prioritized streaming, meshopt
and AVIF. Those formats are separate decoder capabilities; they do not change the
resource/instance/camera API.
