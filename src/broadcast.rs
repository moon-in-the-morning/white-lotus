use crate::{NodeId, Payload};
use crate::action::Action;
use crate::message::{Message, MessageId};

// Eager push: send the FULL payload to every eager peer except the sender.
// hop is the incoming hop count; forwarded copies carry hop + 1.
// origin + seq travel unchanged so the (origin, seq) id stays globally unique.
pub fn eager_push<Id: NodeId, P: Payload>(
	me: Id,
	eager_peers: &[Id],
	exclude: Option<Id>,
	origin: Id,
	seq: MessageId,
	hop: u32,
	payload: &P,
) -> Vec<Action<Id, P>> {
	let mut actions = Vec::new();
	for &peer in eager_peers {
		if Some(peer) == exclude {
			continue; // never echo back to the sender
		}
		actions.push(Action::Send {
			to: peer,
			msg: Message::Broadcast {
				origin,
				seq,
				sender: me,
				hop: hop + 1,
				payload: payload.clone(),
			},
		});
	}
	actions
}

// Lazy push: send just an IHave(id) announcement to every lazy peer except the
// sender - the cheap "I have this, ask me if you want it" side of Plumtree.
pub fn lazy_push<Id: NodeId, P: Payload>(
	me: Id,
	lazy_peers: &[Id],
	exclude: Option<Id>,
	origin: Id,
	seq: MessageId,
) -> Vec<Action<Id, P>> {
	let mut actions = Vec::new();
	for &peer in lazy_peers {
		if Some(peer) == exclude {
			continue;
		}
		actions.push(Action::Send {
			to: peer,
			msg: Message::IHave {
				origin,
				seq,
				sender: me,
			},
		});
	}
	actions
}
