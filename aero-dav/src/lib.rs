#![feature(type_alias_impl_trait)]
#![feature(async_closure)]
#![feature(trait_alias)]

// utils
pub mod error;
pub mod xml;

// webdav
pub mod types;
pub mod encoder;
pub mod decoder;

// calendar
pub mod caltypes;
pub mod calencoder;
pub mod caldecoder;

// acl (wip)
pub mod acltypes;
pub mod aclencoder;
pub mod acldecoder;

// versioning (wip)
mod versioningtypes;

// final type
pub mod realization;
