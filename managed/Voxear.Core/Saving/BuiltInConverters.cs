using System;
using System.Collections.Generic;
using System.Globalization;
using System.Numerics;

namespace Voxear.Saving
{
	/// <summary>
	/// How saves keep the math and time types that have no public fields to save, or that read better another way:
	/// vectors, rotations, colours and matrices as lists of numbers, as scenes write them, and dates, times and GUIDs
	/// as text that sorts and reads the same everywhere.
	/// </summary>
	internal static class BuiltInConverters
	{
		public static Dictionary<Type, SaveConverter> All { get; } = Make();

		private static Dictionary<Type, SaveConverter> Make()
		{
			var all = new List<SaveConverter>
			{
				new DelegateConverter<Vector2>(v => SaveNode.NewList(v.X, v.Y), n => Floats(n, 2) is [var x, var y] ? new Vector2(x, y) : default),
				new DelegateConverter<Vector3>(v => SaveNode.NewList(v.X, v.Y, v.Z), n => Floats(n, 3) is [var x, var y, var z] ? new Vector3(x, y, z) : default),
				new DelegateConverter<Vector4>(v => SaveNode.NewList(v.X, v.Y, v.Z, v.W), n => Floats(n, 4) is [var x, var y, var z, var w] ? new Vector4(x, y, z, w) : default),
				new DelegateConverter<Quaternion>(q => SaveNode.NewList(q.X, q.Y, q.Z, q.W), n => Floats(n, 4) is [var x, var y, var z, var w] ? new Quaternion(x, y, z, w) : default),
				new DelegateConverter<Color>(c => SaveNode.NewList(c.R, c.G, c.B, c.A), n => Floats(n, 4) is [var r, var g, var b, var a] ? new Color(r, g, b, a) : default),
				new DelegateConverter<Matrix4x4>(WriteMatrix, ReadMatrix),
				new DelegateConverter<DateTime>(d => d.ToString("O", CultureInfo.InvariantCulture), n => DateTime.Parse(Text(n), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind)),
				new DelegateConverter<DateTimeOffset>(d => d.ToString("O", CultureInfo.InvariantCulture), n => DateTimeOffset.Parse(Text(n), CultureInfo.InvariantCulture, DateTimeStyles.RoundtripKind)),
				new DelegateConverter<TimeSpan>(t => t.ToString("c", CultureInfo.InvariantCulture), n => TimeSpan.Parse(Text(n), CultureInfo.InvariantCulture)),
				new DelegateConverter<Guid>(g => g.ToString("D"), n => Guid.Parse(Text(n))),
			};
			var byType = new Dictionary<Type, SaveConverter>();
			all.ForEach(converter => byType[converter.Type] = converter);
			return byType;
		}

		private static SaveNode WriteMatrix(Matrix4x4 m)
		{
			var list = SaveNode.NewList();
			for (var row = 0; row < 4; row++)
			{
				for (var column = 0; column < 4; column++)
				{
					list.Add(m[row, column]);
				}
			}
			return list;
		}

		private static Matrix4x4 ReadMatrix(SaveNode node)
		{
			var f = Floats(node, 16);
			var m = new Matrix4x4();
			for (var at = 0; at < 16; at++)
			{
				m[at / 4, at % 4] = f[at];
			}
			return m;
		}

		/// <summary>Exactly count numbers, or a FormatException, so the patterns that take them apart always match.</summary>
		private static float[] Floats(SaveNode node, int count)
		{
			if (node.Kind != SaveNodeKind.List || node.Count != count)
			{
				throw new FormatException($"The save has {node.Shown()} where a list of {count} numbers was expected.");
			}

			var floats = new float[count];
			for (var at = 0; at < count; at++)
			{
				floats[at] = node[at].AsFloat();
			}
			return floats;
		}

		private static string Text(SaveNode node)
		{
			return node.AsString() ?? throw new FormatException("The save has nothing where text was expected.");
		}
	}
}
