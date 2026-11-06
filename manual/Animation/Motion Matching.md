---
order: 2
---

A **Motion Matcher** animates a character from its clips without a graph of states to wire up. Many times a second it looks through every frame of the model's clips for the one that best carries on from the pose it shows into where the character is heading, and plays on from there. A start, a stop, a turn on the spot or a step back each come from whichever moment of the clips fits best, and each change of frame settles in without a pop.

Add one with **Add Component ▸ Animation ▸ Motion Matcher** to an Obj whose model was made to animate. Its clips are what it has to choose from. Long, unbroken takes of someone walking about, starting, stopping and turning give it far more to choose from than short loops.

## Moving the character

The game moves the Obj, with a Character Controller or by setting its position, and turns it to face where the character faces. The body follows: it moves as its clips carry it, pulled towards the Obj and never further from it than **Max Drift**.

Left alone, the Motion Matcher expects the Obj to carry on as it is moving. Tell it where the player wants to go and it turns, starts and slows before the Obj does:

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

`WantedFacing` does the same for the way the character should face. `FollowObj()` forgets both.

## Settings

- **Response**: how quickly the character is expected to reach what is wanted, in seconds to halve the gap. Match it to how quickly the game's movement eases in.
- **Search Every**: seconds between searches.
- **Blend Time**: how long a jump to a new frame takes to settle.
- **Max Drift**: how far, in metres, the body may stray from the Obj.
- **Max Turn**: how far, in degrees, the body may turn from the Obj's facing.
- **Foot Position**, **Foot Velocity**, **Hip Velocity**, **Path Position** and **Path Facing**: how much each counts in a search. Raise the path's to follow the player more closely, the feet's for smoother steps.
- **Debug**: draws the path wanted in blue, the path of the frame playing in orange, and a white line from the Obj to the body.

## What it needs from the model

It finds the bones it follows by their names: the root (a bone named `root`, or else the first), the hips (or pelvis), and each foot (or ankle), marked left and right. The root bone marks where the character stands on the ground and which way it faces in each frame of each clip.
