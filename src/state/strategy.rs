use std::{fmt::Display, str::FromStr};

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::TunnelInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TunnelLBStrategy {
    Random,
    RoundRobin,
    MinimumOpenConnections,
}

impl Default for TunnelLBStrategy {
    fn default() -> Self {
        TunnelLBStrategy::Random
    }
}

impl TunnelLBStrategy {
    pub fn create(&self) -> Box<dyn LBStrategy + Send + Sync + 'static> {
        match self {
            TunnelLBStrategy::Random => Box::new(Random),
            TunnelLBStrategy::RoundRobin => Box::new(RoundRobin),
            TunnelLBStrategy::MinimumOpenConnections => Box::new(MinimumOpenConnections),
        }
    }
}

impl FromStr for TunnelLBStrategy {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "random" => Ok(TunnelLBStrategy::Random),
            "roundrobin" | "round-robin" | "round_robin" => Ok(TunnelLBStrategy::RoundRobin),
            "minimum-open-connections"
            | "minimum_open_connections"
            | "minimumopenconnections"
            | "min-open-connections"
            | "min_open_connections"
            | "minopenconnections" => Ok(TunnelLBStrategy::MinimumOpenConnections),
            _ => Err(Error::InvalidLBStrategy),
        }
    }
}

impl Display for TunnelLBStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TunnelLBStrategy::Random => write!(f, "random"),
            TunnelLBStrategy::RoundRobin => write!(f, "round-robin"),
            TunnelLBStrategy::MinimumOpenConnections => write!(f, "minimum-open-connections"),
        }
    }
}

pub trait LBStrategy: std::fmt::Debug {
    fn select_remote(&self, tunnel: &TunnelInfo) -> Result<usize>;
}

#[derive(Debug)]
pub struct Random;

impl LBStrategy for Random {
    fn select_remote(&self, tunnel: &TunnelInfo) -> Result<usize> {
        let size = tunnel.remotes.len();
        let idx: usize = rand::thread_rng().gen_range(0..size);
        Ok(idx)
    }
}

#[derive(Debug)]
pub struct RoundRobin;

impl LBStrategy for RoundRobin {
    fn select_remote(&self, tunnel: &TunnelInfo) -> Result<usize> {
        let size = tunnel.remotes.len();
        let last = tunnel
            .last_selected_index
            .unwrap_or_else(|| tunnel.remotes.len().saturating_sub(1));
        Ok((last + 1) % size)
    }
}

#[derive(Debug)]
pub struct MinimumOpenConnections;

impl LBStrategy for MinimumOpenConnections {
    fn select_remote(&self, tunnel: &TunnelInfo) -> Result<usize> {
        let mut min_idx = 0usize;
        let mut min_val = usize::MAX;
        for (idx, open_conns) in tunnel
            .remotes
            .iter()
            .map(|(_, r)| r.stats.streams_open + r.stats.streams_pending)
            .enumerate()
        {
            if open_conns == 0 {
                return Ok(idx);
            } else if open_conns < min_val {
                min_idx = idx;
                min_val = open_conns;
            }
        }

        return Ok(min_idx);
    }
}
