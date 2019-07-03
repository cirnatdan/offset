use serde::{Deserialize, Serialize};

use crate::crypto::{HashResult, PublicKey, RandValue, Signature, Uid};

use crate::funder::messages::{FriendsRoute, Rate};
use crate::net::messages::NetAddress;

/// IndexClient -> IndexServer
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RequestRoutes {
    pub request_id: Uid,
    /// Wanted capacity for the route.
    /// 0 means we want to optimize for capacity??
    pub capacity: u128,
    pub source: PublicKey,
    pub destination: PublicKey,
    /// This directed edge must not show up any any route inside the multi-route.
    /// Useful for finding non trivial directed loops.
    pub opt_exclude: Option<(PublicKey, PublicKey)>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RouteCapacityRate {
    pub route: FriendsRoute,
    /// How many credits we can push along this route?
    pub capacity: u128,
    /// Combined rate of pushing credits along this route.
    pub rate: Rate,
}

/// Multiple routes that together allow to pass a certain amount of credits to a destination.
/// All routes must have the same beginning and the same end.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct MultiRoute {
    pub routes: Vec<RouteCapacityRate>,
}

/// IndexServer -> IndexClient
#[derive(Debug, Clone)]
pub struct ResponseRoutes {
    pub request_id: Uid,
    /// A few separate multi routes that allow to send the wanted amount of credits to the
    /// requested destination:
    pub multi_routes: Vec<MultiRoute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFriend {
    /// Friend's public key
    pub public_key: PublicKey,
    /// To denote remote requests closed, assign 0 to sendCapacity
    pub send_capacity: u128,
    /// To denote local requests closed, assign 0 to recvCapacity
    pub recv_capacity: u128,
    /// The rate we charge for forwarding messages to another friend from this friend.
    /// For example, in the following diagram we are X and A is the friend we are updating:
    /// A -- X -- B
    ///      \
    ///       --- C
    /// We can set how much we charge A for forwarding funds. The same rate applies either when A
    /// sends funds to B or to C.
    pub rate: Rate,
}

/// IndexClient -> IndexServer
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexMutation {
    UpdateFriend(UpdateFriend),
    RemoveFriend(PublicKey),
}

#[derive(Debug, Clone)]
pub struct MutationsUpdate {
    /// Public key of the node sending the mutations.
    pub node_public_key: PublicKey,
    /// List of mutations to relationships with direct friends.
    pub index_mutations: Vec<IndexMutation>,
    /// A time hash (Given by the server previously).
    /// This is used as time, proving that this message was signed recently.
    pub time_hash: HashResult,
    /// A randomly generated sessionId. The counter is related to this session Id.
    pub session_id: Uid,
    /// Incrementing counter, making sure that mutations are received in the correct order.
    /// For a new session, the counter should begin from 0 and increment by 1 for every MutationsUpdate message.
    /// When a new connection is established, a new sessionId should be randomly generated.
    pub counter: u64,
    /// Rand nonce, used as a security measure for the next signature.
    pub rand_nonce: RandValue,
    /// signature(sha_512_256("MUTATIONS_UPDATE") ||
    ///           nodePublicKey ||
    ///           mutation ||
    ///           timeHash ||
    ///           counter ||
    ///           randNonce)
    pub signature: Signature,
}

#[derive(Debug, Clone)]
pub struct TimeProofLink {
    /// List of hashes that produce a certain hash
    /// sha_512_256("HASH_CLOCK" || hashes)
    pub hashes: Vec<HashResult>,
}

#[derive(Debug, Clone)]
pub struct ForwardMutationsUpdate {
    pub mutations_update: MutationsUpdate,
    /// A proof that MutationsUpdate was signed recently
    /// Receiver should verify:
    /// - sha_512_256(hashes[0]) == MutationsUpdate.timeHash,
    /// - For all i < n - 1 : hashes[i][index[i]] == sha_512_256(hashes[i+1])
    /// - hashes[n-1][index[n-1]] is some recent time hash generated by the receiver.
    pub time_proof_chain: Vec<TimeProofLink>,
}

#[derive(Debug)]
pub enum IndexServerToClient {
    TimeHash(HashResult),
    ResponseRoutes(ResponseRoutes),
}

#[derive(Debug)]
pub enum IndexClientToServer {
    MutationsUpdate(MutationsUpdate),
    RequestRoutes(RequestRoutes),
}

#[derive(Debug)]
pub enum IndexServerToServer {
    TimeHash(HashResult),
    ForwardMutationsUpdate(ForwardMutationsUpdate),
}

// ----------------------------------------------
// ----------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedIndexServerAddress<ISA = NetAddress> {
    pub public_key: PublicKey,
    pub address: ISA,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexServerAddress<ISA = NetAddress> {
    pub public_key: PublicKey,
    pub address: ISA,
}

impl<ISA> From<NamedIndexServerAddress<ISA>> for IndexServerAddress<ISA> {
    fn from(from: NamedIndexServerAddress<ISA>) -> Self {
        IndexServerAddress {
            public_key: from.public_key,
            address: from.address,
        }
    }
}
