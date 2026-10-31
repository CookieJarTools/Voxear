using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// How a <see cref="Preview"/> looks. Every setting starts as an icon wants it: a three-quarter view from a little
	/// above, framed to fit, 128 pixels square, on a clear background. Change only what matters, for example
	/// <c>new PreviewOptions { Size = 256, Background = new Vector4(0.1f, 0.1f, 0.12f, 1f) }</c>.
	/// </summary>
	public sealed class PreviewOptions
	{
		/// <summary>Width and height in pixels, up to 512.</summary>
		public int Size { get; set; } = 128;

		/// <summary>Behind the object: red, green, blue and alpha, in sRGB from 0 to 1. An alpha of 0 leaves it clear.</summary>
		public Vector4 Background { get; set; } = Vector4.Zero;

		/// <summary>Turns the object before it is framed. The default turns it a little to show three sides.</summary>
		public Quaternion Rotation { get; set; } = Quaternion.CreateFromAxisAngle(Vector3.UnitY, -35f * MathF.PI / 180f);

		/// <summary>Moves the framed object across the image, as a share of its width and height: (0.25, 0) is a quarter to the right.</summary>
		public Vector2 Offset { get; set; } = Vector2.Zero;

		/// <summary>How large the object is against the frame: 1 fits it, 2 is twice as close.</summary>
		public float Zoom { get; set; } = 1f;

		/// <summary>The camera's field of view in degrees. Small looks flat, like an icon; large looks close.</summary>
		public float FieldOfView { get; set; } = 30f;

		/// <summary>Where the key light comes from, towards the object. A softer fill from the camera's side follows it.</summary>
		public Vector3 LightFrom { get; set; } = new Vector3(-0.5f, 0.8f, -0.6f);

		/// <summary>The key light's colour, linear, from 0 to 1.</summary>
		public Vector3 LightColor { get; set; } = Vector3.One;

		public float LightIntensity { get; set; } = 3f;

		/// <summary>Light from the sky on every side, as a share of a scene's.</summary>
		public float Ambient { get; set; } = 1f;

		/// <summary>Brightens or darkens the whole image, as a camera's exposure.</summary>
		public float Exposure { get; set; } = 1f;

		internal PreviewOptionsData ToData()
		{
			return new PreviewOptionsData
			{
				Size = (uint)Math.Clamp(Size, 1, 512),
				Background = Background,
				Rotation = Rotation,
				Offset = Offset,
				Zoom = Zoom,
				FieldOfView = FieldOfView,
				LightFrom = LightFrom,
				LightColor = LightColor,
				LightIntensity = LightIntensity,
				Ambient = Ambient,
				Exposure = Exposure,
			};
		}
	}
}
