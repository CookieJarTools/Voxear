using System;
using System.Collections.Generic;
using System.Linq;

namespace Voxear.Tests.Reloading;

/// <summary>
/// A behaviour holding one of each kind of value a reload has to carry. The reload tests load this assembly twice,
/// so its classes stand for a script's old build and its new one.
/// </summary>
public sealed class Keeper : Behaviour
{
	public int Score { get; private set; }

	public bool HasHandler => onHit != null;

	public object? Crate => crate;

	private Mood mood;
	private Cargo cargo;
	private Crate? crate;
	private Crate? sameCrate;
	private List<Crate> crates = new List<Crate>();
	private Mood[] moods = Array.Empty<Mood>();
	private Dictionary<string, Crate> byLabel = new Dictionary<string, Crate>();
	private Keeper? friend;
	private List<Behaviour> others = new List<Behaviour>();
	private Action? onHit;
	private float started = 1f;

	/// <summary>Sets every value to something other than where the class starts it.</summary>
	public void Fill(Keeper other)
	{
		Score = 7;
		mood = Mood.Cross;
		cargo = new Cargo { Weight = 2.5f, Mood = Mood.Sleepy };
		crate = new Crate { Label = "a" };
		crate.Next = crate;
		sameCrate = crate;
		crates = new List<Crate> { crate, new Crate { Label = "b" } };
		moods = new[] { Mood.Sleepy, Mood.Calm };
		byLabel = crates.ToDictionary(item => item.Label);
		friend = other;
		others = new List<Behaviour> { other };
		onHit = () => Score++;
		started = 3f;
	}

	/// <summary>Everything it holds as text, the same for either build when the values are the same.</summary>
	public string Describe()
	{
		var shared = ReferenceEquals(crate, sameCrate) && ReferenceEquals(crate?.Next, crate) && ReferenceEquals(crates[0], crate);
		var labels = string.Join(",", crates.Select(item => item.Label));
		var table = string.Join(",", byLabel.Select(pair => $"{pair.Key}={pair.Value.Label}"));
		return $"{Score} {mood} {cargo.Weight} {cargo.Mood} {crate?.Label} {shared} {labels} {string.Join(",", moods)} {table} {started}";
	}

	/// <summary>Whether both of its references to another behaviour point at that one.</summary>
	public bool Knows(Keeper other)
	{
		return ReferenceEquals(friend, other) && others.Count == 1 && ReferenceEquals(others[0], other);
	}
}
