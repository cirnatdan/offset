// use std::convert::TryFrom;
use std::collections::HashMap;

use futures::channel::mpsc;
use futures::StreamExt;

use crypto::hash_lock::HashLock;
use crypto::identity::{Identity, SoftwareEd25519Identity};
use crypto::rand::RandGen;
use crypto::test_utils::DummyRandom;

use proto::crypto::{HashResult, HmacResult, PlainLock, PrivateKey, PublicKey, Signature, Uid};
use proto::funder::messages::{
    CancelSendFundsOp, Currency, FriendTcOp, FriendsRoute, PendingTransaction, RequestSendFundsOp,
    ResponseSendFundsOp,
};
use signature::signature_buff::create_response_signature_buffer;

use crate::types::create_pending_transaction;

use crate::mutual_credit::incoming::{
    process_operation, ProcessOperationError, /*ProcessOperationOutput,*/
};
use crate::mutual_credit::outgoing::{OutgoingMc, QueueOperationError};
use crate::mutual_credit::types::{McBalance, McOp, McOpResult, McTransaction};

/*
/// Helper function for applying an outgoing operation over a token channel.
fn apply_outgoing(
    mc_transaction: &mut McTransaction,
    friend_tc_op: &FriendTcOp,
) -> Result<(), QueueOperationError> {
    let mut outgoing = OutgoingMc::new(mutual_credit);
    let mutations = outgoing.queue_operation(friend_tc_op)?;

    for mutation in mutations {
        mutual_credit.mutate(&mutation);
    }
    Ok(())
}

/// Helper function for applying an incoming operation over a token channel.
fn apply_incoming(
    mut mutual_credit: &mut MutualCredit,
    friend_tc_op: FriendTcOp,
    remote_max_debt: u128,
) -> Result<ProcessOperationOutput, ProcessOperationError> {
    process_operation(&mut mutual_credit, friend_tc_op, remote_max_debt)
}
*/

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct McPendingTransactions {
    /// Pending transactions that were opened locally and not yet completed
    pub local: HashMap<Uid, PendingTransaction>,
    /// Pending transactions that were opened remotely and not yet completed
    pub remote: HashMap<Uid, PendingTransaction>,
}

impl McPendingTransactions {
    fn new() -> McPendingTransactions {
        McPendingTransactions {
            local: HashMap::new(),
            remote: HashMap::new(),
        }
    }
}

#[derive(Arbitrary, Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MutualCredit {
    /// Currency in use (How much is one credit worth?)
    pub currency: Currency,
    /// Current credit balance with respect to remote side
    pub balance: McBalance,
    /// Requests in progress
    pub pending_transactions: McPendingTransactions,
}

impl MutualCredit {
    pub fn new(
        // TODO: Should we move instead of take a reference here?
        local_public_key: &PublicKey,
        remote_public_key: &PublicKey,
        currency: &Currency,
        balance: i128,
    ) -> MutualCredit {
        MutualCredit {
            currency: currency.clone(),
            balance: McBalance::new(balance),
            pending_transactions: McPendingTransactions::new(),
        }
    }

    fn set_balance(&mut self, balance: i128) {
        self.balance.balance = balance;
    }

    fn insert_remote_pending_transaction(&mut self, pending_transaction: PendingTransaction) {
        self.pending_transactions
            .remote
            .insert(pending_transaction.request_id.clone(), pending_transaction);
    }

    fn remove_remote_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self.pending_transactions.remote.remove(request_id);
    }

    fn insert_local_pending_transaction(&mut self, pending_transaction: PendingTransaction) {
        self.pending_transactions
            .local
            .insert(pending_transaction.request_id.clone(), pending_transaction);
    }

    fn remove_local_pending_transaction(&mut self, request_id: &Uid) {
        let _ = self.pending_transactions.local.remove(request_id);
    }

    fn set_remote_pending_debt(&mut self, remote_pending_debt: u128) {
        self.balance.remote_pending_debt = remote_pending_debt;
    }

    fn set_local_pending_debt(&mut self, local_pending_debt: u128) {
        self.balance.local_pending_debt = local_pending_debt;
    }
}

#[derive(Debug)]
pub enum McServerError {
    SendError,
}

pub async fn mc_server(
    mut mc: MutualCredit,
    mut incoming_ops: mpsc::Receiver<McOp>,
) -> Result<(), McServerError> {
    while let Some(mc_op) = incoming_ops.next().await {
        match mc_op {
            McOp::GetBalance(mc_balance_sender) => {
                mc_balance_sender
                    .send(Ok(mc.balance.clone()))
                    .map_err(|_| McServerError::SendError)?;
            }
            McOp::SetBalance(new_balance, sender) => {
                mc.set_balance(new_balance);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::SetLocalPendingDebt(new_pending_debt, sender) => {
                mc.set_local_pending_debt(new_pending_debt);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::SetRemotePendingDebt(new_pending_debt, sender) => {
                mc.set_remote_pending_debt(new_pending_debt);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::GetLocalPendingTransaction(request_id, pending_transaction_sender) => {
                let opt_pending_transaction =
                    mc.pending_transactions.local.get(&request_id).cloned();
                pending_transaction_sender
                    .send(Ok(opt_pending_transaction))
                    .map_err(|_| McServerError::SendError)?;
            }
            McOp::InsertLocalPendingTransaction(pending_transaction, sender) => {
                mc.insert_local_pending_transaction(pending_transaction);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::RemoveLocalPendingTransaction(request_id, sender) => {
                mc.remove_local_pending_transaction(&request_id);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::GetRemotePendingTransaction(request_id, pending_transaction_sender) => {
                let opt_pending_transaction =
                    mc.pending_transactions.remote.get(&request_id).cloned();
                pending_transaction_sender
                    .send(Ok(opt_pending_transaction))
                    .map_err(|_| McServerError::SendError)?;
            }
            McOp::InsertRemotePendingTransaction(pending_transaction, sender) => {
                mc.insert_remote_pending_transaction(pending_transaction);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
            McOp::RemoveRemotePendingTransaction(request_id, sender) => {
                mc.remove_remote_pending_transaction(&request_id);
                sender.send(Ok(())).map_err(|_| McServerError::SendError)?;
            }
        }
    }
    Ok(())
}
