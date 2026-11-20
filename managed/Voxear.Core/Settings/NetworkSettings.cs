using System.Text.Json.Nodes;

namespace Voxear;

/// <summary>How a networked game is played, as Project Settings ▸ Network has it: fixed once the game has started.</summary>
public sealed class NetworkSettings
{
	/// <summary>How many times a second the server sends what changed.</summary>
	public int TickRate { get; private set; } = 60;

	/// <summary>The port a server listens on.</summary>
	public int Port { get; private set; } = 7777;

	/// <summary>The most players a server takes.</summary>
	public int MaxPlayers { get; private set; } = 64;

	/// <summary>The most a server sends each player, in kilobytes a second.</summary>
	public float KilobytesPerSecond { get; private set; } = 64f;

	internal static NetworkSettings Read(JsonObject from)
	{
		var settings = new NetworkSettings();
		settings.TickRate = SettingsJson.Read(from, "tick_rate", settings.TickRate);
		settings.Port = SettingsJson.Read(from, "port", settings.Port);
		settings.MaxPlayers = SettingsJson.Read(from, "max_players", settings.MaxPlayers);
		settings.KilobytesPerSecond = SettingsJson.Read(from, "kilobytes_per_second", settings.KilobytesPerSecond);
		return settings;
	}
}
