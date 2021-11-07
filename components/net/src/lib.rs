#![crate_type = "lib"]
#![deny(trivial_numeric_casts, warnings)]
#![allow(broken_intra_doc_links)]
#![allow(
    clippy::too_many_arguments,
    clippy::implicit_hasher,
    clippy::module_inception,
    clippy::new_without_default
)]

#[macro_use]
extern crate log;

mod tcp_connector;
mod tcp_listener;
#[cfg(test)]
mod tests;
mod types;
mod utils;

pub use self::tcp_connector::TcpConnector;
pub use self::tcp_listener::TcpListener;
