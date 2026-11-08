// Skinning (src/skinning.rs): a skinned part's vertices moved by its bones,
// each by up to four, weighed, into its posed vertices. push.textures: x
// where its mesh's vertices start, y its skin, z its bones' matrices among
// the frame's, w where its posed vertices go; push.size.x how many.
//
// Each vertex also says how much of it is bound to hidden bones, in the top
// byte of its wind masks, which only plants' own vertices use; the passes
// that draw what a camera sees leave out what is mostly hidden (shading.slang).

#include "common.slang"

// A bone's matrix, three rows of a 3×4, and a fourth row whose x is 1 for a hidden bone.
float3x4 boneMatrix(uint index)
{
    float4* rows = buffers()->skinMatrices;
    return float3x4(rows[index * 4], rows[index * 4 + 1], rows[index * 4 + 2]);
}

bool boneHidden(uint index)
{
    return buffers()->skinMatrices[index * 4 + 3].x > 0.5;
}

[shader("compute")]
[numthreads(64, 1, 1)]
void skinMain(uint3 id : SV_DispatchThreadID)
{
    if (id.x >= push.size.x)
    {
        return;
    }
    Vertex v = buffers()->vertices[push.textures.x + id.x];
    // Four joints and four weights out of 65535, two to a word.
    uint4 skin = buffers()->skins[push.textures.y + id.x];
    uint joints[4] = { skin.x & 0xFFFF, skin.x >> 16, skin.y & 0xFFFF, skin.y >> 16 };
    float weights[4] = { float(skin.z & 0xFFFF), float(skin.z >> 16), float(skin.w & 0xFFFF), float(skin.w >> 16) };
    float3x4 moved = 0.0;
    float hidden = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        if (weights[k] > 0.0)
        {
            uint bone = push.textures.z + joints[k];
            moved += boneMatrix(bone) * (weights[k] / 65535.0);
            hidden += boneHidden(bone) ? weights[k] / 65535.0 : 0.0;
        }
    }
    v.wind = (v.wind & 0x00FFFFFF) | (uint(saturate(hidden) * 255.0 + 0.5) << 24);
    float3x3 turn = float3x3(moved[0].xyz, moved[1].xyz, moved[2].xyz);
    v.position = mul(moved, float4(v.position, 1.0));
    v.normal = normalizeOr(mul(turn, v.normal), v.normal);
    v.tangent.xyz = normalizeOr(mul(turn, v.tangent.xyz), v.tangent.xyz);
    buffers()->vertices[push.textures.w + id.x] = v;
}
