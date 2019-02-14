#![feature(futures_api, async_await, await_macro, arbitrary_self_types)]
#![feature(nll)]
#![feature(try_from)]
#![feature(generators)]
#![feature(never_type)]

#![deny(
    trivial_numeric_casts,
    warnings
)]

// #[macro_use]
// extern crate log;

mod connector;
mod identity;
mod interface;

pub use self::interface::{ConnectError, connect};
pub use self::identity::{identity_from_file, IdentityFromFileError};
pub use self::connector::NodeConnection;

pub use proto::app_server::messages::{AppToAppServer, AppServerToApp, AppPermissions};
