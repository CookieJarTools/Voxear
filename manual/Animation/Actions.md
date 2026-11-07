---
order: 5
---

An action is a clip a [[Motion Sets|Motion Set]] plays once over its state: swinging a wand, throwing, jumping, landing, sitting down or dying. It fades in, plays, and fades out near its end, unless it loops or holds its last frame. While it plays, the state's clips play on underneath, so the legs keep walking under a wand swing.

## Settings

- **Name**: what code and other fields know it by.
- **Clip**: the clip it plays.
- **Body**: the part of the body it plays on. **Full Body**, **Upper Body** (the spine and everything above it), **Arms**, **Left Arm** and **Right Arm**, or **From Bone** for a bone and every bone under it. The rest of the body plays the state's clips.
- **Blend In** and **Blend Out**: how long it takes to fade in, and out at its end, in seconds.
- **Loop**: goes round until stopped.
- **Hold End**: stays on its last frame until another action plays or it is stopped, as for dying.
- **Movement**: what the character does while it plays.
  - **Keep Moving**: walks on as the game moves it, as for a wand swing.
  - **Freeze**: stands still, as for sitting down.
  - **Root Motion**: goes where the clip's root goes, as for a roll.
  - **Lunge**: goes ahead **Lunge Distance** metres, starting **Lunge Start** seconds in and taking **Lunge Length** seconds, as for a sword thrust.
- **Turn Until**: the body does not turn until this many seconds in.
- **Cancel From** and **Moving Cancels**: wanting to move after this many seconds in ends it, as for a landing a player can run out of.
- **Marks**: named moments, as the moment a grenade leaves the hand.

## Playing an action

Give the script a `MotionAction` field, pick the action in the Inspector, and play it on the Motion Matcher:

```csharp
[SerializeField] private Obj body;
[SerializeField] private MotionAction cast;

protected override void Update()
{
	if (Mouse.WasPressed(MouseButton.Right))
	{
		var matcher = body.MotionMatcher;
		matcher.Play(cast);
	}
}
```

One action plays at a time: playing another takes its place. `StopAction()` fades out the one playing. `ActionPlaying` is the action playing, and `ActionTime` how far into it it is.

## Marks

When an action passes a mark, `OnMotionMark` is called on the behaviours of the Motion Matcher's Obj and of every Obj above it, so a player script on the Obj above its body hears its body's marks. Give the script a `MotionMark` field, pick the mark in the Inspector, and compare:

```csharp
[SerializeField] private MotionMark release;

protected override void OnMotionMark(MotionMark mark)
{
	if (mark == release)
	{
		Throw();
	}
}
```

It is called before `LateUpdate`, in the frame the action passes the mark.
