pub mod config;
pub mod cryptoblob;
pub mod login;
pub mod storage;

// A user is composed of 3 things:
// - An identity (login)
// - A storage profile (storage)
// - Some cryptography data (cryptoblob)
