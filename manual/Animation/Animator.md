---
order: 2
---

An **Animator** plays one clip on its Obj's model. Add one with **Add Component ▸ Animation ▸ Animator** to an Obj whose model was made to animate: a glTF or FBX file with a skeleton.

- **Clip**: the clip it plays, the model's own or one from another file with the same rig, or made for another humanoid (see [[Animation Clips]]). None holds the model at rest.
- **Speed**: 1 plays the clip as it was made, 2 twice as fast.
- **Looping**: going round from the end to the start, or holding the last frame.
- **Blend Time**: how long a change of clip takes to settle into the new one, in seconds. Nothing pops: the old pose eases into the new.
- **Root Motion**: moving the Obj along the ground as the clip's root bone travels, rather than the model away from its Obj.

The bones are posed after `Update` and before `LateUpdate`, so a camera placed in `LateUpdate` sees this frame's pose.

A script reaches it through `Obj.Animator`, or adds one with `Obj.AddAnimator()`. Its fields are the Inspector's, with **Blend Time** as `BlendTime`:

```csharp
if (Obj.Animator is { } animator)
{
	animator.Speed = 1.5f;
	animator.Play(new AnimationClip("Models/Guard.glb#clip:Wave"), looping: false);
}
```

`Stop()` clears the clip and holds the model at rest. From a terminal, `field.set` sets the same fields on an Obj in the editor: `vox editor call field.set obj=Guard component=animator field=speed value=1.5`, with the fields named `clip`, `speed`, `looping`, `blend` and `root_motion` (see [[Driving the Editor from a Terminal]]).

For a character the game moves, use a [[Motion Matching|Motion Matcher]] instead.
