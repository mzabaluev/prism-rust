use crate::crypto::hash::H256;
use std::sync::{Arc, Weak};
use std::sync::Mutex;
use std::convert::TryFrom;
use crate::chain::*;
use std::collections::{HashMap, HashSet};
use std::iter::{IntoIterator, FromIterator};
use statrs::distribution::{Discrete, Poisson, Univariate};

pub struct LedgerIndex {
    voter_tips: Vec<Arc<Voter>>,
    proposer_tip: Arc<Proposer>,
    unconfirmed_proposer: HashSet<H256>,
    leader_sequence: Vec<Option<H256>>,
    ledger_order: Vec<Option<Vec<H256>>>,
}

impl LedgerIndex {
    // TODO: for now, we only have the ability to start from scratch
    pub fn new<'a, T>(proposer_tip: &Arc<Proposer>, voter_tips: &[Arc<Voter>], unconfirmed: T,
                      leader_sequence: &[Option<H256>], ledger_order: &[Option<Vec<H256>>]) -> Self
    where T: IntoIterator<Item = &'a H256>,
    {
        Self {
            voter_tips: voter_tips.to_vec(),
            proposer_tip: Arc::clone(&proposer_tip),
            unconfirmed_proposer: HashSet::from_iter(unconfirmed.into_iter().copied()),
            leader_sequence: leader_sequence.to_vec(),
            ledger_order: ledger_order.to_vec(),
        }
    }

    pub fn insert_unconfirmed(&mut self, hash: H256) {
        self.unconfirmed_proposer.insert(hash);
    }

    // returns added transaction blocks, removed transaction blocks
    //pub fn advance_ledger_to(&mut self, new_voter_tips: &[Voter]) -> (Vec<H256>, Vec<H256>) {}
    //}
    
    fn proposer_leader(&self, voter_tips: &[Voter], level: u64, quantile: f32, adversary_ratio: f32) -> Option<H256> {
        // compute the new leader of this level
        // we use the confirmation policy from https://arxiv.org/abs/1810.08092
        let mut new_leader: Option<H256> = None;

        // collect the depth of each vote on each proposer block
        let mut votes_depth: HashMap<H256, Vec<u64>> = HashMap::new(); // chain number and vote depth cast on the proposer block

        // collect the total votes on all proposer blocks of the level, and the number of
        // voter blocks mined on the main chain after those votes are casted
        let mut total_vote_count: u16 = 0;
        let mut total_vote_blocks: u64 = 0;

        // get the vote from each voter chain
        for voter in voter_tips.iter() {
            let vote = voter.proposer_vote_of_level(level);
            // if this chain voted
            if let Some((hash, depth)) = vote {
                if let Some(l) = votes_depth.get_mut(&hash) {
                    l.push(depth);
                } else {
                    votes_depth.insert(hash, vec![depth]);
                }
                total_vote_count += 1;
                // count the number of blocks on main chain starting at the vote
                total_vote_blocks += depth;
            }
        }
        let proposer_blocks: Vec<H256> = votes_depth.keys().copied().collect();
        let num_voter_chains = u16::try_from(voter_tips.len()).unwrap();

        // no point in going further if less than 3/5 votes are cast
        if total_vote_count > num_voter_chains * 3 / 5 {
            // calculate the average number of voter blocks mined after
            // a vote is casted. we use this as an estimator of honest mining
            // rate, and then derive the believed malicious mining rate
            let avg_vote_blocks = total_vote_blocks as f32 / f32::from(total_vote_count);
            // expected voter depth of an adversary
            let adversary_expected_vote_depth =
                avg_vote_blocks / (1.0 - adversary_ratio) * adversary_ratio;
            let poisson = Poisson::new(f64::from(adversary_expected_vote_depth)).unwrap();

            // for each block calculate the lower bound on the number of votes
            let mut votes_lcb: HashMap<&H256, f32> = HashMap::new();
            let mut total_votes_lcb: f32 = 0.0;
            let mut max_vote_lcb: f32 = 0.0;

            for block in &proposer_blocks {
                let votes = votes_depth.get(block).unwrap();

                let mut block_votes_mean: f32 = 0.0; // mean E[X]
                let mut block_votes_variance: f32 = 0.0; // Var[X]
                let mut block_votes_lcb: f32 = 0.0;
                for depth in votes.iter() {
                    // probability that the adversary will remove this vote
                    let mut p: f32 = 1.0 - poisson.cdf((*depth as f32 + 1.0).into()) as f32;
                    for k in 0..(*depth as u64) {
                        // probability that the adversary has mined k blocks
                        let p1 = poisson.pmf(k) as f32;
                        // probability that the adversary will overtake 'depth-k' blocks
                        let p2 = (adversary_ratio
                            / (1.0 - adversary_ratio))
                            .powi((depth - k + 1) as i32);
                        p += p1 * p2;
                    }
                    block_votes_mean += 1.0 - p;
                    block_votes_variance += p * (1.0 - p);
                }
                // using gaussian approximation
                let tmp = block_votes_mean - (block_votes_variance).sqrt() * quantile;
                if tmp > 0.0 {
                    block_votes_lcb += tmp;
                }
                votes_lcb.insert(block, block_votes_lcb);
                total_votes_lcb += block_votes_lcb;

                if max_vote_lcb < block_votes_lcb {
                    max_vote_lcb = block_votes_lcb;
                    new_leader = Some(*block);
                }
                // In case of a tie, choose block with lower hash.
                if (max_vote_lcb - block_votes_lcb).abs() < std::f32::EPSILON
                    && new_leader.is_some()
                {
                    // TODO: is_some required?
                    if *block < new_leader.unwrap() {
                        new_leader = Some(*block);
                    }
                }
            }
            // check if the lcb_vote of new_leader is bigger than second best ucb votes
            let remaining_votes = f32::from(num_voter_chains) - total_votes_lcb;

            // if max_vote_lcb is lesser than the remaining_votes, then a private block could
            // get the remaining votes and become the leader block
            if max_vote_lcb <= remaining_votes || new_leader.is_none() {
                new_leader = None;
            } else {
                for p_block in &proposer_blocks {
                    // if the below condition is true, then final votes on p_block could overtake new_leader
                    if max_vote_lcb < votes_lcb.get(p_block).unwrap() + remaining_votes
                        && *p_block != new_leader.unwrap()
                    {
                        new_leader = None;
                        break;
                    }
                    //In case of a tie, choose block with lower hash.
                    if (max_vote_lcb - (votes_lcb.get(p_block).unwrap() + remaining_votes)).abs()
                        < std::f32::EPSILON
                        && *p_block < new_leader.unwrap()
                    {
                        new_leader = None;
                        break;
                    }
                }
            }
        }
        new_leader
    }
}
