---
order: 7
---

Some guns come with animation of their own, in an animation pack. Such a pack usually has:

- **The gun**: one skinned model whose moving pieces, the bolt, the trigger, the rounds, are bones, with clips of its own that move them.
- **Arms clips**: clips of a character's arms holding the gun, for each thing they do with it. The gun rides a bone of its own, often called `gun_carrier`, and the hands are on it.
- **Clips in pairs**: an arms clip and a gun clip of the same name and length, played together, as working the bolt is both the arms' moves and the bolt's.

Voxear holds such a gun by its clips, in your character's own hands. The clips say where the gun goes and where each hand holds it; your character's body reaches there, as it reaches for any gun. So the clips hold the gun alike on any character with arms, hands and fingers, whatever its skeleton names them, and its legs and back stay its own.

## Bringing a pack in

Bring the gun's model and the model with the arms clips into your project as glTF or FBX files (see [[Models]]). Keep each gun's clips in the gun's own file, or in a file of the same skeleton: a clip from another file plays on the gun's bones that sit where the clip's do.

A pack's normal maps may be made with their green pointing down, which lights the surface as if from below. Turn on **Flip Green** in each normal map's import settings.

## Setting up the gun

1. Put the gun's model in your character's prefab and add a **Gun**, as any gun has (see [[Setting Up a Gun]]). Pick its **Kind**.
2. Put its sockets on the gun's bones: right-click **Rear Sight**, **Front Sight** and **Muzzle** in turn and choose **Put on** and the bone named for it. These say where aiming brings the eye and where shots leave.
3. Give it a new **Gun Handling** in its **Handling** field.
4. Put attachments, such as a scope or a bayonet, on the gun's bones with a **Bone Attachment** (see [[Bone Attachments]]).
5. For a case thrown out as the bolt is worked, put the pack's case model under the gun as a **Gun Part** whose **Role** is **Case**, at the port.

A gun clip may move a round or case of its own out of the port. Leave the Case part out for such a gun, or it throws out two.

## Holding it by its clips

Select the gun's handling, and in its **Clips** part:

- Put the arms clips the gun is held by in **Idle**, **Walk**, **Run** and **Sprint**, and set **Walk Speed** and **Run Speed** to how fast your character walks and runs.
- Put the clips that start a run and a sprint, if the pack has them, in **Run Start** and **Sprint Start**. Setting off from standing or walking, the start plays first, and the run or sprint carries on from its first frame, where the start leaves off. Without one, the hold blends into the run.
- Put the gun's clip it stands still in, if it has one, in **Rest**.
- Put the shot's arms clips in **Fire** and **Aimed Fire**, and the gun's own in **Gun Fire**.
- **Carrier** finds the bone the gun rides by itself when it is called `gun_carrier`. For another, pick it from the list. If the gun sits off that bone in the pack, as when it is attached with an offset, set **Carrier At** and **Carrier Turn**.

**Eyes** is where the camera the clips were filmed from sat, from the clips' head bone: right, up and forward, in metres. At 0 the gun is framed for a camera at the head bone itself, but packs are often filmed from a little lower. Compare your view with the pack's own videos:

- The gun shows lower than in the videos: move **Eyes** down.
- The gun shows smaller and farther off: move **Eyes** forward.

The camera's **Field of view** matters as much. Packs are usually filmed 90° across, which on a wide screen is about 59° on a Camera, whose field of view is measured from top to bottom (see [[Cameras]]).

**Camera** says where your character looks from while the clips hold the gun:

| Camera | What you see |
| --- | --- |
| **As Filmed** | The camera is where the pack's camera was, as far from your character's shoulders as it was from the clips', scaled to its arms, and the gun comes as much nearer or further. The arms hold the gun bent as the clips' are, and it shows as in the pack's videos, a little larger for shorter arms. |
| **At the Eyes** | The camera is at your character's eyes. Those sit higher above the shoulders than a pack's camera usually does, so the gun shows lower and nearer, and a short arm may not quite reach it. |

Either way, the camera is worked out on your character standing at rest and then stays put on it: a pack films its clips from a camera held still, and your character's walk and run don't carry the camera up and down with them. The pack's camera is often lower than real eyes, near the neck, so **As Filmed** looks from a little lower than your character's eyes. Put the gun away and the camera goes back to the eyes.

