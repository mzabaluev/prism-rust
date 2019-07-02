use crate::block::{Block, Content};
use crate::crypto::hash::{Hashable, H256};

use std::convert::From;
use std::thread;
use log::warn;

use std::sync::mpsc;
use websocket::client::ClientBuilder;
use websocket::message::OwnedMessage;

#[derive(Serialize)]
struct ProposerBlock {
    /// Hash of this block
    id: String,
    /// Proposer parent
    parent: String,
    /// Transaction refs
    transaction_refs: Vec<String>,
    /// Proposer refs
    proposer_refs: Vec<String>,
}
#[derive(Serialize)]
struct VoterBlock {
    /// Hash of this block
    id: String,
    /// Proposer parent
    parent: String,
    /// Voting chain number
    chain: u16,
    /// Voter parent
    voter_parent: String,
    /// Votes
    votes: Vec<String>,
}
#[derive(Serialize)]
struct TransactionBlock {
    /// Hash of this block
    id: String,
    /// Proposer parent
    parent: String,
}
#[derive(Serialize)]
struct UpdatedLedger {
    /// Hash of proposer blocks that are added to ledger 
    added: Vec<String>,
    /// Hash of proposer blocks that are removed from ledger 
    removed: Vec<String>,
}
#[derive(Serialize)]
enum DemoMsg {
    ProposerBlock(ProposerBlock),
    VoterBlock(VoterBlock),
    TransactionBlock(TransactionBlock),
    UpdatedLedger(UpdatedLedger),
}

impl From<&Block> for DemoMsg {
    fn from(block: &Block) -> Self {
        let hash = block.hash();
        let parent = block.header.parent;
        match &block.content {
            Content::Proposer(content) => {
                let b = ProposerBlock { id: hash.to_string(), parent: parent.to_string(), transaction_refs: content.transaction_refs.iter().map(|x|x.to_string()).collect(), proposer_refs: content.proposer_refs.iter().map(|x|x.to_string()).collect()};
                DemoMsg::ProposerBlock(b)
            }
            Content::Voter(content) => {
                let b = VoterBlock { id: hash.to_string(), parent: parent.to_string(), chain: content.chain_number, voter_parent: content.voter_parent.to_string(), votes: content.votes.iter().map(|x|x.to_string()).collect()};
                DemoMsg::VoterBlock(b)
            }
            Content::Transaction(_) => {
                let b = TransactionBlock { id: hash.to_string(), parent: parent.to_string() };
                DemoMsg::TransactionBlock(b)
            }
        }
    }
}

pub fn new(url: &str) -> mpsc::Sender<String> {
    let (sender, receiver) = mpsc::channel();
    let client_builder = ClientBuilder::new(url);
    if let Ok(client_builder) = client_builder {
        let client = client_builder
            .add_protocol("rust-websocket")
            .connect_insecure();
        if let Ok(mut client) = client {
            thread::spawn(move|| {
                for msg in receiver.iter() {
                    if client.send_message(&OwnedMessage::Text(msg)).is_err() {break;}
                }
            });
        } else {
            warn!("Fail to connect to demo websocket {}.", url);
        }
    } else {
        warn!("Fail to connect to demo websocket {}.", url);
    }
    sender
}

pub fn insert_block_msg(block: &Block) -> String {
    let msg: DemoMsg = block.into();
    let json: String = serde_json::to_string_pretty(&msg).unwrap();
    json
}

pub fn update_ledger_msg(added: &[H256], removed: &[H256]) -> String {
    if added.is_empty() && removed.is_empty() {
        return String::from("");
    }
    let added = added.iter().map(|x|x.to_string()).collect();
    let removed = removed.iter().map(|x|x.to_string()).collect();
    let msg: DemoMsg = DemoMsg::UpdatedLedger(UpdatedLedger{added, removed});
    let json: String = serde_json::to_string_pretty(&msg).unwrap();
    json
}

