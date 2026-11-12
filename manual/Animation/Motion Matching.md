---
order: 3
---

A **Motion Matcher** animates a character from its clips, with no graph of states to wire up. Ten times a second, and at once whenever the movement wanted changes, it searches its looping clips for the ones that move as the character should: as fast, the same way, along the same path. It blends the few that match best, fading each one's share in and out so nothing pops. Below a walk, a clip that stands still fades in instead.

Add one with **Add Component ▸ Animation ▸ Motion Matcher** to an Obj whose model was made to animate, and give it a [[Motion Sets|Motion Set]]: the clips of the state playing are what it has to choose from. With no Motion Set, it plays the model's own clips.

- Looping clips that move steadily are what it walks, runs and steps aside with. A walk forward, a jog, a run, steps to each side and back give it the most to work with.
- A loop made to stay in place, as many walks are, still counts: it moves as fast as its planted foot slides back, or as its clip settings say (see [[Animation Clips]]).
- The state's **Idle Clip** is the clip it stands with.

The clips blended play in step, each the same share of the way through its loop, so their feet fall together. With **Match Speed** on, they play a little faster or slower so their feet keep pace with the ground the Obj covers. With **Foot Locking** on, a foot the clips have down stays where it came down, the leg bending to reach it, until the clips lift it or the leg would have to stretch too far; then it eases back to the clips' foot.

## Moving the character

The Motion Matcher moves nothing itself and needs no particular movement code. The game moves the Obj however it likes, with a Character Controller, a Rigidbody, by setting its position or along a path, and the model stands on it. Left alone, the Motion Matcher follows how the Obj moves. Tell it where the character wants to go, from keys, a stick, an AI or anything else, and it changes pace as soon as that does:

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

The body turns the way it goes, at its **Turn Rate**. To keep it facing one way, as when aiming, set `WantedFacing`; set it to zero to face the way it goes again. `FollowObj()` forgets both.

## Moving by the clips

To have the clips move the character instead, turn on **Root Motion**: they move it through the Character Controller on the matcher's Obj or the nearest Obj above it, as on a player whose model is a child, and else move the Obj itself. To move the character your own way by them, leave Root Motion off and read `ClipVelocity`: how fast and which way the clips playing carry the body, in metres a second.

While an action stands the character still or moves it itself, as a sit or a roll, `ActionTakesMovement` is true. A game moving the character itself should leave it to `ClipVelocity` meanwhile.

`FeetDown` says which feet the clips playing have down this frame, left then right. A foot that was up last frame and is down now is a footfall, for a step's sound, or a held gun's bob.

`AimPitch` bends the spine a little to look up or down, down positive: set it to the view's pitch. For a first-person camera on the body, see [[Bone Attachments]].

Going another way than it faces, the hips and legs turn towards the way it goes, up to **Leg Turn**, and the spine twists back so the chest still faces ahead. With only a forward and a backward walk, it walks forward and to the side, or backs away to the side, turning smoothly from one to the next. Steps to each side, if the model has them, take over once they fit better.

## Settings

- **Motion Set**: its states and actions. None plays the model's own clips, standing with the first loop that stands still.
- **Root Motion**: the clips move the character, rather than the game moving it.
- **Idle Speed**: below this many metres a second wanted, it stands.
- **Search Every**: seconds between searches.
- **Blend Time**: how long a change of clips takes to fade in.
- **Turn Rate**: how fast the body turns, in degrees a second. 0 turns at once.
- **Turn To Movement**: the body faces the way it goes. Off, it faces the Obj's forward, for a game that turns the Obj itself.
- **Leg Turn**: how far, in degrees, the hips and legs may turn from the way the body faces. 0 keeps them with the body.
- **Match Speed**: clips keep pace with the ground.
- **Foot Locking**: feet stay where they come down.
- **Debug**: draws the velocity wanted in blue, the clips' velocity in orange and the body's facing in white, and names the clips playing above its head.

## What it needs from the model

It finds the root bone by its name, `root`, or else takes the first. The root marks where the character stands on the ground and which way it faces in each frame of each clip. For loops that stay in place, it finds the feet by their names: toes, feet or ankles, marked left and right. To twist the body as the legs turn, it finds the spine's bones by their names, `spine` or `chest`. To hold feet down, it finds each leg from its foot or ankle bone and the two bones above it.
