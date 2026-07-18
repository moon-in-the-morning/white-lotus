use std::collections::HashMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use white_lotus::{Action, Config, Message, Node};

// What we gossip: a file-hash announcement, as a simple String.
type Announcement = String;

fn main() {
	// usage: node <my_id> <my_port> [<peer_id> <peer_host:port>]...
	//   two peers:  node 1 9001 2 10.7.23.32:9002 3 10.7.23.33:9003
	let args: Vec<String> = env::args().collect();
	let my_id: u32 = args[1].parse().unwrap();
	let my_port: u16 = args[2].parse().unwrap();

	// build the node and its address book
	let mut book: HashMap<u32, String> = HashMap::new();
	let mut node: Node<u32, Announcement> = Node::new(Config::new(my_id));

	// the remaining args are (peer_id  peer_host:port) pairs - one per peer
	let mut i = 3;
	while i + 1 < args.len() {
		let peer_id: u32 = args[i].parse().unwrap();
		let peer_addr: String = args[i + 1].clone();
		book.insert(peer_id, peer_addr);
		// put each peer into our active view so we forward to it
		let _ = node.handle(Message::Join { new_node: peer_id });
		i += 2;
	}

	// share both safely across the keyboard, ticker, and network threads
	let book = Arc::new(book);
	let node = Arc::new(Mutex::new(node));

	// --- keyboard thread: read a typed line, broadcast it ---
	{
		let node = Arc::clone(&node);
		let book = Arc::clone(&book);
		thread::spawn(move || {
			let stdin = std::io::stdin();
			for line in stdin.lock().lines() {
				let text = match line {
					Ok(t) => t,
					Err(_) => break,
				};
				if text.trim().is_empty() {
					continue;
				}
				let actions = node.lock().unwrap().broadcast(text);
				execute(&actions, &book);
			}
		});
	}

	// --- ticker thread: drive the Plumtree GRAFT timers ---
	{
		let node = Arc::clone(&node);
		let book = Arc::clone(&book);
		thread::spawn(move || {
			let start = std::time::Instant::now();
			loop {
				thread::sleep(std::time::Duration::from_millis(50));
				let now = start.elapsed().as_millis() as u64;
				let actions = node.lock().unwrap().tick(now);
				execute(&actions, &book);
			}
		});
	}

	// --- network thread (main): accept messages and handle them ---
	let listener = TcpListener::bind(format!("0.0.0.0:{my_port}")).unwrap();
	println!("[node {my_id}] listening on 0.0.0.0:{my_port}  (type a message + Enter to send)");
	for stream in listener.incoming() {
		let mut reader = BufReader::new(stream.unwrap());
		let mut line = String::new();
		if reader.read_line(&mut line).is_ok() && !line.trim().is_empty() {
			let msg: Message<u32, Announcement> = serde_json::from_str(line.trim()).unwrap();
			let actions = node.lock().unwrap().handle(msg);
			execute(&actions, &book);
		}
	}
}

// Carry out the Actions a node produced.
fn execute(actions: &[Action<u32, Announcement>], book: &HashMap<u32, String>) {
	for action in actions {
		match action {
			Action::Send { to, msg } => {
				if let Some(addr) = book.get(to) {
					if let Ok(mut stream) = TcpStream::connect(addr) {
						let line = serde_json::to_string(msg).unwrap();
						let _ = writeln!(stream, "{line}");
					}
				}
			}
			Action::Deliver { payload } => {
				println!(">>> DELIVERED announcement: {payload}");
			}
			Action::Connect { peer } => println!("[connect to node {peer}]"),
			Action::Disconnect { peer } => println!("[disconnect from node {peer}]"),
		}
	}
}
