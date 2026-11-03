// Turning the scene's meshes into the voxel GI's voxels (gi.slang): each
// grid of the clipmap draws the meshes inside it three times, flat along
// each axis, so every surface faces at least one of the three views. Each
// fragment writes the colour, normal and glow of the surface into its
// voxel; the lighting pass (gi_inject.slang) then lights them.
//
// push: level the grid, list the axis drawn along (0 X, 1 Y, 2 Z),
// textures.xyz the albedo, normal and emissive voxels as storage images.

#include "gi.slang"

struct VoxelOut
{
    float4 position : SV_Position;
    float3 worldPosition : POSITION;
    float3 normal : NORMAL;
    float2 uv : TEXCOORD0;
    nointerpolation uint material : MATERIAL;
};

[shader("vertex")]
VoxelOut voxelVertex(uint vertexId : SV_VertexID, uint instanceIndex : SV_StartInstanceLocation)
{
    Frame* frame = push.frame;
    Instance instance = buffers()->instances[instanceIndex];
    Vertex v = buffers()->vertices[buffers()->meshes[instance.mesh].vertexOffset + vertexId];
    float3 world = transform(instance.model, float4(v.position, 1.0)).xyz;
    // The grid as 0 to 1 each way, turned so the axis drawn along is depth.
    float3 inGrid = (world / giVoxelSize(frame, push.level) - float3(frame->gi.origin[push.level].xyz)) / float(GI_SIZE);
    float3 flat = push.list == 0 ? inGrid.yzx : push.list == 1 ? inGrid.zxy : inGrid;

    VoxelOut o;
    o.position = float4(flat.xy * 2.0 - 1.0, flat.z, 1.0);
    o.worldPosition = world;
    o.normal = instance.normal0.xyz * v.normal.x + instance.normal1.xyz * v.normal.y + instance.normal2.xyz * v.normal.z;
    o.uv = v.uv;
    o.material = instance.material;
    return o;
}

[shader("fragment")]
void voxelFragment(VoxelOut input)
{
    Frame* frame = push.frame;
    uint level = push.level;
    int3 voxel = int3(floor(input.worldPosition / giVoxelSize(frame, level))) - frame->gi.origin[level].xyz;
    if (any(voxel < 0) || any(voxel >= GI_SIZE))
    {
        return;
    }
    Material material = buffers()->materials[input.material];
    // Drawn small, so the texture's own mipmaps give each voxel its average colour.
    float4 base = material.baseColor * sampleTexture(material.baseColorTexture, input.uv);
    if (material.alphaCutoff > 0.0 && base.a < material.alphaCutoff)
    {
        discard;
    }
    float metallic = material.metallic * sampleTexture(material.metallicRoughnessTexture, input.uv).b;
    float3 emissive = material.emissive * sampleTexture(material.emissiveTexture, input.uv).rgb;
    int2 texel = int2(voxel.x + voxel.z * GI_SIZE, voxel.y + int(level) * GI_SIZE);
    // Metals send on little diffuse light; their colour shows in what they reflect instead.
    storageImagesRgba16f[push.textures.x][texel] = float4(base.rgb * (1.0 - metallic), 1.0);
    storageImagesRgba16f[push.textures.y][texel] = float4(normalize(input.normal), 1.0);
    storageImagesRgba16f[push.textures.z][texel] = float4(emissive, 1.0);
}
