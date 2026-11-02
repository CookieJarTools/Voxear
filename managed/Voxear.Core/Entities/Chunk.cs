using System;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// One of the engine's chunks a query matched: up to a few hundred entities with the same components, each
/// component a column. Its spans point into the engine's memory and are good until the walk moves on, which is
/// why adding, removing and destroying wait until the walk is done.
/// </summary>
public readonly unsafe ref struct Chunk
{
	private const int Header = 3;

	private readonly ReadOnlySpan<nint> row;
	private readonly ComponentIds fetch;
	private readonly int fetchCount;

	internal Chunk(ReadOnlySpan<nint> row, ComponentIds fetch, int fetchCount)
	{
		this.row = row;
		this.fetch = fetch;
		this.fetchCount = fetchCount;
	}

	/// <summary>How many entities it holds, the length of every span it hands out.</summary>
	public int Count => (int)row[1];

	public ReadOnlySpan<Entity> Entities => new ReadOnlySpan<Entity>((void*)row[2], Count);

	/// <summary>A fetched component's values, to read.</summary>
	public ReadOnlySpan<T> Read<T>() where T : unmanaged, IComponent
	{
		return new ReadOnlySpan<T>((void*)row[Column(ComponentType<T>.Id, typeof(T))], Count);
	}

	/// <summary>A fetched component's values, to change in place. It marks the chunk changed, for Changed filters.</summary>
	public Span<T> Write<T>() where T : unmanaged, IComponent
	{
		var id = ComponentType<T>.Id;
		var column = Column(id, typeof(T));
		Native.Api->ChunkWritten((ulong)row[0], id);
		return new Span<T>((void*)row[column], Count);
	}

	private int Column(uint id, Type type)
	{
		for (var i = 0; i < fetchCount; i++)
		{
			if (fetch[i] == id)
			{
				return Header + i;
			}
		}
		throw new InvalidOperationException($"The query does not fetch {type.Name}; name it in Query<...>() to read or write it");
	}
}
