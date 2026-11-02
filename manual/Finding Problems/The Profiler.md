---
order: 2
---

When your game runs slowly, the Profiler shows where the time goes. It records each frame the editor draws, the game's frames included while it plays, and breaks every frame down into the work done in it.

## Opening the Profiler

Open the **Layout** menu, at the top left of the editor, and pick **Profiling**. This switches to a layout made for finding slow spots, with the Game view at the top and the Profiler under it. Pick **Default** to go back; each layout stays as you left it.

The Profiler records while its tab is showing and **Record** is on. It shares its place with the Entities panel, the Frame Debugger and the Console, so it stops recording while you look at one of those. Drag its tab elsewhere, or float it, to keep it recording while you use the others.

## The Profiler's chart

The chart at the top shows the last 300 frames: how long each took on the main processor (CPU) and on the graphics card (GPU), in milliseconds. Lines mark 60 and 30 frames a second. A frame above the 60 line took too long for a smooth 60 frames a second.

Click the chart to look at one frame. **Live** goes back to the newest. **Record** stops and starts recording, and **Clear** forgets the frames kept so far.

The line under the toolbar sums up the frame: frames a second, how long it took, the CPU and GPU times, and how much memory your scripts asked for (**GC**).

## The Profiler's Hierarchy

**Hierarchy** lists the work done in the frame as a tree, slowest first:

| Column | What it means |
| --- | --- |
| Total | The share of the frame spent in this row, and everything inside it |
| Self | The share spent in this row alone |
| Calls | How many times it ran in the frame |
| GC Alloc | Memory your scripts asked for while it ran |
| Time ms, Self ms | The same as Total and Self, in milliseconds |

Open **Game** to see the game's own work: **FixedUpdate**, **Update**, **LateUpdate** and the rest, and inside them each behaviour's methods by class, as `MyGame.Spinner.Update()`. All the Spinners share one row, and **Calls** says how many there were.

**Editor**, **UI** and **Render** are the editor's own work: its panels, its UI, and drawing the frame. Inside **UI**, **Styles** is working out how elements look after something changed, **Layout** where they go, and **Draw** turning them into shapes and text. With the display's refresh rate holding the frame back, **Render** includes the time spent waiting for it, so it can fill most of the frame without anything being wrong.

> [!TIP]
> **GC Alloc** is worth watching. Memory asked for every frame must be cleaned up later, and the cleaning makes the game stutter. A behaviour that shows GC Alloc on every frame is worth fixing first.

## The Profiler's Timeline

**Timeline** shows the same frame as bars across time, so you can see what ran when, and what ran inside what. Point at a bar to see its name, how long it took, and when in the frame it started. Script work is purple, the engine's blue, drawing green, and waiting grey.

Work on other threads, such as loading a level in the background, shows in lanes of its own under the main thread.

## Drawing passes and what a frame drew

The right side lists each drawing pass with how long the graphics card took over it, and what the frame drew: how many Objs were drawn and how many were skipped because they were out of view or hidden behind others, the levels of detail used, and what is loaded.

The graphics card's times are from a frame or two earlier than the rest, since the card finishes its work after the processor does.

## Timing your own code

Wrap the code you want to time in a sample, and it shows as a row of its own:

```csharp
using (Profiler.Sample("Find Path"))
{
	FindPath();
}
```

For code that runs very often, make a marker once and use it each time:

```csharp
private static readonly ProfilerMarker Pathing = new("Find Path");

private void Plan()
{
	using (Pathing.Auto())
	{
		FindPath();
	}
}
```

`Profiler.BeginSample("Find Path")` and `Profiler.EndSample()` work too, as in Unity. Samples cost almost nothing while the Profiler is not recording.

> [!NOTE]
> The Profiler records the editor, and the game while it plays in the editor. It cannot connect to a build or to `vox run` yet.
