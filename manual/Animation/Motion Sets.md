---
order: 4
---

A **Motion Set** holds what a character can do: the ways it gets about, as walking, crouching or falling, and the [[Actions]] it plays over them, as jumping, casting or dying. Every character that plays the set shares it, so a change to it shows on all of them.

Right-click in the Project panel and choose **New Motion Set**. Select it, and the Inspector shows its states and actions. Give it to a [[Motion Matching|Motion Matcher]] in its **Motion Set** field.

## States

A state is one way the character gets about. Each has:

- **Name**: what code and other fields know it by.
- **Role**: what it is for. **Grounded** walks and runs about on the ground, **Crouched** crouches, **Airborne** jumps or falls, **Swimming** swims. **Custom** plays only when code asks for it by its name.
- **Idle Clip**: the clip it stands with. None picks the first of its loops that stands still.
- **Clips**: its loops, the ones the Motion Matcher blends as the character moves. Turn on **Backwards** to play a loop in reverse: a forward walk played backwards is a backward walk.
- **Blend Time**: how long changing into the state takes to settle, in seconds.
- **Enter Action**: an action played on the way in, as standing to crouching.

The first state that is **Grounded** plays to begin with. A state change fades the old pose into the new one over the new state's Blend Time.

## Choosing a state

From a script, set the Motion Matcher's `State` to a role, and the set's first state for it plays:

```csharp
var matcher = body.MotionMatcher;
matcher.State = crouching ? MotionState.Crouched : MotionState.Grounded;
```

For a custom state, give the script a `MotionSetState` field and pick the state in the Inspector, then call `SetState`:

```csharp
[SerializeField] private MotionSetState sitting;

private void Sit()
{
	var matcher = body.MotionMatcher;
	matcher.SetState(sitting);
}
```

`CurrentState` is the state playing.

## Root motion

With **Root Motion** on the Motion Matcher, the clips move the character rather than the game: through the Character Controller on the matcher's Obj or the nearest Obj above it, else the Obj itself. A clip set to move at a Speed and Direction (see [[Animation Clips]]) moves it at exactly those.

## In the file

A Motion Set is a `.motionset` file, plain text:

```toml
[[state]]
name = "Walking"
role = "grounded"
idle = "Models/Hero.fbx#clip:Idle_Loop"
[[state.clip]]
clip = "Models/Hero.fbx#clip:Walk_Loop"
[[state.clip]]
clip = "Models/Hero.fbx#clip:Walk_Loop"
backwards = true

[[action]]
name = "Cast"
clip = "Models/Hero.fbx#clip:Spell_Shoot"
body = "upper"
[[action.mark]]
name = "Release"
time = 0.2
```
