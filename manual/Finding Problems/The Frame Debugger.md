---
order: 3
---

# The Frame Debugger

The Frame Debugger shows how a frame is drawn, one step at a time. Voxear draws each frame in a series of **passes**: first the shadows, then the solid Objs, then the sky, then effects such as bloom, and last the editor's own panels. When something looks wrong on screen, the Frame Debugger shows which pass it goes wrong in.

It is in the **Profiling** layout, beside the Profiler. It always shows the frame just drawn, and keeps up as frames go by.

## Looking at a pass

The line at the top sums up the frame: how many passes there were, how long the graphics card took, and how many Objs were drawn or skipped.

1. Pick a pass in **Passes**. Each shows how long the graphics card spent on it.
2. **Images** lists the pictures the pass used, and how: drawn into, read from, copied and so on. The first one it drew into is picked for you.
3. **View** shows the picked image as it stood when that pass finished.

Over the view:

- **RGB**, **R**, **G**, **B** and **A** show all the colours, or one channel on its own, such as the transparency in **A**.
- **Mip** picks a smaller copy of the image, for images that have them.
- **Exposure** makes the image brighter or darker, in stops, so that very dark or very bright images can be seen. Depth images start brighter for that reason.

Because you are looking at the frame the editor drew, the editor's own panels are among its passes.

> [!NOTE]
> On some systems the window's own final image cannot be copied, and the view says so. The other passes still show.
