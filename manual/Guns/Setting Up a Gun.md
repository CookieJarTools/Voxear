---
order: 1
---

A gun is a model with a **Gun** component on it. Put the gun's model in the character's prefab, under the Obj with the [[First-Person View]], and add **Add Component ▸ Guns ▸ Gun**. Point the model's barrel along its +Z, its top up, and put its origin at the grip.

## Kind

**Kind** says how the gun works, and choosing one fills in a typical gun's numbers and sockets:

| Kind | Fed from | Worked by |
| --- | --- | --- |
| Bolt-Action Rifle | 5-round stripper clips | lifting, pulling, pushing and turning the bolt |
| Semi-Auto Rifle | an 8-round en-bloc clip | itself; the clip pings out when empty |
| Open-Bolt SMG | a 32-round magazine | itself; cocked by hand after a reload |
| Slide Pistol | a 7-round magazine | itself; the slide locks back on empty |
| Toggle Pistol | an 8-round magazine | itself; the toggle locks open on empty |
| Belt-Fed MG | a 50-round belt under a cover | itself; cocked by hand |
| Pump Shotgun | a 5-shell tube | pumping the fore-end |
| Top-Break Revolver | a 6-round cylinder | itself; tipped open to load |

**Mass** and **Length** decide how it sways and settles: heavier, longer guns lag the view more and take longer to come to rest. **Fire Rate** is rounds a minute, and **Recoil** how hard each shot kicks, 1 for a full-power rifle round.

## Sockets

Sockets are places on the gun, in the model's own space. Select the gun and they show in the Scene view:

- **Grip** and **Support**: where the shooting hand's palm goes, and the other hand's. **Grip Turn** and **Support Turn** turn the hands there if they hold it at the wrong angle. On a long gun, the other hand slides back along the barrel when it can't reach the support.
- **Rear Sight** and **Front Sight**: the line the gun is aimed along. **Eye Relief** is how far the eye is from the rear sight when aiming, and **Aim FOV** the field of view then.
- **Muzzle**, where shots leave; **Feed**, where magazines, clips and rounds go in; and **Port**, where cases come out.

## Handling

**Handling** picks the gun's [[Gun Handling]]: its own poses and actions, worked on in [[The Gun Editor]] with **Open in Gun Editor** at the top of the Gun. With none, the gun is held and handled as its kind's built-ins have it.

## Parts

The gun's moving pieces are Objs of their own under the gun's model, each with a **Gun Part** (**Add Component ▸ Guns ▸ Gun Part**). Its **Role** says what it is, and fills in how such a part usually moves:

- **Bolt**, **Slide**, **Toggle** and **Pump**: worked by hand, and by the gun itself as it fires.
- **Magazine** and **Clip**: taken out and put in. A clip shows only while a hand has it.
- **Cover** and **Barrel**: a machine gun's top cover, and a top-break revolver's barrel.
- **Case**: a spent case at the port. It is hidden, and copied for each case the gun throws out.

**Moves** says whether the part slides or turns, along or about its **Axis**, by its **Amount** in metres or degrees, through its **Pivot**. **Then Pull** gives a turn-bolt its pull back after its lift. **Handle** is where a hand takes hold of it. Select a part and the path its handle moves along shows in the Scene view.

The Gun's section in the Inspector says when a part its kind is worked by is missing. Without one, the hand still goes through the motions, with nothing moving under it.
