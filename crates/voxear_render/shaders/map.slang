// A map of a world of blocks, drawn as a small model of it (see src/map.rs): a slab of the ground cut out round a
// point, square or round, seen from a camera turned and tilted over it. Nothing is meshed: each pixel's ray walks
// the map's columns, a cell at a time, until it meets one. A column is its ground, the water over it and up to two
// pieces standing free of it, so what is met is always a block's top or its side, and the slab's cut sides show
// the ground's layers.
//
// Lit as a model on a table: one lamp fixed to the view, corners shaded by the blocks round them, sides darker
// where they stand on lower ground, and short shadows.
//
// Reads no textures and none of the frame's buffers: its push constants point at its own.

// One column of the map; mirrors GpuMapColumn in src/map.rs.
struct MapColumn
{
    // Where its ground ends, in cells; NO_COLUMN where nothing is.
    int top;
    // Where its water ends, `top` with none.
    int water;
    // Where its highest block ends.
    int highest;
    // Its top block, and the block under it in the high half.
    uint layers;
    // The block under those, and its water's block in the high half.
    uint deep;
    // How thick the top block is, and the one under it, a byte each.
    uint depths;
    // Each piece: where it starts and ends over the ground's top, 16 bits each (none where they are the same);
    // then its top block and its sides' block.
    uint piece0;
    uint piece0Blocks;
    uint piece1;
    uint piece1Blocks;
    // A colour its own are drawn towards, sRGB, and how far in the high byte: news of the place.
    uint tint;
};

static const int NO_COLUMN = int(0x80000000);
// The top of a column of ground the map does not know: drawn as paper, flat, as high as the slab's lowest ground,
// wherever the slab is cut.
static const int PAPER_COLUMN = int(0x80000001);

// What a map is drawn as this frame; mirrors GpuMapView in src/map.rs. Places are in cells from the window's
// corner along x and z, and from `base` up.
struct MapView
{
    float3 origin;
    // How far the slab reaches from its middle.
    float reach;
    float3 forward;
    // The slab's underside.
    float bottom;
    // To the right of the view, as long as half the view is wide at one cell's distance.
    float3 right;
    // The highest block's top in the slab.
    float ceiling;
    // Up the view, as long as half the view is high at one cell's distance.
    float3 up;
    // Rays a pixel along each side: 1 to 4.
    uint samples;
    float2 middle;
    float2 size;
    int2 corner;
    int base;
    // The window's cells along a side, a power of two: a cell is kept at its place modulo this.
    uint side;
    // Towards the lamp.
    float3 sun;
    // How dark its shadows are, 0 for none.
    float shadows;
    // Whether the slab is round, how many blocks the palette has, how dark edges are drawn, and the lowest
    // ground in the slab, which colours are a little darker towards.
    uint round;
    uint blocks;
    float edges;
    float lowest;
};

struct MapPush
{
    MapView* view;
    MapColumn* columns;
    // Each block's colour, sRGB, red lowest.
    uint* palette;
    // The highest block's top of each 8 by 8 cells of the window, kept as the columns are, then of each 32 by 32.
    int* tops;
    uint unused[8];
};

[[vk::push_constant]] ConstantBuffer<MapPush> mapPush;

static const uint HIT_NONE = 0;
static const uint HIT_TOP = 1;
static const uint HIT_SIDE = 2;
static const uint HIT_WATER_TOP = 3;
static const uint HIT_WATER_SIDE = 4;

struct MapHit
{
    uint kind;
    float t;
    float3 normal;
    int2 cell;
    uint block;
    // How high the face met stands (a top's own height), and for a side the ground's top it rises from.
    float height;
    float foot;
};

// How many cells a side the ray steps over at once where none reaches up to it, as powers of two: small groups,
// and large ones of several small.
static const int SKIPS[2] = { 3, 5 };

