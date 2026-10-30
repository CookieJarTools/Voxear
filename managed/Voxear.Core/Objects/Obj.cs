using System;
using System.Numerics;
using Voxear.Interop;

namespace Voxear
{
	/// <summary>
	/// A thing in a scene, like a GameObject. It is a handle to an engine entity rather than a class,
	/// so passing one around never allocates, and an Obj whose entity is gone simply stops being valid.
	/// It is called Obj rather than Object because Object collides with System.Object in every file
	/// that uses System.
	/// </summary>
	public readonly struct Obj : IEquatable<Obj>
	{
		public static readonly Obj None = default;

		internal Obj(ulong id)
		{
			Id = id;
		}

		/// <summary>False for None. The engine treats generation zero as the null entity.</summary>
		public bool IsValid => (Id >> 32) != 0;

		public string Name => IsValid ? Native.ObjName(Id) : string.Empty;

		public Transform Transform
		{
			get { return Native.GetTransform(Id); }
			set { Native.SetTransform(Id, value); }
		}

		public Vector3 Position
		{
			get { return Transform.Position; }
			set { Transform = Transform with { Position = value }; }
		}

		public Quaternion Rotation
		{
			get { return Transform.Rotation; }
			set { Transform = Transform with { Rotation = value }; }
		}

		internal ulong Id { get; }

		public static bool operator ==(Obj left, Obj right)
		{
			return left.Id == right.Id;
		}

		public static bool operator !=(Obj left, Obj right)
		{
			return left.Id != right.Id;
		}

		public bool Equals(Obj other)
		{
			return Id == other.Id;
		}

		public override bool Equals(object? other)
		{
			return other is Obj obj && Equals(obj);
		}

		public override int GetHashCode()
		{
			return Id.GetHashCode();
		}

		public override string ToString()
		{
			return IsValid ? $"Obj({Name})" : "Obj(None)";
		}
	}
}
