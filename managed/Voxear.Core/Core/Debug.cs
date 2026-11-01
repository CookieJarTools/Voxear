using System.Diagnostics;
using System.Numerics;
using System.Text;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// Draws lines, shapes and text in the 3D view while the game runs, in the editor and in development builds, for as
	/// long as asked, as Unity's Debug.DrawLine: <c>Debug.Line(Obj.WorldPosition, target, Color.Green)</c>. A duration
	/// of zero shows for one frame, so code that draws each frame keeps its drawing up to date. Lines are dimmed
	/// where the scene is in front of them unless drawn on top. Calls are compiled out of scripts built without DEBUG, as
	/// a release build's are, so they cost a release nothing. Draw has more shapes, scopes and line widths.
	/// </summary>
	public static unsafe class Debug
	{
		private const int OnTop = 1;
		private const int WithHead = 2;

		[Conditional("DEBUG")]
		public static void Line(Vector3 from, Vector3 to, Color color, float duration = 0f, bool onTop = false)
		{
			Segment(from, to, color, duration, onTop ? OnTop : 0);
		}

		/// <summary>A line from one point to another with a head at the end, as for a direction or a velocity.</summary>
		[Conditional("DEBUG")]
		public static void Arrow(Vector3 from, Vector3 to, Color color, float duration = 0f, bool onTop = false)
		{
			Segment(from, to, color, duration, WithHead | (onTop ? OnTop : 0));
		}

		/// <summary>A box's edges, size across on each axis, around center.</summary>
		[Conditional("DEBUG")]
		public static void Box(Vector3 center, Vector3 size, Color color, float duration = 0f, bool onTop = false)
		{
			Box(center, size, Quaternion.Identity, color, duration, onTop);
		}

		/// <summary>A box's edges, size across on each of its own axes, turned by rotation.</summary>
		[Conditional("DEBUG")]
		public static void Box(Vector3 center, Vector3 size, Quaternion rotation, Color color, float duration = 0f, bool onTop = false)
		{
			if (Native.IsConnected)
			{
				var half = size * 0.5f;
				Native.Api->DebugBox(&center, &half, &rotation, &color, duration, onTop ? 1 : 0);
			}
		}

		/// <summary>A sphere, as a circle around each axis.</summary>
		[Conditional("DEBUG")]
		public static void Sphere(Vector3 center, float radius, Color color, float duration = 0f, bool onTop = false)
		{
			if (Native.IsConnected)
			{
				Native.Api->DebugSphere(&center, radius, &color, duration, onTop ? 1 : 0);
			}
		}

		/// <summary>White text at a point in the world, at a fixed size facing the camera. It shows in the editor's views.</summary>
		[Conditional("DEBUG")]
		public static void Text(Vector3 at, string text, float duration = 0f)
		{
			Text(at, text, Color.White, duration);
		}

		[Conditional("DEBUG")]
		public static void Text(Vector3 at, string text, Color color, float duration = 0f)
		{
			if (!Native.IsConnected)
			{
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(text);
			fixed (byte* written = bytes)
			{
				Native.Api->DebugText(&at, written, bytes.Length, &color, duration);
			}
		}

		private static void Segment(Vector3 from, Vector3 to, Color color, float duration, int flags)
		{
			if (Native.IsConnected)
			{
				Native.Api->DebugLine(&from, &to, &color, duration, flags);
			}
		}
	}
}
