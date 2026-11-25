// A world generator's graph run on the GPU, a place at a time: the steps its height, climate and rock compile to
// (voxear_terrain's generate/gpu.rs), read from a buffer of words and run in turn, each writing a slot.
//
// Each kernel here works as its CPU kernel does, from the same numbers in the same order, so a place gets the
// height the CPU gives it to within a rounding or two: the noise is the terrain's gradient noise and its fractal
// kinds (noise.rs), the curves its smooth curves, the gullies, cells, landmass and climate its own.
//
// A step's words: what it does, how many slots it reads and which, the slot it writes, how many numbers it has and
// the numbers. The operations are numbered as GpuOp numbers them.

static const uint GENERATOR_SLOTS = 48;

static const uint OP_X = 0;
static const uint OP_Z = 1;
static const uint OP_COPY = 2;
static const uint OP_CONSTANT = 3;
static const uint OP_MATH = 4;
static const uint OP_NOISE = 5;
static const uint OP_CURVE = 6;
static const uint OP_WARP = 7;
static const uint OP_SHIFT = 8;
static const uint OP_GULLIES = 9;
static const uint OP_CLAMP = 10;
static const uint OP_BLEND = 11;
static const uint OP_TERRACE = 12;
static const uint OP_HEIGHT_MASK = 13;
static const uint OP_CELLS = 14;
static const uint OP_WARMTH = 15;
static const uint OP_WETNESS = 16;
static const uint OP_MILD = 17;
static const uint OP_LANDMASS = 18;
static const uint OP_FALLOFF = 19;
static const uint OP_Y = 20;
static const uint OP_SIZE = 21;
static const uint OP_CARRIED = 22;
static const uint OP_UNDER = 23;
static const uint OP_JOIN = 24;
static const uint OP_NOISE3D = 25;
static const uint OP_STEEP = 26;
static const uint OP_PUSH = 27;
static const uint OP_TUNNELS = 28;
static const uint OP_CHAMBERS = 29;

// noise.rs: a lattice point's hash, the gradient noise and its fractal kinds.

uint latticeHash(int x, int z, uint seed)
{
    uint h = seed * 0x9E3779B9u ^ uint(x) * 0x85EBCA6Bu ^ uint(z) * 0xC2B2AE35u;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    return h ^ (h >> 16);
}

