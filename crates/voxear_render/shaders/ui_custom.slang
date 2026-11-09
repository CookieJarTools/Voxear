// UI shaders: what one is given to work out a pixel of an element's
// background (`shader: …` in its style; see ui.slang and voxear_ui's
// UiShaders), and a project's own, written in .shader files with a `ui`
// function and compiled in when there are any.

// One pixel of a shaded background: where it is in the element's box and
// the box's size, in logical pixels, and from 0 to 1 across and down; the
// element's background and border colours, linear and premultiplied, which
// its transitions ease; the time in seconds; and the display's physical
// pixels per logical one. A UI shader returns the background's colour
// there, premultiplied; the border, rounded corners and opacity are put on
// after.
struct UiInput
{
    float2 position;
    float2 size;
    float2 uv;
    float4 background;
    float4 border;
    float time;
    float scale;
};

#ifdef VOXEAR_CUSTOM_SHADERS
#include "project_ui_shaders.slang"
#else
float4 customUi(uint shader, uint first, UiInput input)
{
    return input.background;
}
#endif
