// Erosion of a terrain's ground by water and by its own weight, on the GPU (src/erosion.rs), for an Erosion layer.
// Rain falls on every cell; water flows to lower neighbours through pipes between them, faster the further down
// they are (erosionFlux), and gathers in each cell (erosionWater). Water running fast picks up ground and water
// slowing drops what it carries (erosionCarve), which goes where its water goes, as large a share of it through
// each pipe (erosionCarry), so none is lost on the way; some water dries up each step, and more rain falls. Then ground steeper than loose ground can stand slumps down to its neighbours (erosionSlump,
// erosionSettle). Each pass reads what the last left and writes cells of its own, so no two threads write a place.

struct ErosionJob
{
    // The ground's heights, read by the first passes and written by the carving, and the copy carving writes,
    // which slumping reads and settles back into the first.
    float* ground;
    float* carved;
    float* water;
    // The water each cell held before this step's flow, which shares out what it carries.
    float* held;
    // Each cell's flow out to its left, right, near (-z) and far (+z) neighbours.
    float4* flux;
    float2* velocity;
    // What the water carries, and the copy carving writes, which drifting carries back.
    float* sediment;
    float* carried;
    // Each cell's ground slumping to each of its eight neighbours.
    float* slump;
    uint2 cells;
    float spacing;
    float dt;
    float rain;
    float capacity;
    float wear;
    float settle;
    float evaporation;
    // How far a cell may stand above a neighbour a metre away before it slumps, and how much of the rest goes.
    float talus;
    float slumpRate;
};

struct ErosionPush
{
    ErosionJob* job;
};

[[vk::push_constant]] ConstantBuffer<ErosionPush> push;

static const float GRAVITY = 9.81;

// The four neighbours water flows to: left, right, near, far.
static const int2 SIDES[4] = { int2(-1, 0), int2(1, 0), int2(0, -1), int2(0, 1) };

// The eight neighbours ground slumps to, each opposite the one at 7 less its place.
static const int2 AROUND[8] = {
    int2(-1, -1), int2(0, -1), int2(1, -1), int2(-1, 0), int2(1, 0), int2(-1, 1), int2(0, 1), int2(1, 1)
};

bool inside(ErosionJob* job, int2 cell)
{
    return all(cell >= 0) && all(cell < int2(job->cells));
}

uint at(ErosionJob* job, int2 cell)
{
    int2 held = clamp(cell, int2(0), int2(job->cells) - 1);
    return uint(held.y) * job->cells.x + uint(held.x);
}

// How fast water flows out of each cell to each side: as fast as it did, sped up by how much lower the water stands
// on that side, and never more than the cell holds. None past the ground's edge.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionFlux(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float here = job->ground[i] + job->water[i];
    float4 was = job->flux[i];
    float4 flow = float4(0.0);
    for (uint side = 0; side < 4; side++)
    {
        int2 next = cell + SIDES[side];
        if (!inside(job, next))
        {
            continue;
        }
        uint n = at(job, next);
        float drop = here - (job->ground[n] + job->water[n]);
        flow[side] = max(0.0, was[side] + job->dt * GRAVITY * drop * job->spacing);
    }
    float out = flow.x + flow.y + flow.z + flow.w;
    float held = job->water[i] * job->spacing * job->spacing;
    float share = out > 0.0 ? min(1.0, held / (out * job->dt)) : 0.0;
    job->flux[i] = flow * share;
}

// The water each cell holds after what flowed in and out, and how fast it moves through.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionWater(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float4 own = job->flux[i];
    float4 from = float4(0.0);
    for (uint side = 0; side < 4; side++)
    {
        int2 next = cell + SIDES[side];
        if (inside(job, next))
        {
            // Its flow towards this cell: the side opposite.
            from[side] = job->flux[at(job, next)][side ^ 1];
        }
    }
    float area = job->spacing * job->spacing;
    float was = job->water[i];
    job->held[i] = was;
    float now = max(0.0, was + job->dt * (dot(from, float4(1.0)) - dot(own, float4(1.0))) / area);
    // The flow through it along x and z, over the water's depth: its speed.
    float across = (from.x - own.x + own.y - from.y) * 0.5;
    float along = (from.z - own.z + own.w - from.w) * 0.5;
    float depth = max((was + now) * 0.5, 1e-4);
    float2 speed = float2(across, along) / (job->spacing * depth);
    // Never further in a step than a cell.
    float fastest = job->spacing / job->dt;
    float size = length(speed);
    if (size > fastest)
    {
        speed *= fastest / size;
    }
    job->velocity[i] = speed;
    job->water[i] = now;
}

