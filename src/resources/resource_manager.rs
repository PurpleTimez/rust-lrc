
use core::time::Duration;
use std::collections::HashMap;
use std::ops::Deref;
use std::time::Instant;
use std::sync::Mutex;

use crate::resources::decaying_average::{DecayingAverage, DecayingAverageStart};
use crate::resources::reputation_tracker::ReputationTracker;
use crate::resources::target_tracker::TargetChannelTracker;
use crate::resources::interface::{ChannelInfo, ForwardDecision, ForwardOutcome, InFlightHTLC, LocalResourceManager, ProposedHTLC, ReputationCheck, ReputationMonitor, ResourceBucketer, ResolvedHTLC, TargetMonitor};
use crate::resources::resource_bucketing::BucketResourceManager;

const MAX_MILLI_SATOSHI: u64 = 21_000_000 * 1000;

#[derive(Copy, Clone, Debug)]
pub(crate) struct ManagerConfig {
	/// Amount of time we examine the revenue of the outgoing links over.
	revenue_window: Duration,
	/// Multiplier on revenue window that is used to determine the longer period of time
	/// that incoming links reputation is assessed over.
	reputation_multiplier: u8,
	/// Percentage of liquidity and slots that are reserved for high reputation, endorsed HTLCs.
	pub(crate) protected_percentage: u64,
	/// Amount of time that we reasonably expect HTLCs to complete within.
	pub(crate) resolution_period: Duration,
	/// Expected block time.
	pub(crate) block_time: Duration
}

impl Default for ManagerConfig {
	fn default() -> ManagerConfig {
		ManagerConfig {
			revenue_window: Duration::from_secs(60 * 60),
			reputation_multiplier: 24,
			protected_percentage: 50,
			resolution_period: Duration::from_secs(90),
			block_time: Duration::from_secs(60 * 10),
		}
	}
}

impl ManagerConfig {
	fn validate(&self) -> bool {
		if self.protected_percentage > 100 {
			return false;
		}
		if self.resolution_period == Duration::from_secs(0) {
			return false;
		}
		if self.block_time == Duration::from_secs(0) {
			return false;
		}
		return true;
	}

	fn reputation_window(&self) -> Duration {
		return Duration::from_secs(self.revenue_window.as_secs() * self.reputation_multiplier as u64)
	}
		
}

pub struct ResourceManager<R: Deref>
	where R::Target: ResourceBucketer
{
	manager_configuration: ManagerConfig,

	// track channel reputation short chan id -> score (?)
	// TODO: make it a trait
	channel_reputation: HashMap<u64, ReputationTracker>,

	//TODO: make it a trait
	target_channels: HashMap<u64, TargetChannelTracker<R>>,

	resolution_period: Duration,

	block_time: Duration,
}

impl<R: Deref> ResourceManager<R>	
	where R::Target: ResourceBucketer
{
	//TODO: add methods to generate scid's TargetChannelTracker / ReputationTracker
	fn sufficient_reputation(&mut self, proposed_htlc: ProposedHTLC, outgoing_channel_revenue: f64) ->Result<ReputationCheck, ()> {

		if let Some(channel_reputation_tracker) = self.channel_reputation.get_mut(&proposed_htlc.incoming_channel) {
			
			let reputation_check = ReputationCheck {
				incoming_reputation: channel_reputation_tracker.incoming_reputation(),
				outgoing_revenue: outgoing_channel_revenue,
				htlc_risk: ReputationTracker::outstanding_risk(self.manager_configuration.block_time.as_secs() as f64, proposed_htlc.clone(), self.resolution_period)
			};
			return Ok(reputation_check);
		}
		return Err(())
	}
}

impl<R: Deref>LocalResourceManager for ResourceManager<R>
	where R::Target: ResourceBucketer
{	
	fn forward_htlc(&mut self, proposed_htlc: ProposedHTLC, chan_info: ChannelInfo) -> Result<ForwardDecision, ()>
	{
		if proposed_htlc.outgoing_amount_msat > MAX_MILLI_SATOSHI {
			return Err(())
		}

		if let Some(channel_reputation_tracker) = self.channel_reputation.get_mut(&proposed_htlc.incoming_channel) {
			if let Some(target_channel_tracker) = self.target_channels.get_mut(&proposed_htlc.incoming_channel) {
				let forward_decision = target_channel_tracker.add_inflight(channel_reputation_tracker.incoming_reputation(), proposed_htlc.clone()).unwrap();

				if channel_reputation_tracker.add_inflight(proposed_htlc.clone(), forward_decision.clone().forward_outcome).is_err() {
					return Err(())
				}
				return Ok(forward_decision);
			}
		}
		return Err(())
	}
	fn resolve_htlc(&mut self, resolved_htlc: ResolvedHTLC) -> Result<InFlightHTLC, ()> {

		if let Some(channel_reputation_tracker) = self.channel_reputation.get_mut(&resolved_htlc.incoming_channel) {
			let in_flight_ret = channel_reputation_tracker.resolve_inflight(resolved_htlc.clone());
			if in_flight_ret.is_err() { return Err(()) }
			
			let in_flight = in_flight_ret.unwrap();

			if in_flight.outgoing_decision == ForwardOutcome::ForwardOutcomeNoResources { return Err(()) }

			if in_flight.proposed_htlc.outgoing_channel != resolved_htlc.outgoing_channel { return Err(()) }

			if let Some(target_channel_tracker) = self.target_channels.get_mut(&in_flight.proposed_htlc.outgoing_channel) {
				let ret = target_channel_tracker.resolve_inflight(resolved_htlc.clone(), in_flight.clone());
				if ret.is_err() { return Err(()) }
			}
			return Ok(in_flight);
		}
		return Err(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_resource_manager() {
		let manager_config = ManagerConfig::default();
		manager_config.validate();
		manager_config.reputation_window();
	}
}