// The highest block's top in the group of cells `group`, small or large, from the map's base.
float groupTop(MapView* view, int2 group, int size)
{
    int shift = SKIPS[size];
    int across = int(view->side >> shift);
    int2 kept = (group + (view->corner >> shift)) & (across - 1);
    int first = size == 0 ? 0 : int(view->side >> SKIPS[0]) * int(view->side >> SKIPS[0]);
    int top = mapPush.tops[first + kept.y * across + kept.x];
    if (top == PAPER_COLUMN)
    {
        return ceil(view->lowest);
    }
    return top == NO_COLUMN ? -1e9 : float(top - view->base);
}

MapColumn columnAt(MapView* view, int2 cell)
{
    int2 kept = (cell + view->corner) & int(view->side - 1);
    MapColumn column = mapPush.columns[kept.y * int(view->side) + kept.x];
    if (column.top == PAPER_COLUMN)
    {
        column.top = view->base + int(ceil(view->lowest));
        column.water = column.top;
        column.highest = column.top;
    }
    return column;
}

float3 linearOf(float3 srgb)
{
    return lerp(srgb / 12.92, pow((srgb + 0.055) / 1.055, 2.4), step(0.04045, srgb));
}

float3 blockColor(MapView* view, uint block)
{
    uint packed = mapPush.palette[min(block, view->blocks - 1)];
    float3 srgb = float3(packed & 0xff, (packed >> 8) & 0xff, (packed >> 16) & 0xff) / 255.0;
    return linearOf(srgb);
}

// A piece's start and end, from the map's base.
float2 pieceSpan(MapColumn column, uint piece)
{
    float top = float(column.top - mapPush.view->base);
    return top + float2(piece & 0xffff, piece >> 16);
}

// Whether `column` holds a block at height `y`.
bool solidAt(MapColumn column, float y)
{
    if (column.top == NO_COLUMN)
    {
        return false;
    }
    if (y < float(column.top - mapPush.view->base))
    {
        return true;
    }
    float2 first = pieceSpan(column, column.piece0);
    float2 second = pieceSpan(column, column.piece1);
    return (y >= first.x && y < first.y) || (y >= second.x && y < second.y);
}

// Where the ray from `origin` along `direction` is inside the slab: the distances it comes in and goes out at,
// and the side it comes in by. Nothing where the first is not under the second.
float2 slabSpan(MapView* view, float3 origin, float3 direction, out float3 normal)
{
    normal = float3(0.0, 1.0, 0.0);
    // Between the ceiling and the underside.
    float invY = 1.0 / (abs(direction.y) > 1e-6 ? direction.y : -1e-6);
    float toCeiling = (view->ceiling - origin.y) * invY;
    float toBottom = (view->bottom - origin.y) * invY;
    float2 span = float2(max(min(toCeiling, toBottom), 0.0), max(toCeiling, toBottom));
    float2 from = origin.xz - view->middle;
    if (view->round != 0)
    {
        float a = dot(direction.xz, direction.xz);
        float b = dot(from, direction.xz);
        float c = dot(from, from) - view->reach * view->reach;
        float root = b * b - a * c;
        if (a < 1e-9 || root < 0.0)
        {
            // Straight down, or past the slab: inside all the way or not at all.
            return c < 0.0 ? span : float2(1.0, 0.0);
        }
        root = sqrt(root);
        float near = (-b - root) / a;
        if (near > span.x)
        {
            span.x = near;
            normal = float3(normalize(from + direction.xz * near), 0.0).xzy;
        }
        span.y = min(span.y, (-b + root) / a);
        return span;
    }
    for (int axis = 0; axis < 2; axis++)
    {
        float along = direction.xz[axis];
        float at = from[axis];
        if (abs(along) < 1e-9)
        {
            if (abs(at) > view->reach)
            {
                return float2(1.0, 0.0);
            }
            continue;
        }
        float first = (-view->reach - at) / along;
        float second = (view->reach - at) / along;
        float near = min(first, second);
        if (near > span.x)
        {
            span.x = near;
            normal = axis == 0 ? float3(-sign(along), 0.0, 0.0) : float3(0.0, 0.0, -sign(along));
        }
        span.y = min(span.y, max(first, second));
    }
    return span;
}

