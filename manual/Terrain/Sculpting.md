---
order: 4
---

Pick **Sculpt** along the top of the Terrain in the Inspector, and paint the ground in the Scene view with the left mouse button. A ring on the ground shows the brush, with an inner ring where it is strongest; it eases to nothing at its edge.

The brush paints into a **Sculpt** layer (see [[Layers]]): the one picked when it is one, else the topmost, else a new one it adds on top. Its strokes stay a layer of their own, so they can be switched off, moved under a stamp or taken out. Each stroke is one step to undo.

## Brushes

| Brush | What it does |
| --- | --- |
| Raise or Lower | Raises the ground while held; hold **Shift** to lower it |
| Smooth | Evens out bumps. Begun on dug ground, it evens that out instead, rounding a trench's or a pit's sharp edges, into the Dig layer |
| Flatten | Brings the ground to the height under the brush where the stroke began |
| Set Height | Brings the ground to its **Height**; **Ctrl**+click takes the height under the pointer |
| Terrace | Cuts the ground into steps **Step** metres high |
| Noise | Roughens the ground with bumps **Bump size** metres across; **Shift** turns them over |
| Holes | Cuts holes in the ground; **Shift** as the stroke starts fills them. See [[Holes]] |
| Dig | Digs a ball the brush's size out of the ground where you click, and along where you drag; **Shift** fills instead. Click again in the same place to dig deeper. See [[Digging]] |
| Fill | Heaps a ball of ground up; **Shift** digs instead |

| Setting | What it does |
| --- | --- |
| Size | The brush's radius, in metres |
| Strength | How fast it works, 0 to 1 |

**[** and **]** make the brush smaller and larger, and with **Shift** weaker and stronger.

A brush works as long as it is held, so holding it still keeps raising the ground, and a stroke does the same however fast the editor draws.
