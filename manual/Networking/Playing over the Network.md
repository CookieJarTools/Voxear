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

`Net.LocalPlayer` is the player this machine plays, if any: a host's own, or a client's once it has joined.

## Ticks

While networked, the game moves in **ticks**: its fixed steps, 60 a second unless the project says otherwise. Each tick, every client sends what its player did, with the last few ticks again in case one is lost. The server runs each player's input for that tick. A client keeps a tick or two ahead of the server, so its input arrives in time, by running a little faster or slower when it drifts. `Net.Tick` is the tick now, and `Net.RoundTrip` how long a message takes a client to the server and back.

## Network settings

`[network]` in `voxear.toml` sets the tick rate, the port, how many may play and how much each end sends:

```toml
[network]
tick_rate = 60
port = 7777
max_players = 64
kilobytes_per_second = 64
```

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