// What the ray from `origin` along `direction`, going down, meets first.
MapHit trace(MapView* view, float3 origin, float3 direction)
{
    MapHit hit;
    hit.kind = HIT_NONE;
    hit.t = 0.0;
    hit.normal = float3(0.0, 1.0, 0.0);
    hit.cell = int2(0, 0);
    hit.block = 0;
    hit.height = 0.0;
    hit.foot = 0.0;

    float3 normal;
    float2 span = slabSpan(view, origin, direction, normal);
    if (span.x >= span.y)
    {
        return hit;
    }
    float t = span.x;
    // A little inside the side it came in by: the cut often lies along the cells' borders, where rounding
    // would pick the cell outside it as often as the one inside.
    float3 start = origin + direction * t - float3(normal.x, 0.0, normal.z) * 2e-3;
    int2 cell = int2(floor(start.xz));
    // Whether the ray is still in the cell it came into the slab by, at the cut.
    bool atCut = normal.y == 0.0;
    int2 stride = int2(direction.x >= 0.0 ? 1 : -1, direction.z >= 0.0 ? 1 : -1);
    int2 ahead = int2(stride.x > 0 ? 1 : 0, stride.y > 0 ? 1 : 0);
    float2 inverse = 1.0 / max(abs(direction.xz), 1e-9);
    // How far along the ray each next cell border is, along x and z.
    float2 next = t + abs(float2(cell + ahead) - start.xz) * inverse;
    // The groups of cells last looked at whole, small and large.
    int2 looked[2] = { int2(0x7fffffff, 0x7fffffff), int2(0x7fffffff, 0x7fffffff) };

    uint most = view->side * 2 + 4;
    for (uint walked = 0; walked < most; walked++)
    {
        // Coming into a group of cells none of which reaches up to where the ray leaves it, the ray steps
        // over the group whole: a large one if it can, else a small one.
        bool stepped = false;
        bool left = false;
        for (int size = 1; size >= 0 && !stepped; size--)
        {
            int shift = SKIPS[size];
            int2 group = cell >> shift;
            if (all(group == looked[size]))
            {
                continue;
            }
            looked[size] = group;
            float2 border = float2((group + ahead) << shift);
            float2 across = t + abs(border - (origin.xz + direction.xz * t)) * inverse;
            float out_ = min(min(across.x, across.y), span.y);
            if (origin.y + direction.y * out_ < groupTop(view, group, size))
            {
                continue;
            }
            stepped = true;
            if (out_ >= span.y - 1e-3)
            {
                left = true;
                break;
            }
            t = out_;
            atCut = false;
            float2 at = origin.xz + direction.xz * t;
            if (across.x < across.y)
            {
                cell.x = ((group.x + ahead.x) << shift) - (stride.x > 0 ? 0 : 1);
                cell.y = int(floor(at.y));
                normal = float3(-float(stride.x), 0.0, 0.0);
            }
            else
            {
                cell.y = ((group.y + ahead.y) << shift) - (stride.y > 0 ? 0 : 1);
                cell.x = int(floor(at.x));
                normal = float3(0.0, 0.0, -float(stride.y));
            }
            next = t + abs(float2(cell + ahead) - at) * inverse;
        }
        if (left)
        {
            break;
        }
        if (stepped)
        {
            continue;
        }
        float leave = min(min(next.x, next.y), span.y);
        float yIn = origin.y + direction.y * t;
        float yOut = origin.y + direction.y * leave;
        MapColumn column = columnAt(view, cell);
        if (column.top != NO_COLUMN && yOut < float(column.highest - view->base))
        {
            float top = float(column.top - view->base);
            float water = float(column.water - view->base);
            float2 first = pieceSpan(column, column.piece0);
            float2 second = pieceSpan(column, column.piece1);
            hit.cell = cell;
            hit.normal = normal;
            hit.t = t;
            hit.height = yIn;
            // A side stands on the ground the ray came over, or on nothing at the slab's cut.
            hit.foot = -1e9;
            bool side = (yIn >= second.x && yIn < second.y) || (yIn >= first.x && yIn < first.y) || yIn < max(top, water);
            if (side && !atCut)
            {
                MapColumn before = columnAt(view, cell + int2(int(normal.x), int(normal.z)));
                hit.foot = before.top != NO_COLUMN ? float(max(before.top, before.water) - view->base) : -1e9;
            }
            // Coming in by its side: a piece's, the water's or the ground's.
            if (yIn >= second.x && yIn < second.y)
            {
                hit.kind = HIT_SIDE;
                hit.block = column.piece1Blocks >> 16;
                return hit;
            }
            if (yIn >= first.x && yIn < first.y)
            {
                hit.kind = HIT_SIDE;
                hit.block = column.piece0Blocks >> 16;
                return hit;
            }
            if (yIn < top)
            {
                float depth = top - yIn;
                // A step in the ground keeps its top block's colour a few blocks down, so a slope reads as one
                // surface; the slab's cut shows the layers as they are.
                float topDepth = float(column.depths & 0xff);
                topDepth = atCut ? topDepth : max(topDepth, 3.0);
                float underDepth = float((column.depths >> 8) & 0xff);
                hit.kind = HIT_SIDE;
                hit.block = depth < topDepth ? (column.layers & 0xffff)
                    : depth < topDepth + underDepth ? (column.layers >> 16) : (column.deep & 0xffff);
                return hit;
            }
            if (yIn < water)
            {
                hit.kind = HIT_WATER_SIDE;
                hit.block = column.deep >> 16;
                hit.foot = top;
                return hit;
            }
            // Going down onto a top: the highest the ray crosses in this cell.
            float met = -1e9;
            if (second.y > second.x && yIn >= second.y && yOut < second.y)
            {
                met = second.y;
                hit.block = column.piece1Blocks & 0xffff;
                hit.kind = HIT_TOP;
            }
            if (first.y > first.x && yIn >= first.y && yOut < first.y && first.y > met)
            {
                met = first.y;
                hit.block = column.piece0Blocks & 0xffff;
                hit.kind = HIT_TOP;
            }
            if (water > top && yIn >= water && yOut < water && water > met)
            {
                met = water;
                hit.block = column.deep >> 16;
                hit.kind = HIT_WATER_TOP;
                hit.foot = top;
            }
            if (yIn >= top && yOut < top && top > met)
            {
                met = top;
                hit.block = column.layers & 0xffff;
                hit.kind = HIT_TOP;
            }
            if (hit.kind != HIT_NONE)
            {
                hit.normal = float3(0.0, 1.0, 0.0);
                hit.height = met;
                hit.t = (met - origin.y) / direction.y;
                return hit;
            }
        }
        // The cut often lies along the cells' borders: a ray that reaches it has left the slab, not come into
        // the cell beyond.
        if (leave >= span.y - 1e-3)
        {
            break;
        }
        t = leave;
        atCut = false;
        if (next.x < next.y)
        {
            cell.x += stride.x;
            next.x += inverse.x;
            normal = float3(-float(stride.x), 0.0, 0.0);
        }
        else
        {
            cell.y += stride.y;
            next.y += inverse.y;
            normal = float3(0.0, 0.0, -float(stride.y));
        }
    }
    hit.kind = HIT_NONE;
    return hit;
}

