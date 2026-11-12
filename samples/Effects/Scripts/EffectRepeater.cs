using Voxear;

namespace Effects;

/// <summary>
/// Plays an effect at its Obj every so often, as a game plays one where a shell lands or a gun fires: nothing is
/// set up for it beforehand, and nothing is cleared up after it.
/// </summary>
public sealed class EffectRepeater : Behaviour
{
	[SerializeField] private ParticleEffect effect;
	[SerializeField] private float every = 4f;

	/// <summary>Played on the Obj, to go where it goes, rather than left at the place it was played.</summary>
	[SerializeField] private bool follows;

	/// <summary>How far round it the effect throws other particles out, in metres: none at 0.</summary>
	[SerializeField] private float blast;

	private float wait;

	protected override void Update()
	{
		wait -= Time.Delta;
		if (wait > 0f)
		{
			return;
		}

		wait = every;
		if (follows)
		{
			Particles.Play(effect, Obj);
		}
		else
		{
			Particles.Play(effect, Obj.WorldPosition, Obj.WorldRotation);
		}

		if (blast > 0f)
		{
			Particles.Push(Obj.WorldPosition, blast, blast * 2f);
		}
	}
}
