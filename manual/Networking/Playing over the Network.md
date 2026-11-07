---
order: 1
---

Networked play has one machine in charge, the **server**, and players who join it, the **clients**. The server runs the game. Each client sends the server what its player presses, and the server decides what happens. A **host** is a server that is also a player, as when a friend starts a game for others to join.

## Hosting and joining

Start networked play from your scripts:

```csharp
Net.StartHost();

// On another machine:
Net.Connect("192.168.1.20");
```

`Net.StartServer()` serves the game without a player of its own, as a dedicated server does. Each takes a port, or uses the project's. `Net.Connect` takes an address with or without a port, and a name such as `game.example.com` works too. `Net.Disconnect()` ends networked play and tells the other end.

`Net.Connected` and `Net.Disconnected` tell a client how joining went. `Net.PlayerJoined` and `Net.PlayerLeft` tell everyone about players coming and going:

```csharp
Net.PlayerJoined += player => Log.Info($"{player} joined");
```

`Net.Role` says what this machine is: `None`, `Server`, `Host` or `Client`. `Net.IsServer` is true on a server and on a host, which is how code that only the machine in charge should run checks.

## Players and their input

`Net.Players` lists everyone playing. On the server, each player's input reads as they pressed it on their own machine, through the actions in your project's input actions:

```csharp
[UpdateIn(Stage.FixedUpdate)]
public sealed class Movement : SystemBase
{
	protected override void Update()
	{
		foreach (var player in Net.Players)
		{
			var move = player.Vector2("Move");
			if (player.Pressed("Jump"))
			{
				Jump(player);
			}
		}
	}
}
```

Read players' input in FixedUpdate. A press shorter than a tick is still seen, once. While a player's input is late, their last input carries on, without pressing anything again; after a tenth of a second, they let go of everything.

### Aim

Where a player looks goes with their input too, as two numbers of your choosing, such as a turn and a tilt in degrees. Their machine sets `Net.Aim` as the mouse moves, and the server reads `player.Aim`, as it was on the tick they pressed:

```csharp
// On the player's machine:
yaw += Mouse.Delta.X * sensitivity;
pitch = Mathf.Clamp(pitch + Mouse.Delta.Y * sensitivity, -85f, 85f);
Net.Aim = new Vector2(yaw, pitch);

// On the server, in FixedUpdate:
var look = Quaternion.Euler(player.Aim.Y, player.Aim.X, 0f);
```

Mouse movement is better sent as where it ended up than as an action: a lost tick of movement would leave the aim off for good. When input is lost, the aim stays where it last was.

`Net.LocalPlayer` is the player this machine plays, if any: a host's own, or a client's once it has joined.

## Ticks

While networked, the game moves in **ticks**: its fixed steps, 60 a second unless the project says otherwise. Each tick, every client sends what its player did, with the last few ticks again in case one is lost. The server runs each player's input for that tick. A client keeps a tick or two ahead of the server, so its input arrives in time, by running a little faster or slower when it drifts. `Net.Tick` is the tick now, and `Net.RoundTrip` how long a message takes a client to the server and back.

## Network settings

`[network]` in `voxear.toml` sets the tick rate, the port, how many may play, how much each end sends, and who decides what players' own Objs do ([[Shared Authority|server or shared]]):

```toml
[network]
tick_rate = 60
port = 7777
max_players = 64
kilobytes_per_second = 64
authority = "server"
```

## Testing in the editor

The button beside Play, **Offline** until you change it, picks how Play plays:

- **Host**: the editor runs the game and plays it, and test players join it.
- **Dedicated server**: a server starts without a window, and the editor joins it as the first player, then the test players.

Below that, pick how many **test players** join, up to seven. Each is a game of its own in a window titled "Player 2", "Player 3" and so on, which you can play side by side with the editor's. Their log lines come into the Console with their names in front. Pick a **network** too: Good, Average or Bad play through a network that delays, jumbles and loses packets as a real one would. Stop closes the test players' windows and the server.

The **F1** stats show the session in each window: the round trip, how many packets are lost, and what goes each way.

## From the command line

`vox run` plays the project networked:

```
vox run MyGame --server
vox run MyGame --host
vox run MyGame --connect 127.0.0.1:7777
```

`--port` picks the port a server or host listens on. `--network good`, `average` or `bad` plays through a simulated network that delays, jumbles and loses packets as a real one would, so you can see how your game holds up. A built game takes the same `--server`, `--host`, `--connect` and `--port`.

## Keeping a server to yourself

Every packet is encrypted, and a player can only join with a connect token sealed with the server's key. By default the key is one every copy of your game works out the same, so anyone with the game can join any server by its address, which suits playing with friends and testing.

A public server should have a key of its own: set `VOXEAR_NET_KEY` to 64 hex digits where the server runs, and have your own service, such as a matchmaker, hand players tokens sealed with that key. A token names the servers its player may join, expires after a while, and can carry 256 bytes of your own, such as which match and team the player is in.