// How much `column` stops of light climbing through it from height `low` to `high`: all of it where it passes
// through a piece, and more of it the further under the ground's top it comes in, so a step one block high casts
// a faint shadow and a cliff a full one.
float stops(MapColumn column, float low, float high, bool ground)
{
    if (column.top == NO_COLUMN)
    {
        return 0.0;
    }
    float2 first = pieceSpan(column, column.piece0);
    float2 second = pieceSpan(column, column.piece1);
    bool through = (first.y > first.x && low < first.y && high > first.x)
        || (second.y > second.x && low < second.y && high > second.x);
    float under = ground ? saturate((float(column.top - mapPush.view->base) - low) / 2.0) : 0.0;
    return through ? 1.0 : under;
}

// How much of the lamp's light reaches `from`: less where blocks stand between, within a few cells.
float lampReaches(MapView* view, float3 from)
{
    float3 direction = view->sun;
    if (direction.y <= 0.01)
    {
        return 1.0;
    }
    int2 cell = int2(floor(from.xz));
    int2 stride = int2(direction.x >= 0.0 ? 1 : -1, direction.z >= 0.0 ? 1 : -1);
    float2 inverse = 1.0 / max(abs(direction.xz), 1e-9);
    float2 border = float2(cell) + float2(stride.x > 0 ? 1.0 : 0.0, stride.y > 0 ? 1.0 : 0.0);
    float2 next = abs(border - from.xz) * inverse;
    // Its own column first: what stands over it there, a crown over its trunk's foot.
    float leave = min(next.x, next.y);
    float reaches = 1.0 - stops(columnAt(view, cell), from.y, from.y + direction.y * leave, false);
    for (uint walked = 0; walked < 24 && reaches > 0.02; walked++)
    {
        float enter = leave;
        if (next.x < next.y)
        {
            cell.x += stride.x;
            next.x += inverse.x;
        }
        else
        {
            cell.y += stride.y;
            next.y += inverse.y;
        }
        leave = min(next.x, next.y);
        float low = from.y + direction.y * enter;
        if (low >= view->ceiling)
        {
            break;
        }
        float stopped = stops(columnAt(view, cell), low, from.y + direction.y * leave, true);
        // Fainter the further off what stops it stands.
        reaches = min(reaches, 1.0 - stopped * (1.0 - enter / 40.0));
    }
    return reaches;
}

