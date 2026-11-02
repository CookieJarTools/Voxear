using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// A small image of an asset drawn on its own: a prefab with all its Objs, a model, a mesh, or a material on a
/// sphere, as the editor's Project panel shows them. Use it for inventory icons, shop items or a level's thumbnail.
/// The easy form is <c>Preview.Of(swordPrefab)</c>; <see cref="PreviewOptions"/> sets its size, background,
/// rotation, offset, zoom and light.
/// </summary>
/// <remarks>
/// A preview is drawn over the next frame or so, after which <see cref="IsReady"/> is true. Save writes it to a PNG
/// file once it is drawn. Release lets it go when it is no longer wanted; until then it keeps its place on the GPU.
/// </remarks>
public sealed class Preview : IDisposable
{
	private int id;

	private Preview(int id)
	{
		this.id = id;
	}

	/// <summary>Whether the preview is drawn, so its <see cref="Texture"/> and <see cref="Region"/> show it.</summary>
	public bool IsReady => State(out _) == 1;

	/// <summary>The engine's texture the preview is drawn into, for showing it in UI once there is a C# UI.</summary>
	public uint Texture => State(out var state) == 1 ? state.Texture : 0;

	/// <summary>Where in <see cref="Texture"/> the preview is: left, top, right and bottom, as fractions of it.</summary>
	public Vector4 Region => State(out var state) == 1 ? state.Region : Vector4.Zero;

	/// <summary>
	/// A preview of an asset by project path or GUID, for example "Prefabs/Sword.prefab" or "Materials/Gold.material",
	/// or a built-in mesh such as "builtin:cube". Logs an error and returns null when it has nothing to show.
	/// </summary>
	public static Preview? Of(string asset, PreviewOptions? options = null)
	{
		return Create(-1, asset, options);
	}

	/// <summary>A preview of a loaded prefab, with all its Objs.</summary>
	public static Preview? Of(Prefab prefab, PreviewOptions? options = null)
	{
		if (!prefab.IsValid)
		{
			Log.Error("Preview.Of was given no prefab.");
			return null;
		}
		return Create(prefab.EngineId, string.Empty, options);
	}

	/// <summary>A preview of the asset a reference names, without loading it into the game.</summary>
	public static Preview? Of<T>(Ref<T> asset, PreviewOptions? options = null) where T : struct, IAsset<T>
	{
		return Create(-1, asset.Path, options);
	}

	/// <summary>
	/// Writes the preview to a PNG file at <paramref name="path"/>, with its alpha, once it is drawn and read back
	/// from the GPU: a few frames from now. Returns false when it cannot be saved.
	/// </summary>
	public bool Save(string path)
	{
		return id >= 0 && Native.SavePreview(id, path);
	}

	/// <summary>Lets the preview go. It shows nothing after this.</summary>
	public void Release()
	{
		if (id >= 0)
		{
			Native.FreePreview(id);
			id = -1;
		}
	}

	public void Dispose()
	{
		Release();
	}

	private static Preview? Create(int prefab, string asset, PreviewOptions? options)
	{
		var data = (options ?? new PreviewOptions()).ToData();
		var made = Native.CreatePreview(prefab, asset, data);
		return made < 0 ? null : new Preview(made);
	}

	private int State(out PreviewStateData state)
	{
		state = default;
		return id < 0 ? -1 : Native.PreviewState(id, out state);
	}
}
