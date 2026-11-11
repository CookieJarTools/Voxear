// Lava's light and the shapes of its crust, shared by lava Pools and Streams (water.slang) and terrain surfaces of
// lava (terrain.slang).

#pragma once

// The light of something as hot as `kelvin`, by Planck's law at red's,
// green's and blue's wavelengths: 1 in red at 1400 K, where it is orange;
// yellow and far brighter hotter; dull red, and dimming fast, as it cools.
float3 blackbody(float kelvin)
{
    static const float3 wavelength = float3(610e-9, 550e-9, 465e-9);
    float3 shorter = pow(wavelength / 610e-9, 5.0);
    float3 planck = 1.0 / (shorter * (exp(1.4388e-2 / (wavelength * max(kelvin, 300.0))) - 1.0));
    return planck * (exp(1.4388e-2 / (610e-9 * 1400.0)) - 1.0);
}

// How hot lava is, in kelvin: where it wells up, its skin as that sets,
// and its crust once it no longer glows by day.
static const float LAVA_MOLTEN = 1440.0;
static const float LAVA_SKIN = 1260.0;
static const float LAVA_SET = 900.0;

// The light lava as hot as `kelvin` gives off, for a Glow of 1: molten rock
// several times as bright as a white wall in the sun.
float3 lavaLight(float kelvin)
{
    return blackbody(kelvin) * 4.0;
}

float2 cellPoint(float2 cell)
{
    return frac(sin(float2(dot(cell, float2(127.1, 311.7)), dot(cell, float2(269.5, 183.3)))) * 43758.5453);
}

// Two numbers from 0 to 1 that belong to whole-numbered `cell`, a different
// two for each `salt`. Reckoned in whole numbers, so they are the same
// wherever they are worked out. Numbers taken from a sine are not: their
// last bits go by how the sum under the sine was reckoned, which differs
// from one use to the next, and the corners two cells share then get
// different numbers in each, so nothing made of them joins up.
float2 lavaCell(float2 cell, uint salt)
{
    uint2 whole = uint2(int2(floor(cell + 0.5)));
    uint mixed = whole.x * 1597334677u ^ whole.y * 3812015801u ^ salt * 2654435761u;
    uint2 v;
    v.x = (mixed ^ (mixed >> 15u)) * 2246822519u;
    v.y = (v.x ^ (v.x >> 13u)) * 3266489917u;
    v ^= v >> 16u;
    return float2(v >> 8u) * (1.0 / 16777216.0);
}

// Which numbers of a cell are whose: where its plate's middle is, its
// plate's own numbers, its noise, and its bubbles'.
static const uint CELL_PLATE = 0;
static const uint CELL_OWN = 1;
static const uint CELL_NOISE = 2;
static const uint CELL_BUBBLE = 3;

// The plates of crust round a place: how far it is from the nearest gap
// between two of them, in plates, and from the next nearest; the way to the
// nearest; its own plate's number, from 0 to 1, and that of the plate
// across the gap.
struct LavaPlates
{
    float edge;
    float next;
    float2 toEdge;
    float plate;
    float beyond;
};

// Where plate `cell` has its middle, in its cell: wandering slowly round
// with `churn`, a phase, as plates jostle. Kept away from its cell's sides,
// so the plates a plate meets are always among the eight cells round its
// own.
float2 platePoint(float2 cell, float churn)
{
    float2 at = lavaCell(cell, CELL_PLATE);
    return 0.27 + 0.46 * at + 0.07 * sin(churn + 6.2831 * at.yx);
}

// The plates round `p`, a unit each, jostling as `churn` goes on.
LavaPlates lavaPlates(float2 p, float churn)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float2 nearest = 0.0;
    float2 nearestCell = 0.0;
    float best = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + platePoint(n + g, churn) - f;
            float d = dot(r, r);
            if (d < best)
            {
                best = d;
                nearest = r;
                nearestCell = g;
            }
        }
    }
    // Half way to each neighbour, across the line between them.
    LavaPlates plates;
    plates.edge = 8.0;
    plates.next = 8.0;
    plates.toEdge = float2(1.0, 0.0);
    plates.plate = lavaCell(n + nearestCell, CELL_OWN).x;
    plates.beyond = plates.plate;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = nearestCell + float2(i, j);
            float2 r = g + platePoint(n + g, churn) - f;
            float2 between = r - nearest;
            float apart = dot(between, between);
            if (apart > 1e-5)
            {
                float2 way = between * rsqrt(apart);
                float d = dot(0.5 * (nearest + r), way);
                if (d < plates.edge)
                {
                    plates.next = plates.edge;
                    plates.edge = d;
                    plates.toEdge = way;
                    plates.beyond = lavaCell(n + g, CELL_OWN).x;
                }
                else
                {
                    plates.next = min(plates.next, d);
                }
            }
        }
    }
    return plates;
}

