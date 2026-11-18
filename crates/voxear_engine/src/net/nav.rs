//! Nav Agents directed from players' machines: a client's script sending a
//! mob the server moves somewhere, or stopping it, is sent to the server,
//! which finds the way. Only the machine with authority moves an agent
//! (see `nav_agent`), so a destination set where it has none would
//! otherwise go nowhere.

use voxear_core::math::Vec3;
use voxear_ecs::{Entity, World};
use voxear_net::Channel;
use voxear_scene::DimensionId;

use super::{End, NetSession, PlayerId, messages::Message, replication::Networked};

/// A player asking the server to send one of its Nav Agents somewhere.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NavAsked {
    pub entity: Entity,
    /// Where to, in which dimension; `None` to stand.
    pub to: Option<(Vec3, DimensionId)>,
    pub stopped: bool,
    pub player: PlayerId,
}

/// Sends a Nav Agent's destination to the server, from a client whose
/// script set it on an Obj it does not move: where to, in `dimension`, and
/// whether it stands. Gives back whether it went.
pub fn ask_nav(world: &mut World, entity: Entity, to: Option<(Vec3, DimensionId)>, stopped: bool) -> bool {
    let Some(id) = world.get::<Networked>(entity).map(|n| n.id).filter(|id| id.0 != 0) else { return false };
    let Some(session) = world.resource_mut::<NetSession>() else { return false };
    let to = match to {
        Some((point, dimension)) => match session.dims.number_of(dimension) {
            Some(number) => Some((point, number)),
            None => return false,
        },
        None => None,
    };
    let origin = session.origin;
    let End::Client(client) = &mut session.end else { return false };
    client.send(Channel::RELIABLE, &Message::NavGo { id, to, stopped }.encode(origin)).is_ok()
}

/// Takes in, on the server, a player's word on one of its Nav Agents: only
/// for an Obj no player owns, or theirs.
pub(super) fn heard(session: &mut NetSession, player: PlayerId, message: Message) {
    let Message::NavGo { id, to, stopped } = message else { return };
    let Some(tracked) = session.replication.objects.get(&id) else { return };
    if tracked.owner.is_some_and(|owner| owner != player) {
        return;
    }
    let entity = tracked.entity;
    let to = match to {
        Some((point, number)) => {
            let Some(viewer) = session.replication.viewers.get(&player) else { return };
            let Some(dimension) = viewer.dimensions.dimension(number) else { return };
            Some((point, dimension))
        }
        None => None,
    };
    session.nav_asked.push(NavAsked { entity, to, stopped, player });
}

/// What players asked of the server's Nav Agents since last taken.
pub fn take_nav_asked(world: &mut World) -> Vec<NavAsked> {
    world.resource_mut::<NetSession>().map(|session| std::mem::take(&mut session.nav_asked)).unwrap_or_default()
}
