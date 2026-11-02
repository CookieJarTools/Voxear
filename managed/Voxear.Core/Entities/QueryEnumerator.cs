using System;
using System.Buffers;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// Walks an EntityQuery's chunks, for foreach. The engine writes every chunk the query matches into a pooled
/// buffer once, at the start, so the walk itself makes no calls into the engine. Adding, removing and destroying
/// wait from the start until it is disposed, which foreach does even when the loop breaks early.
/// </summary>
public unsafe ref struct QueryEnumerator
{
	private const int Header = 3;
	private const int FirstCapacity = 32;

	private readonly ComponentIds fetch;
	private readonly int fetchCount;
	private readonly int stride;
	private nint[]? rows;
	private int count;
	private int at;

	internal QueryEnumerator(EntityQuery query)
	{
		fetch = query.Fetched;
		fetchCount = query.FetchCount;
		stride = Header + fetchCount;
		at = -1;
		rows = ArrayPool<nint>.Shared.Rent(FirstCapacity * stride);
		count = Fill(query, rows, stride);
		if (count > rows.Length / stride)
		{
			ArrayPool<nint>.Shared.Return(rows);
			rows = ArrayPool<nint>.Shared.Rent(count * stride);
			count = Math.Min(Fill(query, rows, stride), rows.Length / stride);
		}
		Native.Api->CommandsDefer(1);
	}

	public Chunk Current => new Chunk(new ReadOnlySpan<nint>(rows, at * stride, stride), fetch, fetchCount);

	public bool MoveNext()
	{
		return rows != null && ++at < count;
	}

	public void Dispose()
	{
		if (rows == null)
		{
			return;
		}

		ArrayPool<nint>.Shared.Return(rows);
		rows = null;
		Native.Api->CommandsDefer(0);
	}

	private static int Fill(EntityQuery query, nint[] rows, int stride)
	{
		fixed (nint* into = rows)
		{
			var found = query.Find(into, rows.Length / stride);
			if (found < 0)
			{
				throw new InvalidOperationException("The engine could not run this query");
			}
			return found;
		}
	}
}