// How far `p` is from the nearest edge between a scatter of cells a unit
// wide (x), and which cell it is in, from 0 to 1 (y).
float2 cellEdge(float2 p)
{
    LavaPlates plates = lavaPlates(p, 0.0);
    return float2(plates.edge, plates.plate);
}

// The skin on molten rock, a finer scatter of cells: how far `p` is from
// the nearest line between two of them, 0 on it (x), and its cell's number,
// from 0 to 1 (y).
float2 lavaSkin(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float first = 8.0;
    float second = 8.0;
    float2 cell = 0.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + lavaCell(n + g, CELL_PLATE) - f;
            float d = dot(r, r);
            if (d < first)
            {
                second = first;
                first = d;
                cell = n + g;
            }
            else if (d < second)
            {
                second = d;
            }
        }
    }
    return float2(sqrt(second) - sqrt(first), lavaCell(cell, CELL_OWN).y);
}

// Smooth noise from 0 to 1, a unit across.
float lavaNoise(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    f = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float a = lavaCell(n, CELL_NOISE).x;
    float b = lavaCell(n + float2(1, 0), CELL_NOISE).x;
    float c = lavaCell(n + float2(0, 1), CELL_NOISE).x;
    float d = lavaCell(n + float2(1, 1), CELL_NOISE).x;
    return lerp(lerp(a, b, f.x), lerp(c, d, f.x), f.y);
}

// Smooth noise from 0 to 1, a unit across (x), as lavaNoise, and how fast it
// rises across `p` each way (yz).
float3 lavaNoiseSloped(float2 p)
{
    float2 n = floor(p);
    float2 f = frac(p);
    float2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    float2 du = 30.0 * f * f * (f * (f - 2.0) + 1.0);
    float a = lavaCell(n, CELL_NOISE).x;
    float b = lavaCell(n + float2(1, 0), CELL_NOISE).x;
    float c = lavaCell(n + float2(0, 1), CELL_NOISE).x;
    float d = lavaCell(n + float2(1, 1), CELL_NOISE).x;
    float twist = a - b - c + d;
    return float3(lerp(lerp(a, b, u.x), lerp(c, d, u.x), u.y), float2(b - a + twist * u.y, c - a + twist * u.x) * du);
}

// Noise from 0 to 1 in three sizes, each turned from the last so no grid
// shows, the largest a unit across.
float lavaClouds(float2 p)
{
    static const float2x2 turn = float2x2(0.8, 0.6, -0.6, 0.8);
    float total = lavaNoise(p) * 0.55;
    p = mul(turn, p) * 2.03 + 3.1;
    total += lavaNoise(p) * 0.3;
    p = mul(turn, p) * 2.01 + 7.7;
    return total + lavaNoise(p) * 0.15;
}

// How deep under its crust's top lava's surface lies `side` metres out of
// a gap between two plates (less than nothing in the gap), the crust
// `depth` thick and its edge sloping down over `bevel`: 0 on the crust,
// `depth` on the molten rock in the gap.
float lavaSunk(float side, float bevel, float depth)
{
    return depth * (1.0 - smoothstep(0.0, bevel, side));
}

// The plates of crust a view into a gap can meet from a place: the plate the
// place is on, the first of them, and the six round it. Where each has its
// middle, from the place, in plates; and the cell of the place's own.
static const uint CRUST_PLATES = 7;
struct LavaCrust
{
    float2 to[CRUST_PLATES];
    float2 cell;
};

// Which cells those are, from the place's own: on a lattice of three ways a
// cell has six round it.
static const float2 CRUST_CELLS[CRUST_PLATES] = {
    float2(0, 0), float2(1, 0), float2(-1, 0), float2(0, 1), float2(0, -1), float2(1, -1), float2(-1, 1)
};

