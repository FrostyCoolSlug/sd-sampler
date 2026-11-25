//! These are standalone audio processors. They all do pretty similar things, take an audio file
//! run it through and produce a result. For now, it's just peak calculation, ebu will probably
//! end up here as well later, and if we get far enough, I might abstract out the actual player
//! so all the common parts are shared.

pub(crate) mod peaks;
