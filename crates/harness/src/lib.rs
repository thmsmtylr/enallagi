//! An autonomous task loop for a coding agent, installed into any git repository. Every module below is one stage of that loop or one thing it reads: the queue, the roles, the gates, the probes and the event log.

pub mod agent;
pub mod archive;
pub mod cli;
pub mod config;
pub mod eject;
pub mod eval;
pub mod events;
pub mod fixture;
pub mod gates;
pub mod git;
pub mod hooks;
pub mod init;
pub mod issue;
pub mod pipeline;
pub mod pr;
pub mod probes;
pub mod queue;
pub mod review;
pub mod roles;
pub mod runners;
pub mod skills;
pub mod tui;
pub mod worktree;
