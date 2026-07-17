// Integration test: exercises white-lotus the way a customer would, through
// its public API only (`use white_lotus::...`). This is the software stand-in
// for a fleet of Raspberry Pis - it spins up N nodes in one process, wires
// them into an overlay, broadcasts one message, and checks it reaches everyone.

use white_lotus::{Action, Config, Message, Node};
use std::collections::{HashMap, VecDeque};

// Build N nodes, wire them into an overlay, broadcast one message from node 0,
// and route every resulting Send until the network goes quiet.
// Returns a map of node id -> how many times it delivered the message.
fn run(n: u32, links: u32) -> HashMap<u32, u32> {
    // 1. Create N nodes (ids 0..n). fanout 4 => active view holds up to 5.
    let mut nodes: HashMap<u32, Node<u32>> = HashMap::new();
    for i in 0..n {
        let mut cfg = Config::new(i);
        cfg.fanout = 4; // active view capacity = fanout + 1 = 5
        cfg.max_rounds = n; // let the flood cross the whole overlay in this test
        nodes.insert(i, Node::new(cfg));
    }

    // 2. Wire the overlay: node i's active view = its next `links` peers
    //    (a ring with forward chords, guaranteed connected).
    for i in 0..n {
        for j in 1..=links {
            let peer = (i + j) % n;
            if peer != i {
                nodes
                    .get_mut(&i)
                    .unwrap()
                    .handle::<u64>(Message::Join { new_node: peer });
            }
        }
    }

    // 3. Node 0 originates one broadcast. Queue up whatever it sends.
    let mut queue: VecDeque<(u32, Message<u32, u64>)> = VecDeque::new();
    for action in nodes.get_mut(&0).unwrap().broadcast(777u64) {
        if let Action::Send { to, msg } = action {
            queue.push_back((to, msg));
        }
    }

    // 4. Run the network: pop a message, hand it to its target node, enqueue
    //    whatever that node forwards, record any delivery.
    let mut delivered: HashMap<u32, u32> = HashMap::new();
    while let Some((target, msg)) = queue.pop_front() {
        for action in nodes.get_mut(&target).unwrap().handle(msg) {
            match action {
                Action::Send { to, msg } => queue.push_back((to, msg)),
                Action::Deliver { .. } => {
                    *delivered.entry(target).or_insert(0) += 1;
                }
                _ => {} // Connect / Disconnect: real-network plumbing, ignore here
            }
        }
    }

    delivered
}

// A single message from node 0 should reach every OTHER node exactly once.
// (Node 0 is the source, so it never "delivers" to its own app.)

#[test]
fn reaches_all_5_nodes() {
    let delivered = run(5, 4);
    assert_eq!(delivered.len(), 4); // the 4 non-source nodes
    assert!(delivered.values().all(|&c| c == 1)); // each got it once
    assert!(!delivered.contains_key(&0)); // source didn't self-deliver
}

#[test]
fn reaches_all_30_nodes() {
    let delivered = run(30, 4);
    assert_eq!(delivered.len(), 29);
    assert!(delivered.values().all(|&c| c == 1));
}

#[test]
fn reaches_all_40_nodes() {
    let delivered = run(40, 4);
    assert_eq!(delivered.len(), 39);
    assert!(delivered.values().all(|&c| c == 1));
}

// Dedup guarantee: even though the ring has cycles (messages arrive from
// multiple directions), no node ever delivers the same message twice.
#[test]
fn dedup_prevents_double_delivery() {
    let delivered = run(30, 4);
    assert!(delivered.values().all(|&count| count == 1));
}
