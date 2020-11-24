use byteorder::{BigEndian, WriteBytesExt};

use crypto::hash;

use common::int_convert::usize_to_u64;

use proto::crypto::{HashResult, PublicKey};
use proto::funder::messages::{
    Currency, MoveToken, PendingTransaction, TokenInfo, UnsignedResponseSendFundsOp,
};
use proto::index_server::messages::MutationsUpdate;

use crate::canonical::CanonicalSerialize;

pub const FUNDS_RESPONSE_PREFIX: &[u8] = b"FUND_RESPONSE";
pub const FUNDS_CANCEL_PREFIX: &[u8] = b"FUND_CANCEL";

// TODO:
// - We probably don't need the "unsigned" version,
// - Take response_send_funds as a reference.
/// Create the buffer we sign over at the Response funds.
/// Note that the signature is not just over the Response funds bytes. The signed buffer also
/// contains information from the Request funds.
pub fn create_response_signature_buffer<RSF>(
    currency: &Currency,
    response_send_funds: RSF,
    pending_transaction: &PendingTransaction,
) -> Vec<u8>
where
    RSF: Into<UnsignedResponseSendFundsOp>,
{
    /*
        # Signature{key=destinationKey}(
        #   sha512/256("FUNDS_RESPONSE") ||
        #   sha512/256(requestId || hmac || srcPlainLock || destPayment)
        #   serialNum ||
        #   totalDestPayment ||
        #   invoiceHash ||
        #   currency [Implicitly known by the mutual credit]
        # )
    */

    let response_send_funds: UnsignedResponseSendFundsOp = response_send_funds.into();
    let mut sbuffer = Vec::new();

    sbuffer.extend_from_slice(&hash::hash_buffer(FUNDS_RESPONSE_PREFIX));

    let mut inner_blob = Vec::new();
    inner_blob.extend_from_slice(&pending_transaction.request_id);
    inner_blob.extend_from_slice(&pending_transaction.hmac);
    inner_blob.extend_from_slice(&response_send_funds.src_plain_lock);
    inner_blob
        .write_u128::<BigEndian>(pending_transaction.dest_payment)
        .unwrap();

    sbuffer.extend_from_slice(&hash::hash_buffer(&inner_blob));
    sbuffer
        .write_u128::<BigEndian>(response_send_funds.serial_num)
        .unwrap();
    sbuffer
        .write_u128::<BigEndian>(pending_transaction.total_dest_payment)
        .unwrap();
    sbuffer.extend_from_slice(&pending_transaction.invoice_hash);
    sbuffer.extend_from_slice(&currency.canonical_serialize());

    sbuffer
}

// Prefix used for chain hashing of token channel funds.
// NEXT is used for hashing for the next move token funds.
pub const TOKEN_NEXT: &[u8] = b"NEXT";

/*
/// Combine all operations into one hash value.
pub fn operations_hash<B, MT>(move_token: MT) -> HashResult
where
    MT: Into<UnsignedMoveToken<B>>,
{
    let move_token: UnsignedMoveToken<B> = move_token.into();
    let operations_data = move_token.currencies_operations.canonical_serialize();
    sha_512_256(&operations_data)
}
*/

/*
pub fn local_address_hash<B, MT>(move_token: MT) -> HashResult
where
    B: CanonicalSerialize + Clone,
    MT: Into<UnsignedMoveToken<B>>,
{
    let move_token: UnsignedMoveToken<B> = move_token.into();
    sha_512_256(&move_token.opt_local_relays.canonical_serialize())
}
*/

pub fn hash_token_info(
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    token_info: &TokenInfo,
) -> HashResult {
    let mut move_token_counter_buff = Vec::new();
    move_token_counter_buff
        .write_u128::<BigEndian>(token_info.move_token_counter)
        .unwrap();

    hash::Hasher::new()
        .chain(&local_public_key)
        .chain(&remote_public_key)
        .chain(&token_info.balances_hash)
        .chain(&move_token_counter_buff)
        .finalize()
}

/*
/// Hash operations and local_address:
pub fn prefix_hash<B, MT>(move_token: MT) -> HashResult
where
    B: CanonicalSerialize + Clone,
    MT: Into<UnsignedMoveToken<B>>,
{
    let move_token: UnsignedMoveToken<B> = move_token.into();
    let mut hash_buff = Vec::new();

    hash_buff.extend_from_slice(&move_token.old_token);
    // hash_buff.extend_from_slice(&move_token.currencies_operations.canonical_serialize());
    // hash_buff.extend_from_slice(&move_token.opt_local_relays.canonical_serialize());
    //hash_buff.extend_from_slice(&move_token.opt_active_currencies.canonical_serialize());

    sha_512_256(&hash_buff)
}
*/

pub fn move_token_signature_buff(move_token: &MoveToken) -> Vec<u8> {
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&hash::hash_buffer(TOKEN_NEXT));
    sig_buffer.extend_from_slice(&move_token.old_token);
    sig_buffer.extend_from_slice(&move_token.info_hash);
    sig_buffer
}

// TODO: Rethink string in this prefix:
pub const RESET_TOKEN_PREFIX: &[u8] = b"RESET_TOKEN";

pub fn reset_token_signature_buff(
    local_public_key: &PublicKey,
    remote_public_key: &PublicKey,
    move_token_counter: u128,
) -> Vec<u8> {
    let mut sig_buffer = Vec::new();
    sig_buffer.extend_from_slice(&hash::hash_buffer(RESET_TOKEN_PREFIX));
    sig_buffer.extend_from_slice(&local_public_key);
    sig_buffer.extend_from_slice(&remote_public_key);
    sig_buffer
        .write_u128::<BigEndian>(move_token_counter)
        .unwrap();
    sig_buffer
}

pub const MUTATIONS_UPDATE_PREFIX: &[u8] = b"MUTATIONS_UPDATE";

pub fn create_mutations_update_signature_buff(mutations_update: &MutationsUpdate) -> Vec<u8> {
    let mut res_bytes = Vec::new();
    res_bytes.extend_from_slice(&hash::hash_buffer(MUTATIONS_UPDATE_PREFIX));
    res_bytes.extend_from_slice(&mutations_update.node_public_key);

    res_bytes
        .write_u64::<BigEndian>(usize_to_u64(mutations_update.index_mutations.len()).unwrap())
        .unwrap();
    for mutation in &mutations_update.index_mutations {
        res_bytes.extend(mutation.canonical_serialize());
    }

    res_bytes.extend_from_slice(&mutations_update.time_hash);
    res_bytes.extend_from_slice(&mutations_update.session_id);
    res_bytes
        .write_u64::<BigEndian>(mutations_update.counter)
        .unwrap();
    res_bytes.extend_from_slice(&mutations_update.rand_nonce);

    res_bytes
}