Looking up or down, the gun turns with the view but swings about the shoulders rather than the eyes, so the arms reach it just as well however far the view tips. It moves a little in the view as it does: up and nearer when looking far up.

Your character's arms take the clips' own pose, bone by bone, as its walk takes the walk clips': the upper arm, forearm, wrist and every finger turn as the clips turn them, twist included. Each hand is then brought the rest of the way onto the gun, its knuckles where the clips' knuckles are, only as far as your character's build needs, with the elbow where the clips' pose has it. So a hand with a longer palm still closes on the bolt handle or the grip as the clips' hand does. Nothing moves the gun from where the clips have it but the layers below; an arm that can't quite reach falls short of it rather than the gun coming to the hand.

A pack's clips seldom aim. Aiming brings the gun's sights onto the camera's line, as it does for any gun, but no nearer the eye than the clips hold it: the gun comes across rather than back, as the pack's own camera would come down onto the sights. So the hands stay as far ahead of the eye as the clips have them, and the elbows bend as the clips bend them. Set the sockets carefully:

- Put **Rear Sight** and **Front Sight** on the tops of the sights, not on their bones, which often sit lower, level with the action.
- **Eye Relief** only matters when it is more than the clips' own distance: it then holds the gun further out. Raise it for a pack whose camera would end up inside the gun's action.

## Layers on top

The clips are the base. On top of them go layers of the Gun Holder's own, each as much as the **Clips** part says: 1 as for any gun, 0 not at all. The gun and both hands move together with each, so the hands stay where the clips have them on it.

| Setting | What it adds | As a pack gun starts |
| --- | --- | --- |
| **Look Sway** | The gun lags the view as it turns. | 0.3 |
| **Move Sway** | The gun trails your character's moves. | 0.3 |
| **Bob** | A kick at each footfall. | 0.3 |
| **Breathing** | A slow rise and fall, and a faint tremor. Packs' idle clips usually breathe of their own. | 0 |
| **Head Bob** | The camera itself bobs with the steps. | 0 |

The Gun Holder's own **Sway**, **Bob** and **Breathing** scale these again, as they do for every gun. A shot's clip kicks the gun as it plays, and the camera kicks too. Up at the eye, where the clips don't hold the gun, it sways, bobs and breathes as any gun does. While an action plays from clips, the view doesn't glance down at the work: the clips bring it into view.

To see the clips as they were made, tick the Gun Holder's **Base Only**: nothing goes on top, not the layers, the camera's kick or the pull back from walls. Compare your view with the pack's own videos this way, then untick it.

Sprinting is your game's to say, as aiming is:

```csharp
namespace Soldiers;

public sealed class Soldier : Behaviour
{
	protected override void Update()
	{
		var holder = Obj.GunHolder;
		holder.Aiming = Input.Held("Aim");
		holder.Sprinting = Input.Held("Sprint");
	}
}
```

## Its actions from clips

For each action the pack has clips for, set its **Mode** to **Clips** and give it a part for each clip pair (see [[Gun Handling]]). Packs have no marks of their own saying when things happen, so add them yourself: play the gun clip and note when the case leaves, when a round goes in and when the bolt shuts. For a bolt-action rifle that loads a round at a time:

| Action | Parts | Marks |
| --- | --- | --- |
| Cycle | working the bolt, When **Chamber Filled** | **Eject** as the case leaves, **Chamber** as the bolt goes home |
| | working the bolt with nothing to throw out, When **Chamber Empty** | **Chamber** |
| Reload | the first round: bolt open, a round in | **Open**, **Round In** |
| | one more round, Repeat **Until Full** | **Round In** |
| | the bolt shut | **Chamber**, **Close** |

A pack's action clips often snap the hand back onto the grip in their last few frames. As an action ends on such a clip, the hold lets go of it as the hand sets off, and the hand eases back over **Hand Return**, in seconds (0.35 to start with). Raise it for a slower return. A clip whose hand comes back slower than that plays to its end as made.

An action the pack has no clips for still plays as built, on the same gun. Give the gun's bolt a Gun Part on its bone so it moves there (see [[Setting Up a Gun]]).

Extra clips, such as inspecting the gun, drawing and putting it away, are actions of the handling's own, played from clips, which scripts do with a Gun Holder's **Do**.

## Skins

A pack's skins are sets of materials, one for each of the gun's parts. To give a gun a skin, list its model's parts in the Hierarchy and give each part the skin's material (see [[Models]]).
