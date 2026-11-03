---
order: 6
---

The Audio panel lists every sound the game is playing, as the mixer hears it. Open it from the **Window** menu or the command palette. It is the place to look when a sound doesn't play, plays too quietly, or plays when it shouldn't.

The line along the top counts the sounds playing, and says whether **Headphones** and the room's **Reverb** are on, with how long the room rings (see [[Sounds and Audio Sources]]).

## The Audio panel's columns

| Column | What it shows |
| --- | --- |
| Voice | What plays the sound: an Obj's Audio Source, by the Obj's name, a **One-shot** from `Audio.Play`, or the Inspector's **Preview** |
| Sound | The sound file |
| Bus | Effects, Music, or Preview for the editor's own sounds |
| Volume | The sound's own volume, from its import settings and the game |
| Heard | How loud it reaches the listener, in decibels from full: after its bus, the master volume, and for a 3D sound its distance |
| Distance | How far a 3D sound is from the listener, or **2D** |
| Through | How much of a 3D sound gets past the walls, with Occlusion on: **Open** when nothing is in the way |
| State | **Playing**, **Paused** or **Ending**, with **Loop** for a sound that loops and **HRTF** for one heard through headphones |

Sounds from Audio Sources come first, by name. Click one to select its Obj.

The list only shows while the panel is open. Closed, the mixer doesn't spend any time on it.
