using System;
using System.Text;

namespace Voxear.Interop
{
	/// <summary>Thin managed wrappers over the engine's function table.</summary>
	internal static unsafe class Native
	{
		public static NativeApi* Api;

		public static bool IsConnected => Api != null;

		public static void Log(LogLevel level, string message)
		{
			if (!IsConnected)
			{
				Console.WriteLine($"[{level}] {message}");
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(message);
			fixed (byte* text = bytes)
			{
				Api->Log((int)level, text, bytes.Length);
			}
		}

		public static string ObjName(ulong obj)
		{
			var length = Api->ObjName(obj, null, 0);
			if (length <= 0)
			{
				return string.Empty;
			}

			var bytes = new byte[length];
			fixed (byte* buffer = bytes)
			{
				Api->ObjName(obj, buffer, length);
			}
			return Encoding.UTF8.GetString(bytes);
		}

		public static Transform GetTransform(ulong obj)
		{
			var transform = Transform.Identity;
			Api->TransformGet(obj, &transform);
			return transform;
		}

		public static void SetTransform(ulong obj, Transform transform)
		{
			Api->TransformSet(obj, &transform);
		}

		public static void SetEnabled(uint behaviour, bool enabled)
		{
			Api->BehaviourSetEnabled(behaviour, enabled ? 1 : 0);
		}

		public static void Destroy(uint behaviour)
		{
			Api->BehaviourDestroy(behaviour);
		}

		public static void LoadLevel(string level)
		{
			var bytes = Encoding.UTF8.GetBytes(level);
			fixed (byte* text = bytes)
			{
				Api->LevelLoad(text, bytes.Length);
			}
		}

		public static void DestroyObj(ulong obj)
		{
			Api->ObjDestroy(obj);
		}

		public static uint[] Behaviours(ulong obj)
		{
			var count = Api->ObjBehaviours(obj, null, 0);
			if (count <= 0)
			{
				return Array.Empty<uint>();
			}

			var handles = new uint[count];
			fixed (uint* buffer = handles)
			{
				Api->ObjBehaviours(obj, buffer, count);
			}
			return handles;
		}

		/// <summary>Creates a behaviour on the Obj and returns its handle, or -1. Its Awake has not run yet.</summary>
		public static int AddBehaviour(ulong obj, string typeName)
		{
			var bytes = Encoding.UTF8.GetBytes(typeName);
			fixed (byte* text = bytes)
			{
				return Api->BehaviourAdd(obj, text, bytes.Length);
			}
		}

		public static int LoadPrefab(string reference)
		{
			var bytes = Encoding.UTF8.GetBytes(reference);
			fixed (byte* text = bytes)
			{
				return Api->PrefabLoad(text, bytes.Length);
			}
		}

		/// <summary>
		/// Creates a prefab's Objs and behaviours, whose handles come back in creation order. None of their
		/// Awake calls have run yet. Returns the root, or None when the prefab could not be instantiated.
		/// </summary>
		public static Obj Instantiate(int prefab, Transform* placement, bool worldSpace, Obj parent, out uint[] handles)
		{
			ulong root = 0;
			var count = Api->PrefabInstantiate(prefab, placement, worldSpace ? 1 : 0, parent.Id, &root);
			handles = count > 0 ? TakeBatch(count) : Array.Empty<uint>();
			return count < 0 ? Obj.None : new Obj(root);
		}

		public static void Adopt(uint[] handles, uint[] states)
		{
			fixed (uint* handleBuffer = handles)
			{
				fixed (uint* stateBuffer = states)
				{
					Api->BehavioursAdopt(handleBuffer, stateBuffer, handles.Length);
				}
			}
		}

		public static string Utf8(byte* text, int length)
		{
			return length <= 0 ? string.Empty : Encoding.UTF8.GetString(text, length);
		}

		private static uint[] TakeBatch(int count)
		{
			var handles = new uint[count];
			fixed (uint* buffer = handles)
			{
				Api->BatchHandles(buffer, count);
			}
			return handles;
		}
	}
}
