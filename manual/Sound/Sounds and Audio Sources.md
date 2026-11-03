---
order: 1
---

Drop a sound file into your project and it is ready to play. Voxear reads four kinds:

| File | Good for |
| --- | --- |
| OGG Vorbis (`.ogg`) | Almost everything: small files that sound good |
| WAV (`.wav`) | Short effects you are still working on. WAV files are large |
| FLAC (`.flac`) | Music you want to keep exactly as recorded, at about half the size of a WAV |
| MP3 (`.mp3`) | Sounds you already have as MP3 |

The Project panel shows a sound with a speaker icon. Select one, and the Inspector shows how long it is, how many channels it has (mono or stereo) and its sample rate. **Play** plays it in the editor, so you can check it is the right one. The game's volumes don't change how loud it plays there.

## How a sound loads

The Inspector's **Import Settings** for a sound are:

| Setting | What it does |
| --- | --- |
| Load | **Whole** decodes the sound into memory when it loads, so it starts instantly and costs nothing while it plays. **Streamed** keeps the file as it is and decodes it a little at a time as it plays, which takes a fraction of the memory. **By length** decodes sounds up to 10 seconds long whole, and streams longer ones |
| Volume | Makes the sound louder or quieter wherever it plays: a quick fix for a file recorded too loud or too quiet. 1 leaves it as recorded |

Click **Apply** to keep your changes. They are saved in the sound's `.meta` file, beside it.

**By length** suits most projects. Effects are short, so they load whole and play the moment they are asked for. Music and long ambience stream, so a three-minute song takes a few megabytes rather than thirty.

## Playing a sound from an Obj

An **Audio Source** plays a sound from the Obj it is on: a crackling fire, a radio, a waterfall.

The quickest way to make one is to drag a sound from the Project panel into the Scene view or the Hierarchy. You get a new Obj named after the sound, with an Audio Source that plays it. In the Scene view, the Obj lands where you let go. In the Hierarchy, letting go on an Obj puts the new one inside it. Undo takes it away again.

You can also make one with the Hierarchy's **+** button and **Audio Source**, or give an existing Obj one with **Add Component**. Then pick its sound, or drag a sound from the Project panel onto its **Sound** field. The Hierarchy shows an Obj whose only job is playing a sound with a speaker icon.

| Setting | What it does |
| --- | --- |
| Sound | The sound it plays |
| Volume | 0 is silent, and 1 plays the sound as it is |
| Pitch | 1 plays the sound as it is. 2 plays it an octave higher and twice as fast, and 0.5 an octave lower and half as fast |
| Loop | Starts the sound again from the beginning each time it ends |
| Play on start | Starts playing as soon as the level starts, or as the Obj is made while the game runs |
| Bus | Effects or Music: which volume setting it follows (see below) |
| Mode | 3D or 2D (see below) |
| Min distance, Max distance | How far a 3D sound carries, in metres (see below) |
| Doppler level | How much a 3D sound's pitch changes as it moves towards or away from the listener (see below) |

Audio Sources only play while the game runs. In the editor, **Pause** holds every sound the game is playing, and **Stop** silences them.

In the Scene view, each Audio Source shows a small speaker where its Obj is. While the game runs, the speakers of the sources that are playing turn blue, so you can see what is making a sound. Select a 3D source to see how far it carries: a blue sphere at its min distance and a fainter one at its max. **Audio Sources** in the Gizmos menu hides the speakers.

## 3D and 2D sounds

A **3D** sound comes from where its Obj is. It gets quieter as the listener moves away, and it comes from the left or right, depending on where the Obj is. A 3D sound follows its Obj as it moves.

- Within **Min distance**, the sound plays at full volume.
- Further out, it fades the way sound does outdoors: half as loud each time the distance doubles.
- At **Max distance**, it has faded to nothing.

Give a loud thing a larger min distance: a waterfall might be at full volume within 5 metres and carry 60, and a ticking clock within half a metre and carry 8. A sound out of reach keeps playing silently, so it is in the right place when the listener comes back.

A 3D sound moving towards the listener sounds higher, and one moving away sounds lower, as a passing car's engine drops in pitch as it goes by. This is the Doppler effect, and it comes from how fast the sound's Obj and the listener move. **Doppler level** sets how strong it is: 1 is as in the world, 0 turns it off, and higher makes it stronger, which can help a slow game feel fast.

A **2D** sound plays the same wherever the listener is. Use 2D for music, for interface sounds, and for anything the player should hear from everywhere.

## Headphones

Panned between two speakers, a 3D sound can only come from the left or the right. With headphones on, Voxear plays 3D sounds through Valve's **Steam Audio**, which gives each one the small differences in time and tone that a real head hears. A sound can then come from above, below or behind as well, and a player wearing headphones can tell where it is without looking.

It suits headphones and not speakers, so it is off unless you turn it on. Most games put a **Headphones** choice in their sound options, as [[Playing Sounds from Scripts]] shows. To start with it on, add this to your project's `voxear.toml`:

```toml
[audio]
headphones = true
```

A build takes Steam Audio's library with it, about 40 MB, beside the game's program, and its licence in `Third Party/Steam Audio`. If your game never uses headphones, leave it out with `steam_audio = false` under `[audio]`. Without Steam Audio, sounds pan as they do with headphones off.

## Who hears the game

The **listener** is where the game hears from. It is the camera the game looks through, unless an Obj has an **Audio Listener** component (from **Add Component**). In a third-person game, put an Audio Listener on the player character, so sounds come from around the character rather than from behind it, where the camera is. If more than one Obj has an Audio Listener, the first one listens.

## Music and effects volumes

Every sound plays through one of two **buses**: **Effects**, which sounds use unless you choose otherwise, or **Music**. A settings menu turns each one up or down, and a **master volume** turns everything up or down together. [[Playing Sounds from Scripts]] shows how.

## Sounds in a build

A build packs every sound that a scene, a prefab or a script field uses, with its import settings. Your code can also load a sound by its path. In that case, tell the build with `[ContentRoot]`, as [[What Goes into a Build]] shows.

Up to 256 sounds can play at once. Past that, a new sound replaces the oldest effect that isn't looping.
