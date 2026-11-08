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
- Put the gun's clip it stands still in, if it has one, in **Rest**.
- Put the shot's arms clips in **Fire** and **Aimed Fire**, and the gun's own in **Gun Fire**.
- **Carrier** finds the bone the gun rides by itself when it is called `gun_carrier`. For another, pick it from the list. If the gun sits off that bone in the pack, as when it is attached with an offset, set **Carrier At** and **Carrier Turn**.

Clips made for a camera at the head bone, as most are, look right with **Eyes** left at 0. If the gun shows too low or too far away, the pack's camera sat in front of the head: move **Eyes** up and forward until it shows as the pack's own videos do.

A pack's clips seldom aim. Aiming brings the gun's sights to the eye from wherever the clips hold it, as it does for any gun, so set the sockets carefully. The gun still sways as the view turns and kicks as it fires, and the hands go with it. Where your character's arms are shorter than the pack's, the gun comes nearer and the other hand slides back along it.

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

An action the pack has no clips for still plays as built, on the same gun. Give the gun's bolt a Gun Part on its bone so it moves there (see [[Setting Up a Gun]]).

Extra clips, such as inspecting the gun, drawing and putting it away, are actions of the handling's own, played from clips, which scripts do with a Gun Holder's **Do**.

## Skins

A pack's skins are sets of materials, one for each of the gun's parts. To give a gun a skin, list its model's parts in the Hierarchy and give each part the skin's material (see [[Models]]).
