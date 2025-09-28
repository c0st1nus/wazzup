pub mod connector;
pub mod models;

// Re-export the primary DB types and connect helper for convenient access as `database::connect()`
#[allow(unused_imports)]
pub use connector::{DB, connect, connect_from_url, connect_with_settings, ping};
