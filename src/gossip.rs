use std::collections::{HashMap, HashSet};
use crate::{NodeId, Payload};
use crate::config::Config;
use crate::membership::Membership;
use crate::message::{Message, MessageId};
use crate::action::Action;
use crate::broadcast;

// The whole node: config + membership + Plumtree broadcast state, tied together.
// Generic over Id (who) and P (the payload we gossip). P is stored in the message
// cache so the node can answer a Plumtree GRAFT by re-sending the real payload.
pub struct Node<Id: NodeId, P: Payload> {
	config: Config<Id>,
	membership: Membership<Id>,
	// (origin, seq) ids we've already seen, so we never deliver or forward twice
	seen: HashSet<(Id, MessageId)>,
	// per-node counter for minting fresh broadcast sequence numbers
	next_seq: MessageId,
	// active peers we've demoted to LAZY (Plumtree PRUNE). Eager = active \ lazy.
	lazy: HashSet<Id>,
	// payloads we've delivered, kept so we can answer a GRAFT for them.
	// TODO: this grows unbounded - real Plumtree bounds it to a recent window.
	cache: HashMap<(Id, MessageId), P>,
}

impl<Id: NodeId, P: Payload> Node<Id, P> {
	// Build a node from its config, setting up an empty membership.
	pub fn new(config: Config<Id>) -> Self {
		let membership = Membership::new(
			config.me,
			config.fanout,
			config.passive_capacity,
			config.active_walk_length,
			config.passive_walk_length,
		);
		Node {
			config,
			membership,
			seen: HashSet::new(),
			next_seq: 0,
			lazy: HashSet::new(),
			cache: HashMap::new(),
		}
	}

	// Split the current active view into (eager, lazy) peer lists.
	fn eager_and_lazy(&self) -> (Vec<Id>, Vec<Id>) {
		let mut eager = Vec::new();
		let mut lazy = Vec::new();
		for &peer in self.membership.active_peers() {
			if self.lazy.contains(&peer) {
				lazy.push(peer);
			} else {
				eager.push(peer);
			}
		}
		(eager, lazy)
	}

	// Start a brand-new broadcast of `payload` from this node.
	pub fn broadcast(&mut self, payload: P) -> Vec<Action<Id, P>> {
		let seq = self.next_seq;
		self.next_seq += 1;
		let id = (self.config.me, seq);
		self.seen.insert(id);
		self.cache.insert(id, payload.clone());
		let (eager, lazy) = self.eager_and_lazy();
		let mut actions =
			broadcast::eager_push(self.config.me, &eager, None, self.config.me, seq, 0, &payload);
		actions.extend(broadcast::lazy_push::<Id, P>(
			self.config.me,
			&lazy,
			None,
			self.config.me,
			seq,
		));
		actions
	}

	// React to any incoming message.
	pub fn handle(&mut self, msg: Message<Id, P>) -> Vec<Action<Id, P>> {
		match msg {
			Message::Broadcast { origin, seq, sender, hop, payload } => {
				let id = (origin, seq);
				// A GOSSIP arriving from `sender` means that link is eager.
				self.lazy.remove(&sender);
				if !self.seen.insert(id) {
					// Duplicate: we already had it, so this eager link is redundant.
					// PRUNE the sender down to lazy.
					self.lazy.insert(sender);
					return vec![Action::Send {
						to: sender,
						msg: Message::Prune { sender: self.config.me },
					}];
				}
				self.cache.insert(id, payload.clone());
				let mut actions = vec![Action::Deliver { payload: payload.clone() }];
				// Stop forwarding once it has reached the hop limit.
				if hop >= self.config.max_rounds {
					return actions;
				}
				let (eager, lazy) = self.eager_and_lazy();
				actions.extend(broadcast::eager_push(
					self.config.me, &eager, Some(sender), origin, seq, hop, &payload,
				));
				actions.extend(broadcast::lazy_push::<Id, P>(
					self.config.me, &lazy, Some(sender), origin, seq,
				));
				actions
			}
			Message::IHave { origin, seq, sender } => {
				let id = (origin, seq);
				if self.seen.contains(&id) {
					// Already have it - the lazy announcement is redundant.
					Vec::new()
				} else {
					// Missing it: GRAFT the sender (promote link to eager) and ask
					// for the payload.
					// TODO: real Plumtree waits a short timer before grafting, to
					// give the eager tree a chance; we graft immediately.
					self.lazy.remove(&sender);
					vec![Action::Send {
						to: sender,
						msg: Message::Graft { origin, seq, sender: self.config.me },
					}]
				}
			}
			Message::Graft { origin, seq, sender } => {
				let id = (origin, seq);
				// Promote this link back to eager.
				self.lazy.remove(&sender);
				// Answer with the cached payload, if we still have it.
				match self.cache.get(&id) {
					Some(payload) => vec![Action::Send {
						to: sender,
						msg: Message::Broadcast {
							origin,
							seq,
							sender: self.config.me,
							hop: 0,
							payload: payload.clone(),
						},
					}],
					None => Vec::new(),
				}
			}
			Message::Prune { sender } => {
				// The peer says our eager link to them is redundant: demote to lazy.
				self.lazy.insert(sender);
				Vec::new()
			}
			// Everything else is membership control - hand it to that layer.
			other => self.membership.handle(other),
		}
	}
}
