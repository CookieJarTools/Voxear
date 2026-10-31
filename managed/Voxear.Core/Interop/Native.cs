using System;
using System.Text;

namespace Voxear.Interop
{
	/// <summary>Thin managed wrappers over the engine's function table.</summary>
	internal static unsafe class Native
	{
		public const int LevelInfoCurrent = 0;
		public const int LevelInfoActiveScene = 1;
		public const int LevelInfoScenes = 2;
		public const int LevelInfoError = 3;

		public static NativeApi* Api;

		public static bool IsConnected => Api != null;

		/// <summary>A line for the engine log, with the stack that wrote it and the Obj it is about (0 for none).</summary>
		public static void Log(LogLevel level, string message, string stack, ulong context)
		{
			if (!IsConnected)
			{
				Console.WriteLine($"[{level}] {message}");
				return;
			}

			var bytes = Encoding.UTF8.GetBytes(message);
			var trace = Encoding.UTF8.GetBytes(stack);
			fixed (byte* text = bytes)
			fixed (byte* frames = trace)
			{
				Api->Log((int)level, text, bytes.Length, frames, trace.Length, context);
			}
		}

		/// <summary>The Objs with a name, oldest first.</summary>
		public static ulong[] FindObjs(string name)
		{
			var bytes = Encoding.UTF8.GetBytes(name);
			fixed (byte* text = bytes)
			{
				var count = Api->ObjFind(text, bytes.Length, null, 0);
				var found = new ulong[Math.Max(count, 0)];
				fixed (ulong* into = found)
				{
					Api->ObjFind(text, bytes.Length, into, found.Length);
				}
				return found;
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

		public static Transform GetWorldTransform(ulong obj)
		{
			var transform = Transform.Identity;
			Api->ObjWorldTransform(obj, &transform);
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

		/// <summary>Starts a background load and returns its id, or -1 when it could not start.</summary>
		public static int LoadAsync(bool level, string reference)
		{
			var bytes = Encoding.UTF8.GetBytes(reference);
			fixed (byte* text = bytes)
			{
				return Api->LoadAsync(level ? 1 : 0, text, bytes.Length);
			}
		}

		/// <summary>Starts a level change in the background as the options say, and returns its id, or -1.</summary>
		public static int LevelLoadAsync(string reference, LevelLoadOptions options)
		{
			var data = new LevelLoadOptionsData
			{
				Hold = options.Hold ? 1 : 0,
				LeastTime = options.LeastTime,
				UnloadRest = options.UnloadRest ? 1 : 0,
				SetActive = options.SetActive ? 1 : 0,
				TidyUp = options.TidyUp ? 1 : 0,
			};
			var bytes = Encoding.UTF8.GetBytes(reference);
			fixed (byte* text = bytes)
			{
				return Api->LevelLoadAsync(text, bytes.Length, &data);
			}
		}

		/// <summary>What the engine says about levels: one of the LevelInfo kinds, for a load where it is about one.</summary>
		public static string? LevelInfo(int what, int id)
		{
			var length = Api->LevelInfo(what, id, null, 0);
			if (length < 0)
			{
				return null;
			}

			var bytes = new byte[length];
			fixed (byte* buffer = bytes)
			{
				Api->LevelInfo(what, id, buffer, length);
			}
			return Encoding.UTF8.GetString(bytes);
		}

		public static bool LoadStatus(int id, out LoadProgress status)
		{
			var result = default(LoadProgress);
			var known = id >= 0 && Api->LoadStatus((uint)id, &result) != 0;
			status = known ? result : default;
			return known;
		}

		public static void AllowActivation(int id, bool allow)
		{
			if (id >= 0)
			{
				Api->LoadAllowActivation((uint)id, allow ? 1 : 0);
			}
		}

		public static void KeepPrefab(int prefab, bool keep)
		{
			Api->PrefabKeep(prefab, keep ? 1 : 0);
		}

		public static void ContentUnloadUnused()
		{
			Api->ContentUnloadUnused();
		}

		public static ContentUsage ContentUsage()
		{
			var usage = default(ContentUsage);
			Api->ContentUsage(&usage);
			return usage;
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
