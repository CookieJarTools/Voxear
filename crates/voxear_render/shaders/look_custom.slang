// Look shaders: a project's own last word on the finished picture, written
// in .shader files with a `look` function and compiled in when there are
// any. A look pack names the one a player's picture is drawn with
// (src/looks.rs's LookShader).

// One pixel of the finished picture: its colour as the level's lighting and
// post-processing left it, from 0 to 1; where it is, from 0 to 1 across and
// down, and in pixels; the picture's size in pixels; and the time in
// seconds. A look shader returns the colour the pixel is shown in.
struct LookInput
{
    float3 color;
    float2 uv;
    float2 pixel;
    float2 size;
    float time;
};

#ifdef VOXEAR_CUSTOM_SHADERS
#include "project_look_shaders.slang"
#else
float3 customLook(uint shader, LookInput input)
{
    return input.color;
}
#endif
