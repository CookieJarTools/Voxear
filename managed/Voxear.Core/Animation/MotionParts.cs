using System;
using System.Collections.Generic;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// References to a Motion Set and its states, actions and marks: the set's file, then the part, as
/// "Sets/Hero.motionset#action:Cast". Each keeps its set by GUID, so a part named by its set's path and one named by
/// its GUID are the same part.
/// </summary>
internal static class MotionParts
{
	public const string State = "state:";
	public const string Action = "action:";
	public const string Mark = "mark:";

	private static readonly Dictionary<string, string> guids = [];

	/// <summary>
	/// The set and the name of the part a reference names, or false when it names none. Logs an error for a
	/// reference that is set but names no part of this kind, or no set the project has.
	/// </summary>
	public static bool Split(string? reference, string part, out string set, out string name)
	{
		(set, name) = (string.Empty, string.Empty);
		if (string.IsNullOrEmpty(reference))
		{
			return false;
		}

		var at = reference.IndexOf('#' + part, StringComparison.Ordinal);
		var found = at > 0 ? SetKey(reference[..at]) : null;
		if (found == null || at + 1 + part.Length >= reference.Length)
		{
			Log.Error($"\"{reference}\" names no {part.TrimEnd(':')} of a Motion Set.");
			return false;
		}

		(set, name) = (found, reference[(at + 1 + part.Length)..]);
		return true;
	}

	/// <summary>
	/// The set's GUID, as saves and comparisons keep it: null when the project has no such set. Away from the engine,
	/// as in tests, the path as given.
	/// </summary>
	public static string? SetKey(string set)
	{
		if (!Native.IsConnected)
		{
			return set;
		}

		if (guids.TryGetValue(set, out var known))
		{
			return known;
		}

		var guid = NativeSaves.Key(NativeSaves.KeyReference, 0, set);
		if (guid != null)
		{
			guids[set] = guid;
		}
		return guid;
	}
}
