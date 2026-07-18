// Focused tests for the Plumtree layer: eager/lazy push, PRUNE (trim redundant
// tree links) and GRAFT (heal gaps). Each test hands the node one message and
// checks the Actions it returns - deterministic, no peeking at private state.

use white_lotus::{Action, Config, Message, Node};

// Receiving the SAME broadcast twice => the second (redundant) delivery makes
// the node PRUNE that link back to lazy.
#[test]
fn duplicate_broadcast_triggers_prune() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1));
	let msg = || Message::Broadcast { origin: 9, seq: 0, sender: 2, hop: 0, payload: 42u64 };

	let _first = node.handle(msg()); // first time: delivered
	let second = node.handle(msg()); // duplicate: should prune the sender

	assert!(second.iter().any(|a|
		matches!(a, Action::Send { to, msg: Message::Prune { .. } } if *to == 2)));
}

// An IHave alone does NOT graft immediately - it starts a grace-period timer,
// giving the eager tree a chance to deliver the payload first.
#[test]
fn ihave_alone_does_not_graft() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1));
	let actions = node.handle(Message::IHave { origin: 9, seq: 0, sender: 2 });
	assert!(actions.is_empty());
}

// If the eager payload never arrives, ticking past the deadline fires a GRAFT.
#[test]
fn graft_fires_after_the_timer_expires() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1)); // graft_timeout = 100
	node.handle(Message::IHave { origin: 9, seq: 0, sender: 2 });

	// before the deadline: nothing
	assert!(node.tick(50).is_empty());
	// after the deadline: GRAFT the announcer
	let actions = node.tick(150);
	assert!(actions.iter().any(|a|
		matches!(a, Action::Send { to, msg: Message::Graft { .. } } if *to == 2)));
}

// If the eager payload arrives before the timer, the pending GRAFT is cancelled.
#[test]
fn payload_before_deadline_cancels_the_graft() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1));
	node.handle(Message::IHave { origin: 9, seq: 0, sender: 2 });
	// the real payload shows up via the eager tree
	node.handle(Message::Broadcast { origin: 9, seq: 0, sender: 3, hop: 0, payload: 42u64 });
	// ticking past the old deadline now produces no GRAFT
	assert!(node.tick(150).is_empty());
}

// An IHave for a message we ALREADY have => ignore it (no graft, no traffic).
#[test]
fn ihave_for_seen_is_ignored() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1));
	// deliver (9,0) first so the node has already seen it
	node.handle(Message::Broadcast { origin: 9, seq: 0, sender: 2, hop: 0, payload: 42u64 });

	let actions = node.handle(Message::IHave { origin: 9, seq: 0, sender: 3 });

	assert!(actions.is_empty());
}

// A GRAFT for a message we cached => answer with the full payload.
#[test]
fn graft_returns_the_cached_payload() {
	let mut node: Node<u32, u64> = Node::new(Config::new(1));
	// deliver (9,0) with payload 42 so it lands in the cache
	node.handle(Message::Broadcast { origin: 9, seq: 0, sender: 2, hop: 0, payload: 42u64 });

	let actions = node.handle(Message::Graft { origin: 9, seq: 0, sender: 3 });

	assert!(actions.iter().any(|a| matches!(
		a,
		Action::Send { to, msg: Message::Broadcast { payload, .. } } if *to == 3 && *payload == 42
	)));
}
