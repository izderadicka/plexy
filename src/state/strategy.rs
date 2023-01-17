use rand::Rng;

use crate::error::{Error, Result};

use super::TunnelInfo;

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
        let last = tunnel.last_selected_index.unwrap_or_default();
        return Ok((last + 1) % size);
    }
}
