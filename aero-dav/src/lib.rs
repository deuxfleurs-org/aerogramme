// utils
pub mod error;
pub mod xml;
pub mod extension;

// webdav core
pub mod coredecoder;
pub mod coreencoder;
pub mod coretypes;

// calendar (CardDAV)
pub mod caldecoder;
pub mod calencoder;
pub mod caltypes;

// contacts (CalDAV)
pub mod carddecoder;
pub mod cardencoder;
pub mod cardtypes;

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
