using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// The entities that have every component it fetches, chunk by chunk, made with World.Query or a system's Query.
/// Walking it hands out each chunk the engine keeps them in, whose components are spans over the engine's own
/// memory: no copies, and no call per entity.
/// <code>
/// foreach (var chunk in Query&lt;Transform, Spin&gt;().Without&lt;Frozen&gt;())
/// {
/// 	var transforms = chunk.Write&lt;Transform&gt;();
/// 	var spins = chunk.Read&lt;Spin&gt;();
/// 	for (var i = 0; i &lt; chunk.Count; i++) { ... }
/// }
/// </code>
/// Building one allocates nothing, so a system can build its queries each frame.
/// </summary>
public struct EntityQuery
{
	public const int MaxFetched = 8;
	public const int MaxFiltered = 4;

	private ComponentIds fetch;
	private FilterIds with;
	private FilterIds without;
	private FilterIds changed;
	private int fetchCount;
	private int withCount;
	private int withoutCount;
	private int changedCount;
	private uint since;

	internal EntityQuery(uint since)
	{
		this.since = since;
	}

	internal int FetchCount => fetchCount;

	internal ComponentIds Fetched => fetch;

	/// <summary>Only entities that also have T, which the chunks do not hand out.</summary>
	public EntityQuery With<T>() where T : unmanaged, IComponent
	{
		var query = this;
		query.with[Next(ref query.withCount, MaxFiltered, "With")] = ComponentType<T>.Id;
		return query;
	}

	/// <summary>Only entities without T.</summary>
	public EntityQuery Without<T>() where T : unmanaged, IComponent
	{
		var query = this;
		query.without[Next(ref query.withoutCount, MaxFiltered, "Without")] = ComponentType<T>.Id;
		return query;
	}

	/// <summary>
	/// Only chunks where T was written since the system last ran, or, outside a system, at all. It goes by chunk, as
	/// Unity's does: a chunk holds many entities, and writing one marks the chunk.
	/// </summary>
	public EntityQuery Changed<T>() where T : unmanaged, IComponent
	{
		var query = this;
		query.changed[Next(ref query.changedCount, MaxFiltered, "Changed")] = ComponentType<T>.Id;
		return query;
	}

	/// <summary>How many entities it matches now. It walks the chunks, so keep it rather than ask each time.</summary>
	public int Count()
	{
		var count = 0;
		foreach (var chunk in this)
		{
			count += chunk.Count;
		}
		return count;
	}

	/// <summary>
	/// Runs action on each entity's components, which it can change in place: the short way, for when the chunk
	/// itself is not needed. Every component it names is marked changed.
	/// </summary>
	public void ForEach<T1>(ForEachAction<T1> action)
		where T1 : unmanaged, IComponent
	{
		foreach (var chunk in this)
		{
			var first = chunk.Write<T1>();
			for (var i = 0; i < first.Length; i++)
			{
				action(ref first[i]);
			}
		}
	}

	public void ForEach<T1, T2>(ForEachAction<T1, T2> action)
		where T1 : unmanaged, IComponent
		where T2 : unmanaged, IComponent
	{
		foreach (var chunk in this)
		{
			var first = chunk.Write<T1>();
			var second = chunk.Write<T2>();
			for (var i = 0; i < first.Length; i++)
			{
				action(ref first[i], ref second[i]);
			}
		}
	}

	public void ForEach<T1, T2, T3>(ForEachAction<T1, T2, T3> action)
		where T1 : unmanaged, IComponent
		where T2 : unmanaged, IComponent
		where T3 : unmanaged, IComponent
	{
		foreach (var chunk in this)
		{
			var first = chunk.Write<T1>();
			var second = chunk.Write<T2>();
			var third = chunk.Write<T3>();
			for (var i = 0; i < first.Length; i++)
			{
				action(ref first[i], ref second[i], ref third[i]);
			}
		}
	}

	public QueryEnumerator GetEnumerator()
	{
		return new QueryEnumerator(this);
	}

	internal EntityQuery Fetch<T>() where T : unmanaged, IComponent
	{
		fetch[Next(ref fetchCount, MaxFetched, "Query")] = ComponentType<T>.Id;
		return this;
	}

	/// <summary>Fills rows with the chunks it matches, as many as fit, and returns how many there are.</summary>
	internal unsafe int Find(nint* rows, int capacity)
	{
		var fetched = stackalloc uint[MaxFetched];
		var withIds = stackalloc uint[MaxFiltered];
		var withoutIds = stackalloc uint[MaxFiltered];
		var changedIds = stackalloc uint[MaxFiltered];
		((ReadOnlySpan<uint>)fetch).CopyTo(new Span<uint>(fetched, MaxFetched));
		((ReadOnlySpan<uint>)with).CopyTo(new Span<uint>(withIds, MaxFiltered));
		((ReadOnlySpan<uint>)without).CopyTo(new Span<uint>(withoutIds, MaxFiltered));
		((ReadOnlySpan<uint>)changed).CopyTo(new Span<uint>(changedIds, MaxFiltered));
		var desc = new QueryDesc
		{
			Fetch = fetched,
			FetchCount = fetchCount,
			With = withIds,
			WithCount = withCount,
			Without = withoutIds,
			WithoutCount = withoutCount,
			Changed = changedIds,
			ChangedCount = changedCount,
			Since = since,
		};
		return Native.Api->QueryChunks(&desc, rows, capacity);
	}

	private static int Next(ref int count, int max, string what)
	{
		if (count == max)
		{
			throw new InvalidOperationException($"A query takes at most {max} components in {what}");
		}
		return count++;
	}
}