// Where the plate of `cell` has its middle, in its cell, wandering slowly
// round with `churn`, a phase, as plates jostle. Kept well away from its
// cell's sides, so the plates a plate meets are the six round its own.
float2 crustPoint(float2 cell, float churn)
{
    float2 at = lavaCell(cell, CELL_PLATE);
    return 0.31 + 0.38 * at + 0.03 * sin(churn + 6.2831 * at.yx);
}

// The plates round `p`, about a unit each, jostling as `churn` goes on.
// Their middles are scattered on a lattice of three ways, not two, so their
// edges meet three at a corner at wide angles, as cracks do, and no plate
// is near square.
LavaCrust lavaCrust(float2 p, float churn)
{
    float2 skewed = float2(p.x - p.y * 0.57735, p.y * 1.1547);
    float2 n = floor(skewed);
    float2 f = frac(skewed);
    float2 nearest = 0.0;
    float best = 8.0;
    for (int j = -1; j <= 1; j++)
    {
        for (int i = -1; i <= 1; i++)
        {
            float2 g = float2(i, j);
            float2 r = g + crustPoint(n + g, churn) - f;
            r = float2(r.x + 0.5 * r.y, r.y * 0.866025);
            float d = dot(r, r);
            if (d < best)
            {
                best = d;
                nearest = g;
            }
        }
    }
    LavaCrust crust;
    crust.cell = n + nearest;
    for (uint k = 0; k < CRUST_PLATES; k++)
    {
        float2 g = nearest + CRUST_CELLS[k];
        float2 r = g + crustPoint(n + g, churn) - f;
        crust.to[k] = float2(r.x + 0.5 * r.y, r.y * 0.866025);
    }
    return crust;
}

// Two numbers of the `which`th of `crust`'s plates, from 0 to 1 each, its
// own from one plate to the next.
float2 lavaPlateNumbers(LavaCrust crust, uint which)
{
    return lavaCell(crust.cell + CRUST_CELLS[which], CELL_OWN);
}

// How plates' edges wander, so none runs straight: three waves across the
// plates, each shifting them sideways along itself. Which way each runs and
// how many turns a plate; and how far each shifts, in plates.
static const float2 WANDER_WAYS[3] = { float2(0.83, 0.56) * 11.4, float2(-0.44, 0.9) * 23.3, float2(0.97, -0.26) * 48.3 };
static const float2 WANDER_SHIFTS[3] = { float2(-0.56, 0.83) * 0.04, float2(-0.9, -0.44) * 0.019, float2(0.26, 0.97) * 0.009 };

// How far along each of those waves `p` is, in plates across the lava.
float3 lavaWanderAt(float2 p)
{
    return float3(dot(p, WANDER_WAYS[0]), dot(p, WANDER_WAYS[1]), dot(p, WANDER_WAYS[2]));
}

// How far a place that far along each wave is shifted, in plates.
float2 lavaWander(float3 phase)
{
    float3 wave = sin(phase);
    return WANDER_SHIFTS[0] * wave.x + WANDER_SHIFTS[1] * wave.y + WANDER_SHIFTS[2] * wave.z;
}

// What of `crust`'s plates is found at a place: how far it is to the nearest
// edge between two of them, in plates; the same `rounded`, measured to all
// the plate's edges together, so it turns a corner of the plate in a curve
// and not at a line; the way from the plate to the nearest edge; and which
// plate it is.
struct LavaPlace
{
    float edge;
    float rounded;
    float2 way;
    uint plate;
};

// How far it is to the nearest edge between two of `crust`'s plates, in
// plates, from `at`, in plates from the crust's place.
float lavaEdge(LavaCrust crust, float2 at)
{
    float far[CRUST_PLATES];
    float best = 1e9;
    float2 mine = 0.0;
    for (uint k = 0; k < CRUST_PLATES; k++)
    {
        float2 r = crust.to[k] - at;
        far[k] = dot(r, r);
        mine = far[k] < best ? crust.to[k] : mine;
        best = min(best, far[k]);
    }
    // Half way to each of the others, across the line between the two.
    float edge = 8.0;
    for (uint k = 0; k < CRUST_PLATES; k++)
    {
        float2 between = crust.to[k] - mine;
        float apart = dot(between, between);
        edge = apart > 1e-6 ? min(edge, (far[k] - best) * 0.5 * rsqrt(apart)) : edge;
    }
    return edge;
}

