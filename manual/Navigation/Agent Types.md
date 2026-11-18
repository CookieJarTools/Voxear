---
order: 9
---

An **agent type** is a kind of thing finding its way: a person, a horse that keeps out of water, an ogre too big for doorways. A project has a **Default** type and four of its own, set in **Project Settings ▸ Navigation**. Each [[Nav Agents|Nav Agent]] picks its type in its **Agent Type** dropdown.

## Making one

1. Open **Project Settings ▸ Navigation** and find **Agent Type 1**.
2. Give it a **Name**, as Horse. An unnamed type is offered as Agent Type 1.
3. Set its sizes, and what roads, water and swimming cost it.
4. On the horse's Nav Agent, pick **Horse** under **Agent Type**.

## Settings

| Setting | What it does |
| --- | --- |
| Name | What the project calls it, as Nav Agents offer it |
| Height | How tall it is: the room it needs over a floor |
| Radius | How wide it is round its middle: how far its ways keep from walls and edges |
| Step | How high a step it walks up |
| Slope | The steepest slope it walks up, in degrees |
| Road cost, Water cost, Swim cost | What a block of each costs it to cross, in place of the project's **Costs**. 0 takes the project's |

The **Default Agent** section sets the Default type's sizes; its costs are the project's own **Costs**.

## Costs

A type's costs change only its own ways. With a Water cost of 50, a horse goes far round a ford the Default type wades straight across. Areas the project names itself cost every type the same.

## Sizes

**Bake Navigation** bakes a [[Levels of Models|level of models]] once for the Default type and once for each type the project has named, each for its own sizes, all into the level's one `.nav` file. An agent walks the mesh baked for its type: an ogre too wide for a doorway goes round by a wider gap, where a person walks through. An agent whose type was not baked for, as one named since the last bake, walks the Default type's. [[Nav Obstacles]] cut every type's mesh. The Navigation gizmo shows the Default type's.

In a [[Block Worlds|Block World]] a Nav Agent's own **Height** and **Radius** say what it fits under and through, whatever its type. One wider than a block, as an ogre 0.8 m round its middle in blocks a metre across, needs two blocks side by side all the way: it goes round a doorway a block wide by a wider gap, and climbs no vines. Worlds keep their ways for walkers two blocks wide ready, so their searches cost what any does.

## From a script

```csharp
// The ogre is the project's first own type.
ogre.NavAgent.AgentType = NavAgentType.Own(1);
```
