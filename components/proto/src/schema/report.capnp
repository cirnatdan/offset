@0x8bc829b5200f3c7f;

using import "common.capnp".PublicKey;
using import "common.capnp".Hash;
using import "common.capnp".CustomUInt128;
using import "common.capnp".CustomInt128;
using import "common.capnp".Signature;
using import "common.capnp".RandNonce;

using import "common.capnp".RelayAddress;
using import "common.capnp".IndexServerAddress;

## Report related structs
#########################

struct MoveTokenHashedReport {
        prefixHash @0: Hash;
        localPublicKey @1: PublicKey;
        remotePublicKey @2: PublicKey;
        inconsistencyCounter @3: UInt64;
        moveTokenCounter @4: CustomUInt128;
        balance @5: CustomInt128;
        localPendingDebt @6: CustomUInt128;
        remotePendingDebt @7: CustomUInt128;
        randNonce @8: RandNonce;
        newToken @9: Signature;
}


struct FriendStatusReport {
        union {
                disabled @0: Void;
                enabled @1: Void;
        }
}

struct RequestsStatusReport {
        union {
                closed @0: Void;
                open @1: Void;
        }
}

struct FriendLivenessReport {
        union {
                offline @0: Void;
                online @1: Void;
        }
}

struct DirectionReport {
        union {
                incoming @0: Void;
                outgoing @1: Void;
        }
}

struct McRequestsStatusReport {
        local @0: RequestsStatusReport;
        remote @1: RequestsStatusReport;
}

struct McBalanceReport {
    balance @0: CustomInt128;
    # Amount of credits this side has against the remote side.
    # The other side keeps the negation of this value.
    remoteMaxDebt @1: CustomUInt128;
    # Maximum possible remote debt
    localMaxDebt @2: CustomUInt128;
    # Maximum possible local debt
    localPendingDebt @3: CustomUInt128;
    # Frozen credits by our side
    remotePendingDebt @4: CustomUInt128;
    # Frozen credits by the remote side
}

struct TcReport {
        direction @0: DirectionReport;
        balance @1: McBalanceReport;
        requestsStatus @2: McRequestsStatusReport;
        numLocalPendingRequests @3: UInt64;
        numRemotePendingRequests @4: UInt64;
}

struct ChannelInconsistentReport {
        localResetTermsBalance @0: CustomInt128;
        optRocalResetTermsBalance: union {
                remoteResetTerms @1: CustomInt128;
                empty @2: Void;
        }
}


struct ChannelStatusReport {
        union {
                inconsistent @0: ChannelInconsistentReport;
                consistenet @1: TcReport;
        }
}

struct OptLastIncomingMoveToken {
        union {
                moveTokenHashed @0: MoveTokenHashedReport;
                empty @1: Void;
        }
}


struct FriendReport {
        address @0: RelayAddress;
        name @1: Text;
        optLastIncomingMoveToken @2: OptLastIncomingMoveToken;
        liveness @3: FriendLivenessReport;
        channelStatus @4: ChannelStatusReport;
        wantedRemoteMaxDebt @5: CustomUInt128;
        wantedLocalRequestsStatus @6: RequestsStatusReport;
        numPendingRequests @7: UInt64;
        numPendingResponses @8: UInt64;
        status @9: FriendStatusReport;
        numPendingUserRequests @10: UInt64;
}

# A full report. Contains a full summary of the current state.
# This will usually be sent only once, and then ReportMutations will be sent.
struct FunderReport {
        localPublicKey @0: PublicKey;
        optAddress: union {
                address @1: RelayAddress;
                empty @2: Void;
        }
        friends @3: List(FriendReport);
        numReadyReceipts @4: UInt64;
}


############################################################################
############################################################################


struct SetAddressReport {
    union {
        address @0: RelayAddress;
        empty @1: Void;
    }
}

struct AddFriendReport {
        friendPublicKey @0: PublicKey;
        address @1: RelayAddress;
        name @2: Text;
        balance @3: CustomInt128;
        optLastIncomingMoveToken @4: OptLastIncomingMoveToken;
        channelStatus @5: ChannelStatusReport;
}

struct RelayAddressName {
        address @0: RelayAddress;
        name @1: Text;
}

struct FriendReportMutation {
        union {
                setFriendInfo @0: RelayAddressName;
                setChannelStatus @1: ChannelStatusReport;
                setWantedRemoteMaxDebt @2: CustomUInt128;
                setWantedLocalRequestsStatus @3: RequestsStatusReport;
                setNumPendingRequests @4: UInt64;
                setNumPendingResponses @5: UInt64;
                setFriendStatus @6: FriendStatusReport;
                setNumPendingUserRequests @7: UInt64;
                setOptLastIncomingMoveToken @8: OptLastIncomingMoveToken;
                setLiveness @9: FriendLivenessReport;
        }
}

struct PkFriendReportMutation {
        friendPublicKey @0: PublicKey;
        friendReportMutation @1: FriendReportMutation;
}

# A FunderReportMutation. Could be applied over a FunderReport to make small changes.
struct FunderReportMutation {
        union {
                setAddress @0: SetAddressReport;
                addFriend @1: AddFriendReport;
                removeFriend @2: PublicKey;
                pkFriendReportMutation @3: PkFriendReportMutation;
                setNumReadyReceipts @4: UInt64;
        }
}


############################################################################
##### IndexClient report
############################################################################

struct IndexClientReport {
        indexServers @0: List(IndexServerAddress);
        optConnectedServer: union {
                indexServerAddress @1: IndexServerAddress;
                empty @2: Void;
        }
}

struct IndexClientReportMutation {
        union {
                addIndexServer @0: IndexServerAddress;
                removeIndexServer @1: IndexServerAddress;
                setConnectedServer: union {
                        indexServerAddress @2: IndexServerAddress;
                        empty @3: Void;
                }
        }
}


############################################################################
##### Node report
############################################################################

struct NodeReport {
        funderReport @0: FunderReport;
        indexClientReport @1: IndexClientReport;
}

struct NodeReportMutation {
        union {
                funder @0: FunderReportMutation;
                indexClient @1: IndexClientReportMutation;
        }
}
