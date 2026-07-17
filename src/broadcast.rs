use crate::{NodeId, Payload};
use crate::action::Action;
use crate::message::{Message, MessageId};

//forward to every peer except sender
//hop is incoming - sent are the hops plus 1
pub fn forward<Id: NodeId, P: Payload>(
      me: Id,
      active_peers: &[Id],
      exclude: Option<Id>,
      id: MessageId,
      hop: u32,
      payload: &P,
) -> Vec<Action<Id, P>> {
      let mut actions = Vec::new();
      for &peer in active_peers {
              if Some(peer) == exclude {
                      continue; // never echo back to the send
              }
              actions.push(Action::Send {
                      to: peer,
                      msg: Message::Broadcast {
                              id,
                              sender: me,
                              hop: hop + 1,
                              payload: payload.clone(),
                      },
              });
      }
      actions
}
