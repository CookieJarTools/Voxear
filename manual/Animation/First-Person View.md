---
order: 7
---

A **First-Person View** puts the camera in a character's own head. Add it to the character's root Obj, with **Add Component ▸ Animation ▸ First-Person View**. It finds the body, the first Model Renderer under its Obj, and the camera, the first under it, or else the first in the level.

- The mouse turns it, by **Sensitivity** degrees a pixel, between its **Tilt Limits**. Turn **Mouse Look** off to turn it from a script with `view.Yaw` and `view.Pitch`.
- **Turn Obj**: the Obj it is on turns to face where it looks.
- **Lean**: how much of the tilt the upper body leans into, so looking down bends the character over rather than only the camera.
- **Head Bob**: how much of the head's bob reaches the camera. A camera that follows the head exactly is hard to look through, so it is steadied.
- **Eyes**: where the eyes are from the head bone.
- **Hide Head**: the head is hidden so the camera doesn't see inside it.

```csharp
var view = Obj.FirstPersonView;
var aim = view.Forward;
```

In networked play, only this machine's own player's view moves the camera, hides the head and takes the mouse. Everyone else's keeps their head on.

While playing, the camera it places sees from 1 cm away rather than the usual 10 cm, so the gun held at the eye and the hands working right before it are never cut off. A hidden head shrinks up and behind the head bone, out of the camera's way.

## Guns

A [[Holding and Aiming|Gun Holder]] holds its gun in front of the view's eyes.