// A top's shade at `at` in its cell (0 to 1 each way), from the blocks standing over it round it: each corner
// darker for the two cells beside it and the one across, as blocks are shaded in the world.
float cornerShade(MapView* view, int2 cell, float2 at, float height, out float edge)
{
    float level = height + 0.5;
    bool west = solidAt(columnAt(view, cell + int2(-1, 0)), level);
    bool east = solidAt(columnAt(view, cell + int2(1, 0)), level);
    bool south = solidAt(columnAt(view, cell + int2(0, -1)), level);
    bool north = solidAt(columnAt(view, cell + int2(0, 1)), level);
    bool southWest = solidAt(columnAt(view, cell + int2(-1, -1)), level);
    bool southEast = solidAt(columnAt(view, cell + int2(1, -1)), level);
    bool northWest = solidAt(columnAt(view, cell + int2(-1, 1)), level);
    bool northEast = solidAt(columnAt(view, cell + int2(1, 1)), level);
    float4 corners = float4(
        (west && south) ? 0.0 : 3.0 - float(west) - float(south) - float(southWest),
        (east && south) ? 0.0 : 3.0 - float(east) - float(south) - float(southEast),
        (west && north) ? 0.0 : 3.0 - float(west) - float(north) - float(northWest),
        (east && north) ? 0.0 : 3.0 - float(east) - float(north) - float(northEast)) / 3.0;
    // How near the nearest edge the ground drops away at is, for its dark line.
    float below = height - 0.5;
    float4 drops = float4(
        solidAt(columnAt(view, cell + int2(-1, 0)), below) ? 1.0 : at.x,
        solidAt(columnAt(view, cell + int2(1, 0)), below) ? 1.0 : 1.0 - at.x,
        solidAt(columnAt(view, cell + int2(0, -1)), below) ? 1.0 : at.y,
        solidAt(columnAt(view, cell + int2(0, 1)), below) ? 1.0 : 1.0 - at.y);
    edge = min(min(drops.x, drops.y), min(drops.z, drops.w));
    return lerp(lerp(corners.x, corners.y, at.x), lerp(corners.z, corners.w, at.x), at.y);
}

