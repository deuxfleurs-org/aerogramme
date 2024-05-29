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

// acl (partial)
pub mod acldecoder;
pub mod aclencoder;
pub mod acltypes;

// versioning (partial)
pub mod versioningdecoder;
pub mod versioningencoder;
pub mod versioningtypes;

// sync
pub mod syncdecoder;
pub mod syncencoder;
pub mod synctypes;

// final type
pub mod realization;
