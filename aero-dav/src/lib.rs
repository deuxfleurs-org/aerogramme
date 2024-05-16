#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(trait_alias)]

// utils
pub mod error;
pub mod xml;

// webdav
pub mod decoder;
pub mod encoder;
pub mod types;

// calendar
pub mod caldecoder;
pub mod calencoder;
pub mod caltypes;

// acl (wip)
pub mod acldecoder;
pub mod aclencoder;
pub mod acltypes;

// versioning (wip)
mod versioningtypes;

// final type
pub mod realization;
