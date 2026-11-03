---
order: 2
---

Give a behaviour a `Sound` field, pick the sound in the Inspector, and play it with `Audio.Play`:

```csharp
namespace MyGame;

public sealed class Door : Behaviour
{
	public Sound Creak;

	public void Open() => Audio.Play(Creak, at: Obj.WorldPosition);
}
```

A `Sound` field loads with its Obj, before `Awake`, so the sound is ready the first time it's needed.

## Playing a sound once

`Audio.Play` plays a sound once, and there are two kinds:

```csharp
// 2D: the same wherever the listener is, as for a button click.
Audio.Play(Click);

// 3D: at a place in the world, quieter further away.
Audio.Play(Explosion, at: Obj.WorldPosition);
```

You can set the volume and pitch of a sound as you play it. A slightly different pitch each time stops a footstep from sounding the same over and over:

```csharp
Audio.Play(Footstep, at: Obj.WorldPosition, volume: 0.6f, pitch: Random.Range(0.9f, 1.1f));
```

A 3D sound is at full volume within 1 metre and fades to nothing at 50 metres. Pass `minDistance` and `maxDistance` to change how far it carries. See [[Sounds and Audio Sources]] for how the fading works.

A sound played this way stays where it started. For a sound that should follow something as it moves, such as an engine, put an Audio Source on the Obj instead.

## Stopping a sound early

`Audio.Play` returns the sound it started. Keep it if you might stop the sound before it ends, or change it while it plays:

```csharp
var alarm = Audio.Play(Siren, volume: 0.8f);

if (alarm.IsPlaying)
{
	alarm.Stop();
}
```

You don't need to keep it otherwise. Once the sound has ended, `IsPlaying` is false, and stopping it does nothing.

## Audio Sources

An Obj's Audio Source is `Obj.AudioSource`:

```csharp
public sealed class Radio : Behaviour
{
	public void Toggle()
	{
		var radio = Obj.AudioSource;
		if (radio.IsPlaying)
		{
			radio.Pause();
		}
		else
		{
			radio.Play();
		}
	}
}
```

| Member | What it does |
| --- | --- |
| `Play()` | Plays the sound from the beginning, or carries on from where it was paused |
| `Pause()` | Holds the sound where it is |
| `Stop()` | Stops the sound; the next `Play` starts from the beginning |
| `IsPlaying` | Whether it is playing |
| `Sound`, `Volume`, `Pitch`, `Loop`, `PlayOnStart` | The settings the Inspector shows |
| `Is3D`, `MinDistance`, `MaxDistance`, `DopplerLevel`, `Bus` | The same |
| `Occlusion`, `OcclusionRadius`, `Transmission` | The same, with `SoundOcclusion.Off`, `Raycast` or `Volumetric` |

`Play`, `Pause` and `Stop` happen at the end of the frame, together with everything else that changes the sound. `IsPlaying` changes straight away, so you can check it on the next line.

An Obj made in code can be given an Audio Source with `AddAudioSource`:

```csharp
var hum = Obj.AddAudioSource(Hum);
hum.Loop = true;
hum.Play();
```

`AudioSource.IsValid` is false when an Obj has no Audio Source.

## Playing through another Obj's Audio Source

A field of type `AudioSource` takes another Obj that has an Audio Source. Pick it in the Inspector, which lists only the Objs that have one, or drag it there from the Hierarchy:

```csharp
public sealed class Doorbell : Behaviour
{
	public AudioSource Chime;

	public void Ring() => Chime.Play();
}
```

The scene saves which Obj the field points at, as it does for an `Obj` field.

## Music

For music, put a 2D Audio Source on an Obj that stays loaded. Set its bus to **Music** and turn on **Loop**. Its sound streams if it is long, as music is, so it takes little memory.

You can also start music from code:

```csharp
var song = Audio.Play(Theme, bus: AudioBus.Music);
```

## Volume settings

A settings menu can change three volumes, each from 0 for silent to 1 for full:

```csharp
Audio.MasterVolume = 0.8f;  // everything
Audio.MusicVolume = 0.5f;   // what plays through the Music bus
Audio.EffectsVolume = 1f;   // what plays through the Effects bus
```

When you stop playing in the editor, the volumes go back to full. To keep the player's choice between runs, save it with [[Saving the Game]] and set it again when the game starts.

A player wearing headphones hears 3D sounds better with **Headphones** on (see [[Sounds and Audio Sources]]). `Audio.HeadphonesAvailable` says whether Steam Audio is there to do it, so your menu can leave the choice out when it isn't:

```csharp
if (Audio.HeadphonesAvailable)
{
	var headphones = menu.Add(new Toggle("Headphones", Audio.Headphones));
	headphones.Toggled += on => Audio.Headphones = on;
}
```

Each time the game starts, Headphones starts as your project's `[audio] headphones` setting has it.

## Loading a sound by its path

When a field isn't practical, load a sound by its path in the project:

```csharp
var thunder = Sound.Load("Sounds/Thunder.ogg");
```

The first time, the file is read from disk, so load sounds when the level starts, not in the middle of the action. A `Sound` field is the cheaper choice: it loads with the level. A `Ref<Sound>` field names a sound without loading it, until you call `Load()`. A build only includes a sound loaded by path if `[ContentRoot]` names it (see [[What Goes into a Build]]).

A sound also tells you about itself: `Length` in seconds, `Channels`, `SampleRate`, and whether it `IsStreamed`.