// What of `crust`'s plates is found `at`, in plates from the crust's place.
LavaPlace lavaPlace(LavaCrust crust, float2 at)
{
    static const float SHARP = 12.0;
    float far[CRUST_PLATES];
    float best = 1e9;
    float2 mine = 0.0;
    LavaPlace place;
    place.plate = 0;
    for (uint k = 0; k < CRUST_PLATES; k++)
    {
        float2 r = crust.to[k] - at;
        far[k] = dot(r, r);
        if (far[k] < best)
        {
            best = far[k];
            mine = crust.to[k];
            place.plate = k;
        }
    }
    place.edge = 8.0;
    place.way = float2(1.0, 0.0);
    float together = 0.0;
    for (uint k = 0; k < CRUST_PLATES; k++)
    {
        float2 between = crust.to[k] - mine;
        float apart = dot(between, between);
        if (apart > 1e-6)
        {
            float closeness = rsqrt(apart);
            float d = (far[k] - best) * 0.5 * closeness;
            together += exp(-SHARP * d);
            if (d < place.edge)
            {
                place.edge = d;
                place.way = between * closeness;
            }
        }
    }
    place.rounded = -log(max(together, 1e-20)) / SHARP;
    return place;
}

// A view going on across lava's plates from a place: by how many plates a
// metre, how far along each of the waves its edges wander by it starts,
// and how fast it goes along them.
struct LavaRay
{
    float2 along;
    float3 phase;
    float3 turning;
};

// The view from `p` across the plates, going `along` plates a metre.
LavaRay lavaRay(float2 p, float2 along)
{
    LavaRay ray = { along, lavaWanderAt(p), lavaWanderAt(along) };
    return ray;
}

// Where `s` metres along `ray` is, in plates from where it started, as the
// plates' edges wander.
float2 lavaRayAt(LavaRay ray, float s)
{
    return ray.along * s + lavaWander(ray.phase + ray.turning * s);
}

// Where a view going down into lava's crust meets it: how far on along the
// surface from where it crossed the crust's top, in metres. `ray` is the
// view across `crust`'s plates, `size` metres each, and it sinks `slant`
// metres a metre; where it starts it is `side` metres out of the nearest
// gap, less than nothing in it. Every gap between two plates reaches `gap`
// metres into each, the crust is `depth` thick and its edge slopes down
// over `bevel`. The plates are followed as they are, round corners and
// where gaps join, so a gap is the same gap from whichever side it is seen.
float lavaInto(LavaCrust crust, LavaRay ray, float side, float slant, float size, float gap, float bevel, float depth)
{
    // No further than where it would reach the molten rock: until then it meets only a plate's side, if anything.
    float most = min(depth / max(slant, 1e-3), 0.7);
    // How fast it can near a plate's side at most, in metres a metre, with how the edges wander.
    float nearing = length(ray.along) * size * 1.3 + 1e-4;
    // Too far out in a gap to come to either side of it.
    if (side + most * nearing < 0.0)
    {
        return most;
    }
    // Over a gap, as far at a time as stays clear of its sides; by one, in short steps, so no sliver of crust is
    // stepped over.
    float least = most / clamp(ceil(most / 0.03), 2.0, 12.0);
    float before = 0.0;
    float s = 0.0;
    bool met = false;
    for (uint i = 0; i < 12 && !met && s < most; i++)
    {
        s = min(s + max(-side / nearing, least), most);
        side = lavaEdge(crust, lavaRayAt(ray, s)) * size - gap;
        met = s * slant >= lavaSunk(side, bevel, depth);
        before = met ? before : s;
    }
    // Nothing but the molten rock, or what lies under where the view is given up.
    if (!met || side <= 0.0)
    {
        return met ? s : most;
    }
    // A plate's side: between the last step over it and the first under it.
    float reached = s;
    for (uint k = 0; k < 4; k++)
    {
        s = (before + reached) * 0.5;
        bool under = s * slant >= lavaSunk(lavaEdge(crust, lavaRayAt(ray, s)) * size - gap, bevel, depth);
        before = under ? before : s;
        reached = under ? s : reached;
    }
    return (before + reached) * 0.5;
}