// Water picks up ground where it runs faster down steeper ground than its load lets it, and drops what it carries
// where it slows.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionCarve(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float b = job->ground[i];
    float dx = (job->ground[at(job, cell + int2(1, 0))] - job->ground[at(job, cell - int2(1, 0))]) / (2.0 * job->spacing);
    float dz = (job->ground[at(job, cell + int2(0, 1))] - job->ground[at(job, cell - int2(0, 1))]) / (2.0 * job->spacing);
    float rise = sqrt(dx * dx + dz * dz);
    float tilt = max(rise / sqrt(1.0 + rise * rise), 0.05);
    // What it can carry: more the faster it runs, the deeper and the steeper.
    float depth = job->water[i];
    float load = job->capacity * tilt * length(job->velocity[i]) * depth;
    float s = job->sediment[i];
    if (s < load)
    {
        // At most a two-hundredth of a cell's width a step, so ground is never torn away faster than water runs off.
        float taken = min(job->wear * (load - s), job->spacing * 0.005);
        b -= taken;
        s += taken;
    }
    else
    {
        float dropped = job->settle * (s - load);
        b += dropped;
        s -= dropped;
    }
    job->carved[i] = b;
    job->carried[i] = s;
}

// The share of a cell's water that left it through each pipe this step: as large a share of what it carries goes
// the same way.
float4 leaving(ErosionJob* job, uint i)
{
    float volume = job->held[i] * job->spacing * job->spacing;
    if (volume <= 1e-9)
    {
        return float4(0.0);
    }
    float4 gone = job->flux[i] * job->dt / volume;
    float total = dot(gone, float4(1.0));
    return total > 1.0 ? gone / total : gone;
}

// What the water carries goes where its water went: each cell keeps what stayed and takes what its neighbours sent
// it. Then some of the water dries up, and rain falls.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionCarry(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float s = job->carried[i] * (1.0 - dot(leaving(job, i), float4(1.0)));
    for (uint side = 0; side < 4; side++)
    {
        int2 next = cell + SIDES[side];
        if (inside(job, next))
        {
            uint n = at(job, next);
            // Its share sent towards this cell: the side opposite.
            s += job->carried[n] * leaving(job, n)[side ^ 1];
        }
    }
    job->sediment[i] = s;
    job->water[i] = job->water[i] * (1.0 - job->evaporation) + job->rain;
}

// How much of each cell slumps to each neighbour it stands too far above: a share of how much further than loose
// ground can stand, split among them as each is too low.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionSlump(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float b = job->carved[i];
    float excess[8];
    float total = 0.0;
    float most = 0.0;
    for (uint k = 0; k < 8; k++)
    {
        int2 next = cell + AROUND[k];
        excess[k] = 0.0;
        if (!inside(job, next))
        {
            continue;
        }
        float away = (k == 1 || k == 3 || k == 4 || k == 6) ? job->spacing : job->spacing * 1.41421356;
        float over = b - job->carved[at(job, next)] - job->talus * away;
        if (over > 0.0)
        {
            excess[k] = over;
            total += over;
            most = max(most, over);
        }
    }
    float moved = job->slumpRate * most * 0.5;
    for (uint k = 0; k < 8; k++)
    {
        job->slump[i * 8 + k] = total > 0.0 ? moved * excess[k] / total : 0.0;
    }
}

// Each cell's ground after what slumped off it and onto it from its neighbours, back into the ground the next step
// reads.
[shader("compute")]
[numthreads(8, 8, 1)]
void erosionSettle(uint3 id : SV_DispatchThreadID)
{
    ErosionJob* job = push.job;
    int2 cell = int2(id.xy);
    if (!inside(job, cell))
    {
        return;
    }
    uint i = at(job, cell);
    float b = job->carved[i];
    for (uint k = 0; k < 8; k++)
    {
        b -= job->slump[i * 8 + k];
        int2 next = cell + AROUND[k];
        if (inside(job, next))
        {
            b += job->slump[at(job, next) * 8 + (7 - k)];
        }
    }
    job->ground[i] = b;
}
