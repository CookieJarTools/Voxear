---
order: 1
---

A **Nav Agent** finds its Obj's way to where it is sent and walks it. It turns to face the way it goes, slows down to stop at the end, and goes round other agents and characters on the way. A guard, a shopkeeper, a herd of animals or a crowd in a town square are each an Obj with a Nav Agent.

## Making an Obj walk somewhere

1. Put an Obj with a model on the ground of a level with a [[Block Worlds|Block World]].
2. Choose **Add Component ▸ Physics ▸ Character Controller**, and set its **Height** and **Radius** to fit the model.
3. Choose **Add Component ▸ Navigation ▸ Nav Agent**.
4. Give it somewhere to go from a script:

```csharp
using Voxear;

namespace Game;

public sealed class Guard : Behaviour
{
	public Obj post;

	protected override void Start()
	{
		Obj.NavAgent.Destination = post.Position;
	}
}
```

Press Play. The guard walks to its post, round anything in the way. Add a [[Mob Motion]] to the same Obj and its legs walk as it goes.

## Settings

| Setting | What it does |
| --- | --- |
| Speed | How fast it walks at most, in metres a second |
| Acceleration | How quickly it speeds up and slows down |
| Turn Speed | How fast it turns to face the way it goes, in degrees a second |
| Stopping Distance | How near its destination it stops |
| Auto Brake | On, it slows down to stop at its destination. Off, it arrives at speed |
| Radius | How wide it is round its middle: how far other agents keep from it |
| Height | How tall it is: what it fits under. With a Character Controller, the controller's height counts |
| Avoidance | How carefully it goes round others: High, Low or None. Others still go round one set to None |
| Priority | From 0 to 1. Others go round an agent with a higher priority more than it goes round them |
| Moves | How it moves along its way (below) |
| Swims | On, its ways cross water too deep to stand in, and it swims across. Off, it goes round deep water, and sinks if it falls in |
| Opens Doors | On, it opens a shut door on its way and walks through. Off, a shut door is a wall to it |
| Jumps | On, it jumps a gap one or two blocks across rather than going round. Off, it goes round |

Hover over a setting in the Inspector to see what it does and its default.

## How it moves

- **Character Controller** walks it with the Obj's Character Controller, which collides with blocks, slopes and other Objs as a player does. Use it for anything that should be pushed, blocked or stood on.
- **Along the Nav** moves it along the places it can walk, with no physics at all. It goes round other agents, never through a wall, and falls where the ground drops away. It costs a small part of what a Character Controller does, so it is the one for a crowd: a town square with hundreds of people in it.
- **Not at All** finds its way and works out the velocity it wants, but leaves moving it to you: read `DesiredVelocity` from a script, or give it to a Motion Matcher.

## Going round others

Agents go round each other by choosing, each step, the velocity nearest the one they want that keeps clear of everyone near them for a couple of seconds ahead. Two agents meeting in a corridor each step aside a little. Agents also go round the player and any other Obj with a Character Controller, though those do not go round them.

## What it is doing, from a script

| Member | What it is |
| --- | --- |
| `Destination` | Where it is going, or null to stand. Set it and it finds its way there |
| `Stop()`, `Resume()` | Stands where it is, keeping its destination, and goes on again |
| `Warp(point)` | Moves it there at once, forgetting its way |
| `Arrived` | True when it is where it was sent, or has nowhere to go |
| `HasPath`, `PathPending` | Whether it has a way to walk, and whether one is being found |
| `PathStatus` | `Complete`, `Partial` (as near as it can get), or `Continues` (a long way found a part at a time) |
| `RemainingDistance` | How far it has still to go along its way |
| `Velocity`, `DesiredVelocity` | How fast it goes, and how fast it wants to |
| `NextCorner`, `Path` | The corner it is heading for, and its whole way: its corners, what it costs |
| `Speed`, `Radius`, `Moves` and the rest | Its settings |

A way it is sent along is found in the background and is there by the next fixed step, so setting `Destination` every frame costs little: the agent only looks again when the destination moves to another place, or something on its way changes.

```csharp
// A dog that follows its owner, and sits once it is close.
var agent = Obj.NavAgent;
if (Vector3.Distance(owner.Position, Obj.Position) > 3f)
{
	agent.Destination = owner.Position;
}
else
{
	agent.Stop();
}
```

## Playing over the network

Only the machine that has authority over an Obj moves its Nav Agent: the server, for the creatures of a game. Other players see it move as it is sent over the network, and its Mob Motion walks it as it goes.

A player's script can still send one. Setting `Destination`, calling `SetDestination`, `Stop` or `Resume` on an agent the server moves sends the request to the server, which finds the way and walks it. The server takes such a request for an Obj no player owns, or for the player's own, and drops it for another player's.
