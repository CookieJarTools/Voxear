---
order: 7
---

The **Gun Editor** is where a [[Gun Handling]] is worked on, with a character holding the gun in a view of its own. Open it from **Window ▸ Gun Editor**, by double-clicking a `.gunhandling` file, or with **Open in Gun Editor** on a Gun in the Inspector, which shows that gun in the character's hands.

## The top bar

- **Handling**: the Gun Handling open, and **+** to make a new one.
- **Character**: who holds the gun. Any prefab with a Gun Holder at its top can, holding its own gun, or a stand-in rifle when it has none.
- **First Person**, **Side**, **Front** and **Orbit**: how the view looks. First Person is what the player sees, which at the hip is little more than the barrel; the views from outside show both hands. They look at where the gun is held and stay put, so what you change shows as a move. Orbit turns as you drag and comes nearer with the wheel.
- **Undo** and **Redo**, which Ctrl+Z and Ctrl+Y do too while the pointer is over the Gun Editor.

The character is made for the editor alone, far from your level, and never saved. It shows while editing. While the game plays, the timeline and details still work, and their changes reach the game as they are made.

## What the character does

- **Hip** and **Aim**: the gun held at the hip, or raised to the eye.
- **Fire**: a shot, from the ammo state picked.
- The actions: the built-in ones, then the gun's own. **+** adds a new action of its own. An action that does nothing from the ammo state picked is greyed out, as Reload is for a full gun.
- **Play**, **Loop** and the speed, from 1× down to 0.1×.
- **Full**, **Half**, **Empty** and **Dry**: what is in the gun as actions start, so a reload from empty and a top-up can each be seen.
- **Walk**: the character walks on the spot, to see the gun bob and the hands stay on it.

An action shown at a time looks the same every time, and throws nothing away.

## The timeline

The timeline under the view shows the action's beats in a lane for each hand, each beat a block, fainter while the hand reaches and solid while it works. Under them are the parts each beat moves or carries, and what each does as it ends: a case ejected, rounds added, a round chambered. Beats that repeat are bracketed with how many times they play. A built-in beat with offsets has a dot.

- Press or drag along the ruler to see any moment.
- Click a beat to see its details.
- Drag a beat's right edge to make it longer or shorter, or drag the beat along to start it later or sooner.

## The details

With nothing picked, the details show how the gun is held. With an action showing, they show the action, and with a beat picked, the beat. Drag a label left or right to change its number. A changed built-in value has a button beside it that puts it back.

Built-in actions show **Make Editable Copy** and **Reset to Built-in**. Actions of the gun's own show **Add Beat** and **Delete Action**, and their beats **Duplicate**, **Delete**, **Earlier** and **Later**.

## Posing the hands

Each hand has a ring in the view, and the gun a cross:

- Drag a hand's ring and the hand slides over the gun on the side it is on, even the side away from you. Its palm stays on the surface under the pointer and its fingers close round the shape there, just as they will in the game. Hold **Alt** to move it freely instead, as to a pouch. A hand dragged further than the arm reaches stays short of the pointer.
- A hand working in the beat showing moves that beat's hold. At rest, it moves where that hand holds the gun.
- Drag the cross to move where the gun is held: for the action showing, at the eye while aiming, or else at the hip.

## Problems

The strip under the timeline lists what is wrong: parts the gun's kind needs that it has no Obj for, a hand more than a centimetre into the gun, sights off the eye's line while aiming, and offsets that do nothing, with a button to remove those. A palm on the gun and fingers closed round it touch it all over, which is not a problem.
