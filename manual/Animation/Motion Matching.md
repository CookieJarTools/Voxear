---
order: 2
---

A **Motion Matcher** animates a character from its clips, with no graph of states to wire up. Ten times a second, and at once whenever the movement wanted changes, it searches its model's looping clips for the ones that move as the character should: as fast, the same way, along the same path. It blends the few that match best, fading each one's share in and out so nothing pops. Below a walk, a clip that stands still fades in instead.

Add one with **Add Component ▸ Animation ▸ Motion Matcher** to an Obj whose model was made to animate. Its clips are what it has to choose from:

- Looping clips that move steadily are what it walks, runs and steps aside with. A walk forward, a jog, a run, steps to each side and back give it the most to work with.
- A loop made to stay in place, as many walks are, still counts: it moves as fast as its planted foot slides back.
- **Idle Clip** is the clip it stands with. None picks the first looping clip that stands still.

The clips blended play in step, each the same share of the way through its loop, so their feet fall together. With **Match Speed** on, they play a little faster or slower so their feet keep pace with the ground the Obj covers.

## Moving the character

The game moves the Obj, with a Character Controller or by setting its position, and the model stands on it. Left alone, the Motion Matcher follows how the Obj moves. Tell it where the player wants to go and it changes pace as soon as the keys do:

```csharp
public sealed class Walker : Behaviour
{
	[SerializeField] private float speed = 1.4f;

	protected override void Update()
	{
		var ahead = (Keyboard.IsDown(Key.W) ? 1f : 0f) - (Keyboard.IsDown(Key.S) ? 1f : 0f);
		var aside = (Keyboard.IsDown(Key.D) ? 1f : 0f) - (Keyboard.IsDown(Key.A) ? 1f : 0f);
		var wanted = (Obj.Forward * ahead + Obj.Right * aside) * speed;
		var matcher = Obj.MotionMatcher;
		matcher.WantedVelocity = wanted;
		var controller = Obj.CharacterController;
		controller.Move((wanted - Vector3.UnitY) * Time.Delta);
	}
}
```

The body turns the way it goes, at its **Turn Rate**. To keep it facing one way while it steps aside or back, as when aiming, set `WantedFacing`; set it to zero to face the way it goes again. `FollowObj()` forgets both.

## Settings

- **Idle Clip**: the clip it stands with.
- **Idle Speed**: below this many metres a second wanted, it stands.
- **Search Every**: seconds between searches.
- **Blend Time**: how long a change of clips takes to fade in.
- **Turn Rate**: how fast the body turns, in degrees a second. 0 turns at once.
- **Turn To Movement**: the body faces the way it goes. Off, it faces the Obj's forward, for a game that turns the Obj itself.
- **Match Speed**: clips keep pace with the ground.
- **Debug**: draws the velocity wanted in blue, the clips' velocity in orange and the body's facing in white, and names the clips playing above its head.

## What it needs from the model

It finds the root bone by its name, `root`, or else takes the first. The root marks where the character stands on the ground and which way it faces in each frame of each clip. For loops that stay in place, it finds the feet by their names: toes, feet or ankles, marked left and right.
