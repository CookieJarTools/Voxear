---
order: 9
---

An HDR display shows highlights far brighter than white paper: the sun on water, a lamp's bulb, a fire's embers. Voxear draws in HDR all the time, and on an HDR display it can show the picture that way too, rather than squeezing it into what any screen shows. On any other display, the game shows SDR as before.

HDR output is off unless the game turns it on, as an option for whoever plays it.

## The two kinds

| Kind | What it is |
| --- | --- |
| **HDR10** | 10 bits a channel, through the PQ curve, in the wide BT.2020 colours. What most HDR TVs and monitors take. Tried first |
| **scRGB** | Half floats in sRGB's colours, 1 being 80 nits, past 1 for brighter. The system fits it to the display |

Which a display offers depends on the display, its driver and the desktop: on Linux, a compositor with HDR turned on for that screen; on Windows, **Use HDR** in the display settings. Where neither is offered, the game shows SDR, and says so in the Console.

## Brightness

| Setting | What it does |
| --- | --- |
| Paper white, 200 nits | How bright the picture's whites and the UI are. The picture up to paper white looks as it does in SDR, at this brightness. Lower for a dark room, higher for a bright one: 80 to 500 suit most |
| Peak brightness, 1000 nits | The brightest the display shows. Highlights brighter than paper white roll off towards it, rather than stopping at white. Set it to the display's own peak, from its box or a calibration screen |

The tonemapper and the grade work as in SDR, so a level looks the same up to paper white; the brightest parts are then lifted by how much brighter than paper white they were, up to the peak. Bloom, fog and every other effect are as they are in SDR.

## From scripts

`Screen` holds the HDR settings, for a game's options menu:

```csharp
if (Screen.HdrAvailable)
{
	Screen.SetHdr(HdrMode.On, paperWhite: 200f, peakBrightness: 1000f);
}
Log.Info($"showing {Screen.HdrOutput}");
```

| Member | What it does |
| --- | --- |
| `Screen.Hdr` | `HdrMode.Off`, `On` (HDR10 first, then scRGB), `Hdr10` or `ScRgb` alone |
| `Screen.HdrOutput` | What the window shows now: `Sdr`, `Hdr10` or `ScRgb` |
| `Screen.HdrAvailable` | Whether the display offers any HDR, so a menu can hide its HDR settings when not |
| `Screen.PaperWhite`, `Screen.PeakBrightness` | The two brightnesses, in nits |
| `Screen.SetHdr(mode, paperWhite, peakBrightness)` | All three at once |

A change shows from the next frame. Voxear does not keep it: save it with the game's other options (see [[Saving the Game]]) and set it again as the game starts. In the editor, the settings are kept for scripts to read, and the editor's own window stays SDR.

To try a display before a game has a menu for it, start the game with `VOXEAR_HDR=on` (or `hdr10`, `scrgb`) set.

## HDR pictures

Pictures can hold light past white too: `.exr` and `.hdr` files are read as light as it is, never as sRGB colours. Use one as a material's **Emission** map for a glow that is bright in its own right, such as a neon sign's tubes or the stars of a night sky's dome. A build compresses them as BC6H, an eighth of their size as floats, with their mips made in linear light; leave **Compress** off in their import settings to keep them as half floats. A sky can be drawn from one, a panorama of a whole sky (see [[The Sky]]); light probes will take them too, once Voxear bakes them.

## What it cannot do yet

- Screenshots and VoxDocs captures are taken only from SDR output.
- The editor's own windows are always SDR.