// How high molten lava heaves at `plate` across its plates, in metres, at
// time `t`: slow swells a couple of metres across that rise and sink over
// several seconds, where it has cooled `cooled` of its Cooling or less.
// Crust lies still.
float lavaHeave(float2 plate, float t, float cooled)
{
    float molten = 1.0 - smoothstep(0.35, 0.95, cooled);
    if (molten <= 0.0)
    {
        return 0.0;
    }
    float swell = lavaNoise(plate * 0.42 + float2(t * 0.05, -t * 0.035)) + 0.5 * lavaNoise(plate * 0.9 - float2(t * 0.07, t * 0.045) + 4.0);
    return 0.12 * molten * (swell - 0.75);
}

// ---------------------------------------------------------------- Bubbles

// About how long a lava lake's surface lasts before it turns over, in seconds.
static const float LAKE_TURNOVER = 15.0;
// How wide the squares are that lava's bubbles rise in, one at a time in
// each, in metres across its plates; and how long the ring a burst one
// leaves takes to spread and heal, in seconds. A bubble keeps to the middle
// half of its square and its ring spreads no further than half a square, so
// only the four squares nearest a place reach it.
static const float BUBBLE_CELL = 2.4;
static const float BUBBLE_HEAL = 1.8;

// How long the surface of a lava lake has lain at `p` at time `t`, against
// LAKE_TURNOVER: it turns over in patches some metres across, fresh in the
// middle of each, which wander.
float lakeAge(float2 p, float t)
{
    return 0.12 + 1.5 * smoothstep(0.15, 0.85, lavaNoise(p * 0.15 + t * 0.012));
}

// How far a lava lake's plates have drifted by time `t`, in metres.
float2 lakeDrift(float t)
{
    return float2(sin(t * 0.05), cos(t * 0.043)) * 3.0 + float2(t * 0.02, 0.0);
}

// What lava a place across its plates belongs to, for finding how long it
// has cooled: a lake at time `time`, or with `stream` a flow, which runs
// `pace` metres a second on level ground; and its Cooling, in seconds.
struct LavaField
{
    bool stream;
    float time;
    float pace;
    float cooling;
};

// How long the lava at `plate` across `field`'s plates, in metres, had
// cooled at time `when`, against its Cooling: 0 as it wells up, 1 as it
// crusts over. More than that upstream of where a flow sets out.
float lavaCooledAt(LavaField field, float2 plate, float when)
{
    if (field.stream)
    {
        // Down its middle, where its bubbles rise, which cools half as fast as its Cooling says.
        float age = when - plate.x / field.pace;
        return age < 0.0 ? 8.0 : age * 0.5 / field.cooling;
    }
    float2 p = plate - lakeDrift(when);
    return LAKE_TURNOVER * lakeAge(p, when) / field.cooling;
}

// A bubble of gas coming up through lava: where its middle is across the
// plates, in metres; how wide it swells; how many seconds ago it began to,
// and how many it swells for before it bursts; its own numbers, from 0 to
// 1; and whether it is a vent's, which spits one small bubble after another
// from the same place.
struct LavaBubble
{
    float2 middle;
    float radius;
    float begun;
    float swells;
    float3 own;
    bool vent;
};

// The bubble whose turn it is in square `cell` of `field`'s lava; whether
// it rises at all is lavaBubbleRises.
LavaBubble lavaBubble(LavaField field, float2 cell)
{
    float2 a = lavaCell(cell, CELL_BUBBLE);
    float2 b = lavaCell(cell, CELL_BUBBLE + 1);
    LavaBubble bubble;
    bubble.vent = b.y < 0.06;
    float period = bubble.vent ? lerp(2.0, 2.8, a.x) : lerp(4.5, 8.0, a.x);
    float into = field.time + a.y * period;
    float turn = floor(into / period);
    // Its own numbers, new each turn.
    uint turned = uint(int(turn)) * 2u + CELL_BUBBLE + 2;
    float2 c = lavaCell(cell, turned);
    float2 d = lavaCell(cell, turned + 1);
    bubble.begun = into - turn * period;
    bubble.middle = (cell + 0.25 + 0.5 * (bubble.vent ? b.xx * float2(1.0, 0.6) + float2(0.0, 0.3 * a.x) : c)) * BUBBLE_CELL;
    bubble.radius = bubble.vent ? lerp(0.14, 0.24, d.x) : lerp(0.2, 0.55, d.x * d.x);
    // Time left, once it bursts, for its ring to heal and what it threw up to fall back before the next begins.
    bubble.swells = bubble.vent ? 0.15 * period : min(lerp(1.6, 3.2, d.y), period - BUBBLE_HEAL - 0.2);
    bubble.own = float3(c, d.y);
    return bubble;
}

