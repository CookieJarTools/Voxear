---
order: 7
---

Nav Agents and Block Mobs walk through [[Portals]] as a player does. Nothing needs setting up: every open Portal that leads somewhere is a way to them.

## A Portal as a short way

Within one dimension, a Portal is a link from just in front of its opening to just out of the Portal it leads to. A search takes it where it makes the way shorter, as it takes a [[Nav Links|Nav Link]]: a pair of Portals either side of a wall with no way round is how an agent gets past it. An agent reaching the Portal walks into its opening, and the Portal takes it through.

Agents go in at a Portal's front, the side its opening faces, and come out of the front of the one it leads to. A Portal that is shut, or whose far side is not made yet, is no way, and an agent walking to one that shuts finds its way again.

Turn on **Gizmos ▸ Navigation** in the Scene view to see each Portal's way in as a purple arrow through its opening.

## Into another dimension

An agent is sent into another dimension with a dimension as well as a point:

```csharp
// A companion following the players into a dungeon.
companion.NavAgent.SetDestination(leader.Position, leader.Dimension);
```

It walks to the Portal into that dimension that makes the shortest way in a straight line, goes through, and walks on from where it comes out. It reports **Arrived** only once it is in that dimension, where it was sent. Where no Portal in its own dimension leads into the one it is sent to, it stands and its way's status is **None**. Agents go through one Portal into another dimension at a time; one sent two dimensions away goes no further than the first.

`DestinationDimension` says which dimension it is going to. Setting `Destination` alone sends it within its own.

From a terminal, `nav_agent.set_destination` takes a `dimension`, by its number as `dimension.list` gives it or its `.dimension` file.
