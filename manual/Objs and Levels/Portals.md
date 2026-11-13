---
order: 5
---

A **Portal** is an opening that shows another place, live, as if the two places were joined there. Walk through it and you are in the other place, with no loading screen and no cut. Through two Portals a long way apart in one world, a player can travel between towns in a step.

Each Portal leads to another Portal. The far one is the near one seen from behind: what goes in at the front of one comes out of the front of the other, facing out of it and moving as fast as it went in.

## Making two Portals

The quickest way is the Hierarchy's **+ ▸ Portals**. It makes two Portals that lead to each other, each in a frame of stone blocks and marked on every map. Move the second wherever the first should lead.

To make one by hand, add **Add Component ▸ Physics ▸ Portal** to an Obj, then pick another Portal in its **Leads To** field. When the Portal you pick leads nowhere yet, it is set to lead back, so one pick joins the two both ways.

A Portal's opening stands on its Obj's position, on its sill, as wide as its Size's first number and as high as its second. It faces the way its Obj faces (its forward, the blue arrow). The Scene view draws it as an outline with an arrow out of its front and a line to the Portal it leads to.

## Portal settings

| Setting | What it does |
| --- | --- |
| **Leads To** | The Portal it shows and takes things to. With none, it shows nothing and nothing goes through it, though others can still lead to it |
| **Size** | The opening's width and height, in metres |
| **Open** | Closed, it shows its mist and nothing goes through |
| **Mist** | The colour of the mist it shows while it is closed, or while its far side is not ready |
| **View Distance** | How far from the opening a camera still sees the far side, in metres. Further off it shows its mist |

## What goes through

These go through by themselves:

- an Obj with a **Character Controller**;
- an Obj with a **Rigidbody**, keeping its speed and spin. Give something a script moves, such as an arrow, a kinematic Rigidbody and it goes through too.

An Obj goes through when its own position crosses the opening, so a character goes through as its feet do. What is under it goes with it. While an Obj is part-way through, it is drawn on both sides, so a character stepping through is whole from either place.

**Passes** says which layers' Objs go through. In a scene file it is a list of layer numbers; left out, every layer passes.

## Cameras

A camera that follows a player, as a third-person camera does, is often a few metres behind them. When the player steps through, the camera is still on the near side. Voxear handles this: while a Portal lies between a camera and what it films, the camera looks through the opening, as if the two places were joined there. The player never sees the place behind the Portal.

A camera under a Character Controller or a Rigidbody, as a first-person eye is, knows what it films already. For any other camera, set the Obj it films in its **Follows** field.

A script that keeps its own angle round the player, or its own speed, can turn it as the player goes through. `Portal.Passed` tells it how:

```csharp
protected override void OnEnable() => Portal.Passed += GoThrough;
protected override void OnDisable() => Portal.Passed -= GoThrough;

private void GoThrough(PortalPassage passage)
{
	if (passage.Obj == target)
	{
		turn += passage.Yaw;
		velocity = passage.CarryDirection(velocity);
	}
}
```

A camera that keeps clear of walls with a line test should first ask `Portal.Between(pivot, wanted)`. With a Portal between it and the player, nothing on the near side is in its way.

## In a world without end

In a [[Block Worlds|Block World]] without end, ground is only made near the camera. A Portal's far side may be kilometres away, where nothing is made yet. So as the camera comes within a Portal's view distance, the world also makes its ground round the far Portal, with its plants, as it does round the camera, but only out to **Far Side Distance**. Its far view reaches as far as the world's own does.

Until that ground is made and solid, the Portal shows its mist and lets nothing through, then clears over a moment. `portal.IsReady` says whether it has cleared.

## Making Portals from scripts

`Portal.CreatePair` makes two Portals that lead to each other, at two places:

```csharp
var size = new Vector2(3f, 4f);
var (town, camp) = Portal.CreatePair(townSquare, Quaternion.Identity, campfire, Quaternion.Identity, size);
camp.Close();
```

`Portal.Create(position, rotation, size)` makes one, leading nowhere until its `To` is set. `obj.AddPortal()` gives an Obj you already have a Portal, and `Portal.Link(first, second)` joins two Objs both ways. A Portal's `Size`, `IsOpen`, `Mist`, `ViewDistance` and `Passes` read and set what the Inspector shows. `Open()` and `Close()` open and close it. To move or turn one, move or turn its Obj.

`Portal.Passed` is raised once for each Obj that goes through, after it comes out, and before LateUpdate in the frame it went through. Its `PortalPassage` says which Obj, from which Portal to which, whether it went in at the back, and what going through did: `Carry` for a place, `CarryDirection` for a direction or a speed, `CarryRotation` for a turn, and `Yaw` in degrees.

## What Portals cost

A Portal's far side is drawn into the frame, only where its opening shows, and shares the frame's fog, bloom and anti-aliasing. It draws the far side's ground, models and sky, with the sun's shadows near the opening. It costs a little more the more of the screen the opening covers, and nothing while it is off screen, hidden behind something, closed, or further off than its view distance.

How much Portals may spend is the project's, in `voxear.toml`:

| Setting | What it does |
| --- | --- |
| Views | How many far sides are drawn at once, the nearest first: 4. The rest show their mist |
| Depth | 1 shows a Portal seen through a Portal as its mist; 2 shows its far side too |
| Shadow Distance | The sun casts shadows on a far side while the camera is within this many metres of the opening: 30 |
| Far Side Distance | How far round a far side a world without end makes its own blocks, in metres: 96 |
| Water | Whether the water of a far side's Block Worlds is drawn |

```toml
[portals]
views = 4
depth = 1
shadow_distance = 30.0
far_side_distance = 96.0
water = true
```

## In scene files

```toml
[[obj]]
id = 13
name = "Town Gate"
position = [-12.0, 31.0, -12.0]
rotation = [0.0, 225.0, 0.0]

[obj.portal]
to = 14                   # the Portal it leads to, by its place in the file
size = [3.0, 4.0]
open = false
passes = [0, 4]           # left out, every layer
mist = [0.35, 0.55, 1.0]  # linear
view_distance = 96.0

[[obj]]
id = 2
name = "Main Camera"
camera_follows = 12       # the Obj its camera films
```

A Portal that leads to an Obj outside the file, as in another scene, is saved leading nowhere.

## In a networked game

Each machine decides for itself what goes through, as it moves its own Objs, and makes the far sides its own players can see. Other players' characters that go through come out on the far side on every machine, never sliding from one place to the other.

## Not done yet

- Light and shadows from one side do not shine through to the other. A lamp near a Portal lights only its own side.
- Sound does not carry through.
- A body resting part-way through, such as a crate left in the opening, collides only on the side its middle is on.
- Particles, smoke, glass, grass on a Terrain and the sea are not drawn on a far side. The water of Block Worlds is.
- The far side is lit by the sky's light, not by bounced light.
- A Portal leads within the world it is in. Portals to another level are not done yet.
- The server does not yet decide crossings for every player and check them, as it does for movement.
