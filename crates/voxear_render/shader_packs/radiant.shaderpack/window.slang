// Reading the light kept for the blocks round the camera from a surface program, which has no lightAround of its
// own: a block's sky light and block light, whether it is solid, and the way its block light comes from.

// The light byte of the block a point is in: sky light in the high four bits, block light in the low, PACK_LIGHT_SOLID
// for a solid block; open sky where the window keeps none.
uint windowCell(float3 position)
{
    PackLightWindow* window = frame()->lightWindow;
    if (window == nullptr || window->found == 0)
    {
        return 0xF0;
    }
    int3 cell = int3(floor((position - window->corner) / window->blockSize));
    if (any(cell < int3(0)) || any(cell >= PACK_LIGHT_WINDOW))
    {
        return 0xF0;
    }
    uint index = (uint(cell.y) * uint(PACK_LIGHT_WINDOW.z) + uint(cell.z)) * uint(PACK_LIGHT_WINDOW.x) + uint(cell.x);
    return (frame()->lightBytes[index >> 2] >> ((index & 3) * 8)) & 0xFF;
}

// A block's block light from 0 to 1, as bright as the blocks round it let it be where it is solid.
float windowBlockLight(float3 position)
{
    uint light = windowCell(position);
    return light == PACK_LIGHT_SOLID ? -1.0 : float(light & 15) / 15.0;
}

// The way block light comes to a face from, as the way it grows brightest from the block in front of the face: up
// to a torch, round a corner. Zero where it is the same all round.
float3 torchWay(float3 position, float3 faceNormal)
{
    PackLightWindow* window = frame()->lightWindow;
    if (window == nullptr || window->found == 0)
    {
        return 0.0;
    }
    float size = window->blockSize;
    float3 front = position + faceNormal * (0.5 * size);
    float here = max(windowBlockLight(front), 0.0);
    float3 way = 0.0;
    for (uint axis = 0; axis < 3; axis++)
    {
        float3 step = 0.0;
        step[axis] = size;
        float plus = windowBlockLight(front + step);
        float minus = windowBlockLight(front - step);
        // A solid block on one side: only the open side says anything.
        plus = plus < 0.0 ? here : plus;
        minus = minus < 0.0 ? here : minus;
        way[axis] = plus - minus;
    }
    return way;
}

// How many blocks and a half a walk towards the sun takes at most.
static const int SUN_STEPS = 10;

// Whether the sun reaches a point between the blocks: walked towards it a block and a half at a time until a solid
// block stops it or the open sky is found. `start`, from 0 to 1, is how far into the first step it begins: noise
// there that changes each frame turns the steps' edges into a grain TAA smooths away.
float sunReaches(float3 position, float3 toSun, float size, float start = 0.5)
{
    float3 at = position + toSun * (1.5 * size) * (start - 1.0);
    for (int i = 0; i < SUN_STEPS; i++)
    {
        at += toSun * (1.5 * size);
        uint light = windowCell(at);
        if (light == PACK_LIGHT_SOLID)
        {
            return 0.0;
        }
        if ((light >> 4) == 15)
        {
            return 1.0;
        }
    }
    return smoothstep(9.0, 14.0, float(windowCell(at) >> 4));
}