// Whether `bubble` rises: only where the lava was still molten as it began
// to, the more of them the fresher it was; a vent's wherever it was.
bool lavaBubbleRises(LavaField field, LavaBubble bubble)
{
    float cooled = lavaCooledAt(field, bubble.middle, field.time - bubble.begun);
    return bubble.vent ? cooled < 0.85 : bubble.own.y < (1.0 - smoothstep(0.3, 1.0, cooled)) * 0.9;
}

// The dome a bubble has swelled into before it bursts: how far out from its
// middle it reaches (x) and how high its top stands (y), in metres. It
// rises slowly at first, then faster as its skin thins.
float2 lavaDome(LavaBubble bubble)
{
    float k = saturate(bubble.begun / bubble.swells);
    float rise = k * k * (0.35 + 0.65 * k);
    return float2(bubble.radius * (0.55 + 0.45 * sqrt(k)), bubble.radius * 0.9 * rise);
}

// How bubble `bubble` shapes the lava `off` metres from its middle: how
// high it raises it, in metres (x); how much hotter it shows, from 0 to 1
// (y); and how much it has broken any crust there open, from 0 to 1 (z). It
// swells slowly into a dome, its skin thinning and brightening towards its
// top, and bursts; the ring it leaves spreads and sinks back as the fresh
// rock inside it skins over.
float3 lavaBubbleShape(LavaBubble bubble, float off)
{
    float r = bubble.radius;
    if (bubble.begun < bubble.swells)
    {
        float2 dome = lavaDome(bubble);
        float inside = saturate(1.0 - off * off / (dome.x * dome.x));
        // Its skin thins as it stretches, and glows through: most at its top, and most just before it bursts.
        float thin = smoothstep(0.3, 1.0, bubble.begun / bubble.swells) * sqrt(inside);
        float open = (1.0 - smoothstep(dome.x, dome.x * 1.25, off)) * smoothstep(0.0, 0.1, dome.y / r);
        return float3(dome.y * pow(inside, 0.65), thin * 0.85, open);
    }
    float healed = (bubble.begun - bubble.swells) / BUBBLE_HEAL;
    if (healed >= 1.0)
    {
        return 0.0;
    }
    float left = (1.0 - healed) * (1.0 - healed);
    float ringAt = r * (1.0 + 1.2 * healed);
    float ring = exp(-pow((off - ringAt) / (0.2 * r + 0.04), 2.0));
    float inside = 1.0 - smoothstep(ringAt * 0.55, ringAt, off);
    float open = (1.0 - smoothstep(ringAt * 1.05, ringAt * 1.3, off)) * (1.0 - smoothstep(0.6, 1.0, healed));
    return float3((ring * 0.2 - inside * 0.07 * (1.0 - healed)) * r * left, 0.8 * inside * left, open);
}

// The bubbles round `plate` across `field`'s lava, in metres: how high they
// raise it there, in metres (x), how much hotter it shows (y) and how much
// they have broken its crust open (z). A flow's plates are `crowded`
// plates' metres a metre along it, and its bubbles keep to the middle
// `half` metres either side. The lava's own points are too far apart to
// take these shapes, a dome or the narrow ring a burst leaves, without
// standing up as corners: a dome is a mesh of its own (water.slang's
// lavaDomeVertex), and a ring is only shaded.
float3 lavaBubbles(LavaField field, float2 plate, float crowded, float half)
{
    float2 at = plate / BUBBLE_CELL;
    float2 n = floor(at);
    float2 nearer = select(frac(at) < 0.5, -1.0, 1.0);
    float3 shaped = 0.0;
    for (uint k = 0; k < 4; k++)
    {
        LavaBubble bubble = lavaBubble(field, n + nearer * float2(float(k & 1), float(k >> 1)));
        float2 from = plate - bubble.middle;
        float3 shape = lavaBubbleShape(bubble, length(float2(from.x / crowded, from.y)));
        bool reaches = any(shape != 0.0) && !(field.stream && abs(bubble.middle.y) + bubble.radius > half);
        if (reaches && lavaBubbleRises(field, bubble))
        {
            shaped = float3(shaped.x + shape.x, max(shaped.yz, shape.yz));
        }
    }
    return shaped;
}
