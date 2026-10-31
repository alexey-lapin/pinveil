//! Black-box HTTP contract suite (APP.md, "HTTP interface"). Every test starts the real
//! `pinveil-server` binary with its own configuration and talks to it over HTTP.
//! Test names follow `docs/coverage.md` ids in snake case.

mod harness;
mod web;
