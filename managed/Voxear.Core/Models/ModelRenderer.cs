using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear;

/// <summary>
/// An Obj's Model Renderer: the model it draws whole, every part where the model puts it, and the colour it is tinted.
/// Get it with Obj.Model, or give an Obj one with Obj.AddModel.
/// <code>
/// public ModelAsset LongHair;
/// public Color HairColour = Color.Hex("#6b4a2f");
/// ...
/// var hair = Obj.Find("Hair").Model;
/// hair.Model = LongHair;
/// hair.Tint = HairColour;
/// </code>
/// </summary>
public readonly struct ModelRenderer : IEquatable<ModelRenderer>, IObjComponent
{
	private const int TintValue = 3;
	private const int LodBiasValue = 4;
	private const int PartsValue = 5;
	private const int FollowsValue = 6;
	private const int ModelText = 0;
	private const int SetModelText = 1;
	private const int PartNameText = 2;
	private const int PartMaterialText = 3;
	private const int SetPartMaterialText = 4;
	private const int SheetText = 5;
	private const int SetSheetText = 6;
	private const int EveryPart = 0xFFFFFF;

	public static readonly ModelRenderer None = default;

	internal ModelRenderer(Obj obj)
	{
		Obj = obj;
	}

	/// <summary>The Obj that draws the model.</summary>
	public Obj Obj { get; }

	/// <summary>False when its Obj has no Model Renderer, or is gone.</summary>
	public bool IsValid => Obj.IsValid && NativeComponent.Number(NativeComponent.ModelRenderer, Obj, NativeComponent.Has, 0f) != 0f;

	/// <summary>
	/// The model it draws. Setting another draws that one from this frame, keeping the tint, and the Obj's bones as
	/// they are posed when the new model has the same bones, as parts made on one rig do. A model that cannot be read
	/// is logged and leaves the old one drawn.
	/// </summary>
	public ModelAsset Model
	{
		get
		{
			var reference = NativeComponent.Text(NativeComponent.ModelRenderer, Obj, ModelText);
			return string.IsNullOrEmpty(reference) ? ModelAsset.None : ModelAsset.Load(reference);
		}
		set
		{
			if (value.IsSet)
			{
				NativeComponent.Ask(NativeComponent.ModelRenderer, Obj, SetModelText, value.Reference);
			}
		}
	}

	/// <summary>
	/// The colour that multiplies everything the model draws, as it looks on screen; white leaves it as made. It is this
	/// Obj's own, so two Objs drawing one model can be tinted apart. A box model keeps its sharp pixels and its holes,
	/// near and far, so a grey model tinted by a skin, hair or dye colour does for every colour.
	/// </summary>
	public Color Tint
	{
		get { return GetTint(TintSlot.First); }
		set { SetTint(TintSlot.First, value); }
	}

	/// <summary>Shifts which level of detail each part draws: 1 switches to simpler ones sooner, -1 later.</summary>
	public float LodBias
	{
		get { return NativeComponent.Number(NativeComponent.ModelRenderer, Obj, LodBiasValue, 0f); }
		set { NativeComponent.Set(NativeComponent.ModelRenderer, Obj, LodBiasValue, new Vector4(value, 0f, 0f, 0f)); }
	}

	/// <summary>How many parts it draws.</summary>
	public int PartCount => (int)NativeComponent.Number(NativeComponent.ModelRenderer, Obj, PartsValue, 0f);

	/// <summary>
	/// For a box model, the picture it draws its pixels from in place of the sheet inside its file: a picture of the
	/// same size and layout, as a face or a pattern painted on the model's own sheet and saved as a picture. Its
	/// cubes keep their palette's colours, and each colour of the picture takes the tint the model gives that colour.
	/// <see cref="Texture.None"/> draws the model's own sheet.
	/// </summary>
	public Texture Sheet
	{
		get
		{
			var reference = NativeComponent.Text(NativeComponent.ModelRenderer, Obj, SheetText);
			return string.IsNullOrEmpty(reference) ? Texture.None : Texture.Load(reference);
		}
		set { NativeComponent.Ask(NativeComponent.ModelRenderer, Obj, SetSheetText, value.Reference); }
	}

	/// <summary>A part's name in the model, by its place among the parts it draws; empty past the last.</summary>
	public string PartName(int part)
	{
		return part >= 0 ? NativeComponent.Text(NativeComponent.ModelRenderer, Obj, part << 8 | PartNameText) ?? string.Empty : string.Empty;
	}

	/// <summary>The material a part draws with in place of the model's own; <see cref="MaterialAsset.None"/> for its own.</summary>
	public MaterialAsset GetMaterial(int part)
	{
		var reference = part >= 0 ? NativeComponent.Text(NativeComponent.ModelRenderer, Obj, part << 8 | PartMaterialText) : null;
		return string.IsNullOrEmpty(reference) ? MaterialAsset.None : MaterialAsset.Load(reference);
	}

	/// <summary>
	/// Draws one part, by its place among the parts it draws, with a material in place of the model's own, from this
	/// frame; <see cref="MaterialAsset.None"/> puts the model's own back. As the Inspector's part material, and kept
	/// when the scene is saved.
	/// </summary>
	public void SetMaterial(int part, MaterialAsset material)
	{
		if (part >= 0 && part < EveryPart)
		{
			NativeComponent.Ask(NativeComponent.ModelRenderer, Obj, part << 8 | SetPartMaterialText, material.Reference);
		}
	}

	/// <summary>Draws every part with a material in place of the model's own; <see cref="MaterialAsset.None"/> puts theirs back.</summary>
	public void SetMaterial(MaterialAsset material)
	{
		NativeComponent.Ask(NativeComponent.ModelRenderer, Obj, EveryPart << 8 | SetPartMaterialText, material.Reference);
	}

	/// <summary>
	/// The Obj whose skeleton poses the model every frame, bone by bone by name, as a part worn on a body follows it;
	/// <see cref="Obj.None"/> when nothing does. Bones the other skeleton has not stand at rest from those they hang
	/// from. Set to the Obj's parent, as the Inspector's Follow parent, it is kept when the scene is saved.
	/// </summary>
	public Obj Follows
	{
		get { return NativeComponent.Get(NativeComponent.ModelRenderer, Obj, FollowsValue, out var value) ? NativeComponent.ObjOf(value) : Obj.None; }
		set { NativeComponent.Set(NativeComponent.ModelRenderer, Obj, FollowsValue, NativeComponent.ValueOf(value)); }
	}

	/// <summary>Takes the Model Renderer off its Obj, with how its bones were posed.</summary>
	public void Remove()
	{
		NativeComponent.Set(NativeComponent.ModelRenderer, Obj, NativeComponent.Remove, Vector4.Zero);
	}

	public static bool operator ==(ModelRenderer left, ModelRenderer right)
	{
		return left.Obj == right.Obj;
	}

	public static bool operator !=(ModelRenderer left, ModelRenderer right)
	{
		return left.Obj != right.Obj;
	}

	public bool Equals(ModelRenderer other)
	{
		return Obj == other.Obj;
	}

	public override bool Equals(object? other)
	{
		return other is ModelRenderer renderer && Equals(renderer);
	}

	public override int GetHashCode()
	{
		return Obj.GetHashCode();
	}

	public override string ToString()
	{
		return $"{Obj}'s Model Renderer";
	}

	/// <summary>
	/// One of its three tints; white when it has none. The second and third show where its model says they fall: a
	/// box model's colours that take them, or a material's tint mask.
	/// </summary>
	public Color GetTint(TintSlot slot)
	{
		return NativeComponent.Get(NativeComponent.ModelRenderer, Obj, TintValue | (int)slot << 8, out var value)
			? new Color(value.X, value.Y, value.Z)
			: Color.White;
	}

	/// <summary>Sets one of its three tints, each channel held within 0 and 1.</summary>
	public void SetTint(TintSlot slot, Color tint)
	{
		NativeComponent.Set(NativeComponent.ModelRenderer, Obj, TintValue | (int)slot << 8, new Vector4(tint.R, tint.G, tint.B, 1f));
	}

	internal static bool Draw(Obj obj, ModelAsset model)
	{
		return model.IsSet && NativeComponent.Ask(NativeComponent.ModelRenderer, obj, SetModelText, model.Reference) != null;
	}
}
