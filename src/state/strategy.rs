use std::str::FromStr;

use rand::Rng;

use crate::error::{Error, Result};

use super::TunnelInfo;

#[derive(Debug, Clone)]
pub enum TunnelLBStrategy {
    Random,
    RoundRobin,
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
        }
    }
}

impl FromStr for TunnelLBStrategy {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "random" => Ok(TunnelLBStrategy::Random),
            "roundrobin" | "round-robin" | "round_robin" => Ok(TunnelLBStrategy::RoundRobin),
            _ => Err(Error::InvalidLBStrategy),
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