float fadeOf(float t)
{
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

float2 gradientOf(uint h)
{
    const float d = 0.70710677;
    switch (h & 7u)
    {
    case 0: return float2(1.0, 0.0);
    case 1: return float2(d, d);
    case 2: return float2(0.0, 1.0);
    case 3: return float2(-d, d);
    case 4: return float2(-1.0, 0.0);
    case 5: return float2(-d, -d);
    case 6: return float2(0.0, -1.0);
    default: return float2(d, -d);
    }
}

float gradientNoise(float x, float z, uint seed)
{
    float fx = floor(x);
    float fz = floor(z);
    int ix = int(fx);
    int iz = int(fz);
    float tx = x - fx;
    float tz = z - fz;
    float2 g00 = gradientOf(latticeHash(ix, iz, seed));
    float2 g10 = gradientOf(latticeHash(ix + 1, iz, seed));
    float2 g01 = gradientOf(latticeHash(ix, iz + 1, seed));
    float2 g11 = gradientOf(latticeHash(ix + 1, iz + 1, seed));
    float c00 = g00.x * tx + g00.y * tz;
    float c10 = g10.x * (tx - 1.0) + g10.y * tz;
    float c01 = g01.x * tx + g01.y * (tz - 1.0);
    float c11 = g11.x * (tx - 1.0) + g11.y * (tz - 1.0);
    float u = fadeOf(tx);
    float v = fadeOf(tz);
    float top = c00 + (c10 - c00) * u;
    float bottom = c01 + (c11 - c01) * u;
    return (top + (bottom - top) * v) * 1.4142135;
}

// Style 0 fractal, 1 ridged, 2 billow; 0 to 1.
float fractalNoise(uint style, float x, float z, uint octaves, uint seed)
{
    float sum = 0.0;
    float total = 0.0;
    float amplitude = 1.0;
    float frequency = 1.0;
    float weight = 1.0;
    uint count = clamp(octaves, 1u, 12u);
    for (uint octave = 0; octave < count; octave++)
    {
        float n = gradientNoise(x * frequency, z * frequency, seed + octave * 0x632BE5ABu);
        float value;
        if (style == 0)
        {
            value = n * 0.5 + 0.5;
        }
        else if (style == 2)
        {
            value = abs(n);
        }
        else
        {
            float ridge = 1.0 - abs(n);
            ridge = ridge * ridge * weight;
            weight = clamp(ridge * 2.0, 0.0, 1.0);
            value = ridge;
        }
        sum += value * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return clamp(sum / total, 0.0, 1.0);
}

// noise.rs in three dimensions: a lattice point's hash, gradient noise through space and its fractal kinds.

uint latticeHash3(int x, int y, int z, uint seed)
{
    return latticeHash(int(uint(x) ^ (uint(y) * 0x2C1B3C6Du)), z, seed + uint(y));
}

float3 gradient3Of(uint h)
{
    switch (h % 12u)
    {
    case 0: return float3(1.0, 1.0, 0.0);
    case 1: return float3(-1.0, 1.0, 0.0);
    case 2: return float3(1.0, -1.0, 0.0);
    case 3: return float3(-1.0, -1.0, 0.0);
    case 4: return float3(1.0, 0.0, 1.0);
    case 5: return float3(-1.0, 0.0, 1.0);
    case 6: return float3(1.0, 0.0, -1.0);
    case 7: return float3(-1.0, 0.0, -1.0);
    case 8: return float3(0.0, 1.0, 1.0);
    case 9: return float3(0.0, -1.0, 1.0);
    case 10: return float3(0.0, 1.0, -1.0);
    default: return float3(0.0, -1.0, -1.0);
    }
}

float corner3(int3 i, float3 t, int dx, int dy, int dz, uint seed)
{
    float3 g = gradient3Of(latticeHash3(i.x + dx, i.y + dy, i.z + dz, seed));
    return g.x * (t.x - float(dx)) + g.y * (t.y - float(dy)) + g.z * (t.z - float(dz));
}

float gradientNoise3(float x, float y, float z, uint seed)
{
    float3 f = floor(float3(x, y, z));
    int3 i = int3(f);
    float3 t = float3(x, y, z) - f;
    float u = fadeOf(t.x);
    float v = fadeOf(t.y);
    float w = fadeOf(t.z);
    float c000 = corner3(i, t, 0, 0, 0, seed);
    float c100 = corner3(i, t, 1, 0, 0, seed);
    float c010 = corner3(i, t, 0, 1, 0, seed);
    float c110 = corner3(i, t, 1, 1, 0, seed);
    float c001 = corner3(i, t, 0, 0, 1, seed);
    float c101 = corner3(i, t, 1, 0, 1, seed);
    float c011 = corner3(i, t, 0, 1, 1, seed);
    float c111 = corner3(i, t, 1, 1, 1, seed);
    float a = c000 + (c100 - c000) * u;
    float b = c010 + (c110 - c010) * u;
    float near = a + (b - a) * v;
    float c = c001 + (c101 - c001) * u;
    float d = c011 + (c111 - c011) * u;
    float far = c + (d - c) * v;
    return near + (far - near) * w;
}

float fractalNoise3(uint style, float3 at, uint octaves, uint seed)
{
    float sum = 0.0;
    float total = 0.0;
    float amplitude = 1.0;
    float frequency = 1.0;
    float weight = 1.0;
    uint count = clamp(octaves, 1u, 12u);
    for (uint octave = 0; octave < count; octave++)
    {
        float3 p = at * frequency;
        float n = gradientNoise3(p.x, p.y, p.z, seed + octave * 0x632BE5ABu);
        float value;
        if (style == 0)
        {
            value = n * 0.5 + 0.5;
        }
        else if (style == 2)
        {
            value = abs(n);
        }
        else
        {
            float ridge = 1.0 - abs(n);
            ridge = ridge * ridge * weight;
            weight = clamp(ridge * 2.0, 0.0, 1.0);
            value = ridge;
        }
        sum += value * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return clamp(sum / total, 0.0, 1.0);
}

// landmass.rs: a part's seed, noise as -1 to 1, and its ridges with soft crests.

uint seedOf(uint seed, uint part)
{
    uint h = seed * 0x9E3779B9u ^ part * 0x85EBCA6Bu;
    h ^= h >> 15;
    h *= 0x2C1B3C6Du;
    return h ^ (h >> 12);
}

float signedOf(float value)
{
    return clamp((value - 0.5) * 2.0 * 2.6, -1.0, 1.0);
}

float easeBetween(float edge0, float edge1, float x)
{
    if (edge1 <= edge0)
    {
        return x >= edge0 ? 1.0 : 0.0;
    }
    float t = clamp((x - edge0) / (edge1 - edge0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

float softRidged(float x, float z, uint octaves, uint seed, float soft)
{
    float sum = 0.0;
    float total = 0.0;
    float amplitude = 1.0;
    float frequency = 1.0;
    float weight = 1.0;
    uint count = clamp(octaves, 1u, 12u);
    for (uint octave = 0; octave < count; octave++)
    {
        float n = gradientNoise(x * frequency, z * frequency, seed + octave * 0x632BE5ABu);
        float crease = abs(n) < soft ? (n * n / soft + soft) * 0.5 : abs(n);
        float ridge = max(1.0 - crease, 0.0);
        ridge = ridge * ridge * weight;
        weight = clamp(ridge * 2.0, 0.0, 1.0);
        sum += ridge * amplitude;
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    return clamp(sum / total, 0.0, 1.0);
}

// A curve kept in the program at `at`: how many keys, then each key's place and value.

float2 keyOf(uint* program, uint at, uint i)
{
    return float2(asfloat(program[at + 1 + i * 2]), asfloat(program[at + 2 + i * 2]));
}

float secantOf(uint* program, uint at, uint i)
{
    float2 a = keyOf(program, at, i);
    float2 b = keyOf(program, at, i + 1);
    return b.x - a.x > 1e-6 ? (b.y - a.y) / (b.x - a.x) : 0.0;
}

float tangentOf(uint* program, uint at, uint i, uint count)
{
    if (i == 0)
    {
        return secantOf(program, at, 0);
    }
    if (i == count - 1)
    {
        return secantOf(program, at, i - 1);
    }
    float before = secantOf(program, at, i - 1);
    float after = secantOf(program, at, i);
    return before * after <= 0.0 ? 0.0 : (before + after) * 0.5;
}

// The curve at `t`, as landmass::smooth works it out: cubics between keys that never overshoot them.
float curveAt(uint* program, uint at, float t, float empty)
{
    uint count = program[at];
    if (count == 0)
    {
        return empty;
    }
    if (count == 1)
    {
        return keyOf(program, at, 0).y;
    }
    float2 first = keyOf(program, at, 0);
    float2 last = keyOf(program, at, count - 1);
    if (t <= first.x)
    {
        return first.y;
    }
    if (t >= last.x)
    {
        return last.y;
    }
    uint i = count - 2;
    for (uint k = 0; k + 1 < count; k++)
    {
        if (t <= keyOf(program, at, k + 1).x)
        {
            i = k;
            break;
        }
    }
    float slope = secantOf(program, at, i);
    float m0 = tangentOf(program, at, i, count);
    float m1 = tangentOf(program, at, i + 1, count);
    if (slope == 0.0)
    {
        m0 = 0.0;
        m1 = 0.0;
    }
    else
    {
        float alpha = m0 / slope;
        float beta = m1 / slope;
        float size = alpha * alpha + beta * beta;
        if (size > 9.0)
        {
            float shrink = 3.0 / sqrt(size);
            m0 = alpha * shrink * slope;
            m1 = beta * shrink * slope;
        }
    }
    float2 a = keyOf(program, at, i);
    float2 b = keyOf(program, at, i + 1);
    float span = b.x - a.x;
    if (span <= 1e-6)
    {
        return b.y;
    }
    float s = (t - a.x) / span;
    float s2 = s * s;
    float s3 = s * s * s;
    float h00 = 2.0 * s3 - 3.0 * s2 + 1.0;
    float h10 = s3 - 2.0 * s2 + s;
    float h01 = -2.0 * s3 + 3.0 * s2;
    float h11 = s3 - s2;
    return h00 * a.y + h10 * span * m0 + h01 * b.y + h11 * span * m1;
}

// The words a curve kept at `at` takes.
uint curveWords(uint* program, uint at)
{
    return 1 + program[at] * 2;
}

// landmass.rs: gullies down a slope, and ground cut by them.

uint gullyHash(int x, int z, uint seed)
{
    uint h = seed ^ uint(x) * 0x8DA6B343u ^ uint(z) * 0xD8163841u;
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    return h ^ (h >> 16);
}

float3 gulliesAt(float x, float z, float2 along, uint seed, bool joined)
{
    const float reach = 1.5;
    float2 across = float2(-along.y, along.x);
    float reach2 = reach * reach;
    float sum = 0.0;
    float weights = 0.0;
    float2 dsum = float2(0.0, 0.0);
    float2 dweights = float2(0.0, 0.0);
    int x0 = int(floor(x - reach));
    int x1 = int(floor(x + reach));
    int z0 = int(floor(z - reach));
    int z1 = int(floor(z + reach));
    for (int cz = z0; cz <= z1; cz++)
    {
        for (int cx = x0; cx <= x1; cx++)
        {
            uint h = gullyHash(cx, cz, seed);
            float px = float(cx) + float(h & 0xFFFFu) / 65536.0;
            float pz = float(cz) + float((h >> 16) & 0xFFFFu) / 65536.0;
            float dx = x - px;
            float dz = z - pz;
            float r2 = dx * dx + dz * dz;
            if (r2 >= reach2)
            {
                continue;
            }
            float t = 1.0 - r2 / reach2;
            float w = t * t * t;
            float2 dw = float2(-6.0 * t * t * dx / reach2, -6.0 * t * t * dz / reach2);
            float phase = float(gullyHash(cx, cz, seed ^ 0x5bd1e995u) & 0xFFFFu) / 65536.0;
            float s = dx * across.x + dz * across.y + phase;
            float u = s - floor(s);
            float wave = 8.0 * u * (1.0 - u) - 1.0;
            float slope = 8.0 - 16.0 * u;
            if (joined)
            {
                float up = u < 0.5 ? u : 1.0 - u;
                if (up < 0.125)
                {
                    slope *= up * 8.0;
                }
            }
            sum += w * wave;
            weights += w;
            dsum.x += dw.x * wave + w * slope * across.x;
            dweights.x += dw.x;
            dsum.y += dw.y * wave + w * slope * across.y;
            dweights.y += dw.y;
        }
    }
    if (weights <= 1e-6)
    {
        return float3(0.0, 0.0, 0.0);
    }
    float value = sum / weights;
    float ddx = (dsum.x * weights - sum * dweights.x) / (weights * weights);
    float ddz = (dsum.y * weights - sum * dweights.y) / (weights * weights);
    return float3(value, ddx, ddz);
}

struct Gullied
{
    float strength;
    float spacing;
    uint octaves;
    float scale;
    float seaLevel;
    uint seed;
    bool joined;
};

float gullySpacing(Gullied cut)
{
    return max(cut.spacing * max(cut.scale, 0.01), 1.0);
}

// `height` at (x, z) with gullies cut down it, `around` its height a step off further and back along x, then z.
float gulliesCut(Gullied cut, float x, float z, float height, float4 around)
{
    float scale = max(cut.scale, 0.01);
    float spacing = gullySpacing(cut);
    float step = spacing * 0.15;
    float gx = (around.x - around.y) / (2.0 * step);
    float gz = (around.z - around.w) / (2.0 * step);
    float sum = 0.0;
    uint octaves = min(cut.octaves, 8u);
    for (uint octave = 0; octave < octaves; octave++)
    {
        float slope = sqrt(gx * gx + gz * gz);
        if (slope > 1e-4)
        {
            float2 along = float2(gx / slope, gz / slope);
            float3 g = gulliesAt(x / spacing, z / spacing, along, seedOf(cut.seed, 20 + octave), cut.joined);
            float depth = cut.strength * spacing * 0.25 * easeBetween(0.0, 0.6, slope);
            float share = (g.x - 1.0) * 0.5;
            sum += depth * share;
            gx += depth * 0.5 * g.y / spacing;
            gz += depth * 0.5 * g.z / spacing;
        }
        spacing *= 0.5;
    }
    float deepest = cut.strength * cut.spacing * scale * 0.25;
    float above = easeBetween(cut.seaLevel, cut.seaLevel + max(deepest, 1.0), height);
    return max(height + sum * above, min(height, cut.seaLevel));
}

// cells.rs: a scatter of points, one a square of the grid, and the distance to the nearest, its value or how far
// its cell's border is.

uint cellsAgain(uint h)
{
    h ^= h >> 16;
    h *= 0x7FEB352Du;
    h ^= h >> 15;
    h *= 0x846CA68Bu;
    return h ^ (h >> 16);
}

uint cellsMixed(int2 cell, uint seed)
{
    uint h = seed;
    h ^= uint(cell.x) * 0x8DA6B343u;
    h = (h ^ (h >> 15)) * 0x2C1B3C6Du;
    h ^= uint(cell.y) * 0xD8163841u;
    h = (h ^ (h >> 15)) * 0x2C1B3C6Du;
    return cellsAgain(h);
}

struct Found
{
    float2 from[25];
    uint cell[25];
    uint count;
};

void cellsGather(inout Found found, int2 cell, float2 inside, int reach, int within, float jitter, uint seed)
{
    int side = 2 * reach + 1;
    for (int index = 0; index < side * side; index++)
    {
        int2 off = int2(index % side - reach, (index / side) % side - reach);
        int furthest = max(abs(off.x), abs(off.y));
        if (furthest <= within && within > 0)
        {
            continue;
        }
        uint number = cellsMixed(cell + off, seed);
        float2 moved = (float2(float(number & 0xFFFFu), float(number >> 16)) / 65536.0 - 0.5) * jitter + 0.5;
        found.from[found.count] = float2(off) + moved - inside;
        found.cell[found.count] = number;
        found.count++;
    }
}

float2 cellsNearest(Found found)
{
    uint nearest = 0;
    float least = 3.402823e38;
    for (uint at = 0; at < found.count; at++)
    {
        float2 f = found.from[at];
        float far = f.x * f.x + f.y * f.y;
        if (far < least)
        {
            nearest = at;
            least = far;
        }
    }
    return float2(float(nearest), least);
}

float cellsBorder(Found found, uint nearest)
{
    float2 own = found.from[nearest];
    float least = 3.402823e38;
    for (uint at = 0; at < found.count; at++)
    {
        if (at == nearest)
        {
            continue;
        }
        float2 other = found.from[at];
        float2 apart = other - own;
        float along = (own.x + other.x) * 0.5 * apart.x + (own.y + other.y) * 0.5 * apart.y;
        float length = apart.x * apart.x + apart.y * apart.y;
        if (length > 1e-12)
        {
            least = min(least, along / sqrt(length));
        }
    }
    return least;
}

float cellsRead(uint what, float size, float jitter, uint seed, float2 place)
{
    int2 cell;
    float2 inside;
    float margin = 1.0;
    float2 scaled = place / size;
    float2 below = floor(scaled);
    cell = int2(below);
    inside = scaled - below;
    for (uint axis = 0; axis < 2; axis++)
    {
        float side = inside[axis] < 0.5 ? inside[axis] : 1.0 - inside[axis];
        if (side < margin)
        {
            margin = side;
        }
    }
    Found found;
    found.count = 0;
    cellsGather(found, cell, inside, 1, 0, jitter, seed);
    float2 near = cellsNearest(found);
    float reach = 1.0 + margin;
    // Distance 0, edges 1, value 2.
    if (what != 1)
    {
        if (near.y > reach * reach)
        {
            cellsGather(found, cell, inside, 2, 1, jitter, seed);
            near = cellsNearest(found);
        }
        if (what == 0)
        {
            return min(sqrt(near.y), 1.0);
        }
        return float(cellsAgain(found.cell[uint(near.x)] ^ 0x68E31DA4u) >> 8) / 16777216.0;
    }
    float border = cellsBorder(found, uint(near.x));
    if (near.y > reach * reach || border > (reach - sqrt(near.y)) * 0.5)
    {
        cellsGather(found, cell, inside, 2, 1, jitter, seed);
        near = cellsNearest(found);
        border = cellsBorder(found, uint(near.x));
    }
    return clamp(border * 2.0, 0.0, 1.0);
}

// landmass.rs: a landmass's ground. Its numbers from `at`: seed, scale, sea level, height; the continents',
// ruggedness' and ridges' sizes and octaves; sharpness, the warp's size and strength, crags; the erosion's
// strength, spacing and octaves; bumps; then the coast, inland, relief and peaks curves.

struct Landmass
{
    uint seed;
    float scale;
    float seaLevel;
    float height;
    float2 continents;
    float2 ruggedness;
    float2 ridges;
    float sharpness;
    float warpSize;
    float warpStrength;
    float crags;
    float erosionStrength;
    float erosionSpacing;
    uint erosionOctaves;
    float bumps;
    uint coast;
    uint inland;
    uint relief;
    uint peaks;
};

Landmass landmassOf(uint* program, uint at)
{
    Landmass land;
    land.seed = program[at];
    land.scale = asfloat(program[at + 1]);
    land.seaLevel = asfloat(program[at + 2]);
    land.height = asfloat(program[at + 3]);
    land.continents = float2(asfloat(program[at + 4]), float(program[at + 5]));
    land.ruggedness = float2(asfloat(program[at + 6]), float(program[at + 7]));
    land.ridges = float2(asfloat(program[at + 8]), float(program[at + 9]));
    land.sharpness = asfloat(program[at + 10]);
    land.warpSize = asfloat(program[at + 11]);
    land.warpStrength = asfloat(program[at + 12]);
    land.crags = asfloat(program[at + 13]);
    land.erosionStrength = asfloat(program[at + 14]);
    land.erosionSpacing = asfloat(program[at + 15]);
    land.erosionOctaves = program[at + 16];
    land.bumps = asfloat(program[at + 17]);
    land.coast = at + 18;
    land.inland = land.coast + curveWords(program, land.coast);
    land.relief = land.inland + curveWords(program, land.inland);
    land.peaks = land.relief + curveWords(program, land.relief);
    return land;
}

float landFractal(Landmass land, uint style, float size, uint octaves, uint part, float2 spot)
{
    float scaled = max(size * max(land.scale, 0.01), 1.0);
    return fractalNoise(style, spot.x / scaled, spot.y / scaled, octaves, seedOf(land.seed, part));
}

// Its height before gullies and bumps, and how much relief it has.
float2 landShaped(uint* program, Landmass land, float2 spot)
{
    float scale = max(land.scale, 0.01);
    float2 bent = spot;
    if (land.warpStrength > 0.0)
    {
        float reach = land.warpStrength * scale;
        float bx = signedOf(landFractal(land, 0, land.warpSize, 3, 1, spot)) * reach;
        float bz = signedOf(landFractal(land, 0, land.warpSize, 3, 2, spot)) * reach;
        bent = float2(spot.x + bx, spot.y + bz);
    }
    float continents = signedOf(landFractal(land, 0, land.continents.x, uint(land.continents.y), 3, bent));
    float ruggedness = signedOf(landFractal(land, 0, land.ruggedness.x, uint(land.ruggedness.y), 4, bent));
    float size = max(land.ridges.x * scale, 1.0);
    float soft = (1.0 - clamp(land.sharpness, 0.0, 1.0)) * 0.35;
    float ridged = softRidged(bent.x / size, bent.y / size, uint(land.ridges.y), seedOf(land.seed, 5), soft);
    float peaks = clamp((ridged - 0.45) * 2.4, -1.0, 1.0);
    float across = clamp((continents + 1.0) * 0.5, 0.0, 1.0);
    float ground = curveAt(program, land.coast, across, 0.0);
    float relief = clamp(curveAt(program, land.inland, across, 1.0) * curveAt(program, land.relief, (ruggedness + 1.0) * 0.5, 1.0), 0.0, 1.0);
    float shape = curveAt(program, land.peaks, (peaks + 1.0) * 0.5, 0.0);
    float share = ground + relief * shape;
    if (land.crags > 0.0 && peaks > 0.0)
    {
        float crag = landFractal(land, 1, land.ridges.x / 8.0, 3, 6, bent);
        share += land.crags * relief * peaks * peaks * crag;
    }
    return float2(land.seaLevel + share * land.height, relief);
}

float landHeight(uint* program, uint at, float x, float z)
{
    Landmass land = landmassOf(program, at);
    float2 shaped = landShaped(program, land, float2(x, z));
    float height = shaped.x;
    if (land.erosionStrength > 0.0 && land.erosionOctaves > 0)
    {
        Gullied cut;
        cut.strength = land.erosionStrength;
        cut.spacing = land.erosionSpacing;
        cut.octaves = land.erosionOctaves;
        cut.scale = land.scale;
        cut.seaLevel = land.seaLevel;
        cut.seed = land.seed;
        cut.joined = false;
        float step = gullySpacing(cut) * 0.15;
        float4 around = float4(
            landShaped(program, land, float2(x + step, z)).x,
            landShaped(program, land, float2(x - step, z)).x,
            landShaped(program, land, float2(x, z + step)).x,
            landShaped(program, land, float2(x, z - step)).x);
        height = gulliesCut(cut, x, z, height, around);
    }
    if (land.bumps != 0.0)
    {
        float size = 30.0 * max(land.scale, 0.01);
        float bump = signedOf(fractalNoise(0, x / size, z / size, 4, seedOf(land.seed, 7)));
        height += land.bumps * bump * (0.3 + 0.7 * shaped.y);
    }
    return height;
}

float heldBetween(float value, float lowest, float highest, float smooth)
{
    if (smooth <= 0.0)
    {
        return value < lowest ? lowest : (value > highest ? highest : value);
    }
    float raised;
    if (value <= lowest - smooth)
    {
        raised = lowest;
    }
    else if (value >= lowest + smooth)
    {
        raised = value;
    }
    else
    {
        float bend = value - lowest + smooth;
        raised = lowest + bend * bend / (4.0 * smooth);
    }
    if (raised >= highest + smooth)
    {
        return highest;
    }
    if (raised <= highest - smooth)
    {
        return raised;
    }
    float bend = highest + smooth - raised;
    return highest - bend * bend / (4.0 * smooth);
}

// What a generator makes at a place: the ground's height in metres, how warm it is in °C, how wet from 0 to 1, and
// its rock mask (0 with none).
struct Generated
{
    float height;
    float warmth;
    float wetness;
    float rock;
};

// Runs the `words` words of `program` at (x, y, z), in metres in the generator's world, into `slots`: for a cell, `y`
// is its middle's height, `size` how large it is and `carried` what its column carries up, as the bits of each.
void runSteps(uint* program, uint words, float x, float y, float z, float size, uint* carried, inout float slots[GENERATOR_SLOTS])
{
    uint at = 0;
    while (at < words)
    {
        uint op = program[at];
        uint count = program[at + 1];
        float a[8];
        for (uint i = 0; i < 8; i++)
        {
            a[i] = i < count ? slots[program[at + 2 + i]] : 0.0;
        }
        uint written = program[at + 2 + count];
        uint numbers = program[at + 3 + count];
        uint p = at + 4 + count;
        float value = 0.0;
        switch (op)
        {
        case OP_X:
            value = x;
            break;
        case OP_Z:
            value = z;
            break;
        case OP_COPY:
            value = a[min(program[p], 7u)];
            break;
        case OP_CONSTANT:
            value = asfloat(program[p]);
            break;
        case OP_MATH:
        {
            uint operation = program[p];
            if (operation == 0)
                value = a[0] + a[1];
            else if (operation == 1)
                value = a[0] - a[1];
            else if (operation == 2)
                value = a[0] * a[1];
            else if (operation == 3)
                value = a[0] < a[1] ? a[0] : a[1];
            else if (operation == 4)
                value = a[0] > a[1] ? a[0] : a[1];
            else
                value = a[1] == 0.0 ? 0.0 : a[0] / a[1];
            break;
        }
        case OP_NOISE:
        {
            float size = asfloat(program[p + 1]);
            float n = fractalNoise(program[p], a[0] / size, a[1] / size, program[p + 2], program[p + 3]);
            value = asfloat(program[p + 4]) + n * asfloat(program[p + 5]);
            break;
        }
        case OP_CURVE:
        {
            float low = asfloat(program[p]);
            float span = asfloat(program[p + 1]);
            float along = span == 0.0 ? 0.0 : (a[0] - low) / span;
            value = curveAt(program, p + 2, along, 0.0);
            break;
        }
        case OP_WARP:
        {
            uint axis = program[p];
            float size = asfloat(program[p + 1]);
            float n = fractalNoise(0, a[0] / size, a[1] / size, program[p + 3], program[p + 4]);
            value = a[min(axis, 1u)] + signedOf(n) * asfloat(program[p + 2]);
            break;
        }
        case OP_SHIFT:
            value = a[0] + asfloat(program[p]);
            break;
        case OP_GULLIES:
        {
            Gullied cut;
            cut.strength = asfloat(program[p]);
            cut.spacing = asfloat(program[p + 1]);
            cut.octaves = program[p + 2];
            cut.scale = asfloat(program[p + 3]);
            cut.seaLevel = asfloat(program[p + 4]);
            cut.seed = program[p + 5];
            cut.joined = program[p + 6] != 0;
            value = gulliesCut(cut, a[0], a[1], a[2], float4(a[3], a[4], a[5], a[6]));
            break;
        }
        case OP_CLAMP:
            value = heldBetween(a[0], asfloat(program[p]), asfloat(program[p + 1]), asfloat(program[p + 2]));
            break;
        case OP_BLEND:
            value = a[2] <= 0.0 ? a[0] : (a[2] >= 1.0 ? a[1] : a[0] + (a[1] - a[0]) * a[2]);
            break;
        case OP_TERRACE:
        {
            float step = asfloat(program[p]);
            float riser = asfloat(program[p + 1]);
            float tread = (1.0 - riser) * 0.5;
            float steps = a[0] / step;
            float whole = floor(steps);
            float past = steps - whole;
            float risen = past <= tread ? 0.0 : (past >= 1.0 - tread ? 1.0 : (past - tread) / riser);
            value = (whole + risen) * step;
            break;
        }
        case OP_HEIGHT_MASK:
        {
            float low = asfloat(program[p]);
            float high = asfloat(program[p + 1]);
            float feather = asfloat(program[p + 2]);
            value = easeBetween(low - feather, low, a[0]) * (1.0 - easeBetween(high, high + feather, a[0]));
            break;
        }
        case OP_CELLS:
            value = cellsRead(program[p], asfloat(program[p + 1]), asfloat(program[p + 2]), program[p + 3], float2(a[0], a[1]));
            break;
        case OP_WARMTH:
        {
            float size = asfloat(program[p]);
            float read = signedOf(fractalNoise(0, a[0] / size, a[1] / size, 3, program[p + 1]));
            float warmth = asfloat(program[p + 2]) + read * asfloat(program[p + 3]);
            float above = a[2] - asfloat(program[p + 5]);
            value = warmth - asfloat(program[p + 4]) * max(above, 0.0) / 1000.0;
            break;
        }
        case OP_WETNESS:
        {
            float size = asfloat(program[p]);
            float read = signedOf(fractalNoise(0, a[0] / size, a[1] / size, 3, program[p + 1]));
            value = clamp(asfloat(program[p + 2]) + read * asfloat(program[p + 3]), 0.0, 1.0);
            break;
        }
        case OP_MILD:
            value = 15.0 - 6.5 * a[0] / 1000.0;
            break;
        case OP_LANDMASS:
            value = landHeight(program, p, a[0], a[1]);
            break;
        case OP_FALLOFF:
        {
            float dx = a[0] - asfloat(program[p + 1]);
            float dz = a[1] - asfloat(program[p + 2]);
            float far = program[p] != 0 ? max(abs(dx), abs(dz)) : sqrt(dx * dx + dz * dz);
            float radius = asfloat(program[p + 3]);
            value = 1.0 - easeBetween(radius, radius + asfloat(program[p + 4]), far);
            break;
        }
        case OP_Y:
            value = y;
            break;
        case OP_SIZE:
            value = size;
            break;
        case OP_CARRIED:
            value = asfloat(carried[program[p]]);
            break;
        case OP_UNDER:
            value = a[0] - a[1];
            break;
        case OP_JOIN:
        {
            uint operation = program[p];
            float blend = asfloat(program[p + 1]);
            if (operation == 0)
            {
                value = a[0] > a[1] ? a[0] : a[1];
            }
            else if (operation == 1)
            {
                value = a[0] < -a[1] ? a[0] : -a[1];
            }
            else
            {
                float greater = a[0] > a[1] ? a[0] : a[1];
                float apart = a[0] > a[1] ? a[0] - a[1] : a[1] - a[0];
                float near = apart < blend ? (blend - apart) / blend : 0.0;
                value = greater + near * near * blend * 0.25;
            }
            break;
        }
        case OP_NOISE3D:
        {
            float size3 = asfloat(program[p + 1]);
            float n = fractalNoise3(program[p], float3(a[0], a[1], a[2]) / size3, program[p + 2], program[p + 3]);
            value = asfloat(program[p + 4]) + n * asfloat(program[p + 5]);
            break;
        }
        case OP_STEEP:
        {
            float reach = asfloat(program[p]);
            float along = (a[0] - a[1]) / (2.0 * reach);
            float across = (a[2] - a[3]) / (2.0 * reach);
            float climb = sqrt(along * along + across * across);
            float t = clamp((climb - 0.55) / (1.0 - 0.55), 0.0, 1.0);
            value = t * t * (3.0 - 2.0 * t);
            break;
        }
        case OP_PUSH:
        {
            float amount = asfloat(program[p]);
            float noiseSize = asfloat(program[p + 1]);
            uint octaves = program[p + 2];
            uint seed = program[p + 3];
            float ground = a[0];
            float mask = a[1];
            float cell = a[5];
            value = ground;
            float beyond = amount + 3.0 * cell;
            if (!(mask <= 0.0 || isnan(mask) || ground >= beyond || ground <= -beyond))
            {
                float sum = 0.0;
                float total = 0.0;
                float strength = 1.0;
                float size3 = noiseSize;
                for (uint octave = 0; octave < octaves; octave++)
                {
                    float held = cell > 0.0 ? clamp((size3 / cell - 2.0) * 0.5, 0.0, 1.0) : 1.0;
                    if (held > 0.0)
                    {
                        sum += gradientNoise3(a[2] / size3, a[3] / size3, a[4] / size3, seed + octave * 0x632BE5ABu) * strength * held;
                    }
                    total += strength;
                    strength *= 0.5;
                    size3 *= 0.5;
                }
                float n = clamp(sum / total, -1.0, 1.0);
                value = ground + n * amount * clamp(mask, 0.0, 1.0);
            }
            break;
        }
        case OP_TUNNELS:
        {
            float t = asfloat(program[p]);
            float width = asfloat(program[p + 1]);
            float under = a[0] - asfloat(program[p + 2]);
            if (under <= -a[4])
            {
                value = under;
                break;
            }
            float3 q = float3(a[1], a[2], a[3]) / t;
            float apart = abs(gradientNoise3(q.x, q.y, q.z, program[p + 3])) + abs(gradientNoise3(q.x, q.y, q.z, program[p + 4]));
            float inside = -((apart - width) * t * 0.5);
            value = inside < under ? inside : under;
            break;
        }
        case OP_CHAMBERS:
        {
            float c = asfloat(program[p]);
            float level = asfloat(program[p + 1]);
            float under = a[0] - asfloat(program[p + 2]);
            if (under <= -a[4])
            {
                value = under;
                break;
            }
            float high = gradientNoise3(a[1] / c, a[2] / (c * 0.6), a[3] / c, program[p + 3]);
            float inside = -((level - high) * c * 0.5);
            value = inside < under ? inside : under;
            break;
        }
        default:
            break;
        }
        slots[min(written, GENERATOR_SLOTS - 1)] = value;
        at = p + numbers;
    }
}

// Runs the `words` words of `program` at (x, z), in metres in the generator's world; `outs` are the slots of its
// height, warmth, wetness and rock (rock past the slots for none).
Generated runGenerator(uint* program, uint words, uint4 outs, float x, float z)
{
    float slots[GENERATOR_SLOTS];
    for (uint s = 0; s < GENERATOR_SLOTS; s++)
    {
        slots[s] = 0.0;
    }
    runSteps(program, words, x, 0.0, z, 0.0, program, slots);
    Generated made;
    made.height = slots[min(outs.x, GENERATOR_SLOTS - 1)];
    made.warmth = slots[min(outs.y, GENERATOR_SLOTS - 1)];
    made.wetness = slots[min(outs.z, GENERATOR_SLOTS - 1)];
    made.rock = outs.w < GENERATOR_SLOTS ? slots[outs.w] : 0.0;
    return made;
}
