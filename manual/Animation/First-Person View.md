---
order: 7
---

A **First-Person View** puts the camera in a character's own head. Add it to the character's root Obj, with **Add Component ▸ Animation ▸ First-Person View**. It finds the body, the first Model Renderer under its Obj whose skeleton has a head and arms, and the camera, the first under it, or else the first in the level.

- The mouse turns it, by **Sensitivity** degrees a pixel, between its **Tilt Limits**. Turn **Mouse Look** off to turn it from a script with `view.Yaw` and `view.Pitch`.
- **Turn Obj**: the Obj it is on turns to face where it looks.
- **Lean**: how much of the tilt the upper body leans into, so looking down bends the character over rather than only the camera.
- **Head Bob**: how much of the head's bob reaches the camera. A camera that follows the head exactly is hard to look through, so it is steadied.
- **Eyes**: where the eyes are from the head bone.
- **Hide Head**: the head and neck are left out of what the camera sees, so it doesn't see inside them, but they still cast their shadow (see [[Bone Attachments]]). While a gun's clips hold it, the camera sits low, near the neck, and the upper chest and collarbones round it are left out too, as the steps carry them up past the camera; the arms stay.

```csharp
var view = Obj.FirstPersonView;
var aim = view.Forward;
```

A script can also move the eyes for a moment, and read where they are:

| Call | What it does |
| --- | --- |
| `view.EyePosition`, `view.EyeRotation` | Where the eyes are this frame, in the world, and which way they look |
| `view.Punch(spin)` | Jolts the camera, in degrees a second about its across, up and forward. It springs back |
| `view.Glance(degrees)` | The eyes look down for this frame, toward the hands, and ease back once it stops being asked |
| `view.FrameEyes(at, weight)` | The eyes move toward a place by the Obj for this frame, and go back to the head once it stops being asked |

In networked play, only this machine's own player's view moves the camera, hides the head and takes the mouse. Everyone else's keeps their head on.

While playing, the camera it places sees from 5 cm away rather than the usual 10 cm, so the gun held at the eye and the hands working right before it are not cut off, while the neck and shoulders round the camera stay out of sight.

## Guns

A [[Holding and Aiming|Gun Holder]] holds its gun in front of the view's eyes.