// One ray's colour, premultiplied: nothing where it misses the slab.
float4 shade(MapView* view, float2 pixel)
{
    float2 ndc = float2(pixel.x / view->size.x * 2.0 - 1.0, 1.0 - pixel.y / view->size.y * 2.0);
    float3 direction = normalize(view->forward + view->right * ndc.x + view->up * ndc.y);
    MapHit hit = trace(view, view->origin, direction);
    if (hit.kind == HIT_NONE)
    {
        return float4(0.0);
    }
    float3 at = view->origin + direction * hit.t;
    // How many cells a pixel covers there: lines thinner than it are left out.
    float footprint = hit.t * length(view->up) * 2.0 / view->size.y;
    float lines = view->edges * saturate(1.25 - footprint * 5.0);
    float3 color = blockColor(view, hit.block);
    float light = 0.6 + 0.4 * saturate(dot(hit.normal, view->sun));
    float shadeBy = 1.0;

    if (hit.kind == HIT_TOP || hit.kind == HIT_WATER_TOP)
    {
        float2 within = at.xz - float2(hit.cell);
        float edge;
        float corners = cornerShade(view, hit.cell, saturate(within), hit.height, edge);
        shadeBy *= lerp(0.55, 1.0, corners);
        shadeBy *= 1.0 - lines * 0.35 * (1.0 - smoothstep(0.0, max(0.07, footprint * 1.5), edge));
        if (hit.kind == HIT_WATER_TOP)
        {
            // Shallow water shows the ground under it; deep water is its own colour.
            float depth = hit.height - hit.foot;
            MapColumn column = columnAt(view, hit.cell);
            // Seen through the water, the ground takes the water's hue.
            float3 hue = color / max(max(color.r, color.g), max(color.b, 1e-3));
            float3 under = blockColor(view, column.layers & 0xffff) * lerp(float3(1.0), hue, 0.75) * 0.85;
            color = lerp(under, color, saturate(0.3 + depth * 0.16));
            shadeBy = lerp(shadeBy, 1.0, 0.6);
        }
    }
    else
    {
        // A side: darker where it stands on the ground in front of it, and a faint line at each block up it.
        float over = hit.height - hit.foot;
        shadeBy *= lerp(0.62, 1.0, saturate(over));
        float up = frac(hit.height);
        float course = min(up, 1.0 - up);
        shadeBy *= 1.0 - lines * 0.2 * (1.0 - smoothstep(0.0, max(0.05, footprint * 1.5), course));
        if (hit.kind == HIT_WATER_SIDE)
        {
            color *= 0.8;
        }
    }
    if (view->shadows > 0.0)
    {
        float reaches = lampReaches(view, at + hit.normal * 0.02 + view->sun * 0.02);
        light *= lerp(1.0 - view->shadows, 1.0, reaches);
    }
    // News of the place colours it, whatever stands there.
    uint tint = columnAt(view, hit.cell).tint;
    if ((tint >> 24) != 0)
    {
        float3 towards = linearOf(float3(tint & 0xff, (tint >> 8) & 0xff, (tint >> 16) & 0xff) / 255.0);
        color = lerp(color, towards, float(tint >> 24) / 255.0);
    }
    // Higher ground a little lighter, so hills read from straight above.
    float high = saturate((hit.height - view->lowest) / max(view->ceiling - view->lowest, 1.0));
    color *= light * shadeBy * lerp(0.86, 1.1, high);
    return float4(color, 1.0);
}

[shader("vertex")]
float4 mapVertex(uint vertexId : SV_VertexID) : SV_Position
{
    float2 uv = float2((vertexId << 1) & 2, vertexId & 2);
    return float4(uv * 2.0 - 1.0, 0.0, 1.0);
}

[shader("fragment")]
float4 mapFragment(float4 position : SV_Position) : SV_Target
{
    MapView* view = mapPush.view;
    uint samples = clamp(view->samples, 1, 4);
    float2 corner = floor(position.xy);
    float4 sum = float4(0.0);
    for (uint y = 0; y < samples; y++)
    {
        for (uint x = 0; x < samples; x++)
        {
            // A grid a little turned, so near-level edges get as many steps as rays.
            float2 within = (float2(x, y) + 0.5 + float2(float(y), float(samples - 1 - x)) / float(samples) * 0.5 - 0.25)
                / float(samples);
            sum += shade(view, corner + saturate(within));
        }
    }
    return sum / float(samples * samples);
}
