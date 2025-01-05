
use std::time::Duration;
use std::ops::Deref;

use crate::resources::decaying_average::{DecayingAverage, DecayingAverageStart};
use crate::resources::resource_manager::ManagerConfig;
use crate::resources::interface::{ChannelInfo, Endorsement, ForwardDecision, ForwardOutcome, InFlightHTLC, IncomingReputation, ProposedHTLC, ReputationCheck, ResourceBucketer, ResolvedHTLC, TargetMonitor};
use crate::resources::resource_bucketing::BucketResourceManager;
use crate::resources::reputation_tracker::ReputationTracker;

pub struct TargetChannelTracker<R: Deref>
	where R::Target: ResourceBucketer,
{

	revenue: DecayingAverage,

	/// Expected time to find a block, surfaced to account for simulation scenarios
	/// where this isn't 10 minutes in average.
	block_time: f64,

	/// The amount of time that we reasonably expect a HTLC to resolve in.
	resolution_period: Duration,

	resource_buckets: R,
}

impl <R: Deref>TargetChannelTracker<R>
	where R::Target: ResourceBucketer,
{
	pub(crate) fn new(manager_config: ManagerConfig, chan_info: ChannelInfo, start_value: DecayingAverageStart, resource_buckets: R) -> Result<Self, ()> {

		let decaying_average = DecayingAverage::new(Duration::from_secs(0), start_value);

		return Ok(TargetChannelTracker {
			revenue: decaying_average,
			resource_buckets: resource_buckets,
			block_time: manager_config.block_time.as_secs() as f64,
			resolution_period: manager_config.resolution_period,
		});
	}
}

impl <R: Deref>TargetMonitor for TargetChannelTracker<R>
	where R::Target: ResourceBucketer,
{
	fn add_inflight(&mut self, incoming_reputation: IncomingReputation, proposed_htlc: ProposedHTLC) -> Result<ForwardDecision, ()> {
		
		let reputation_check = ReputationCheck {
			incoming_reputation,
			outgoing_revenue: self.revenue.get_value(),
			htlc_risk: ReputationTracker::outstanding_risk(self.block_time, proposed_htlc.clone(), self.resolution_period),
		};

		let htlc_protected = reputation_check.sufficient_reputation() && proposed_htlc.incoming_endorsed == Endorsement::EndorsementTrue;

		let can_forward = self.resource_buckets.add_htlc(htlc_protected, proposed_htlc.outgoing_amount_msat);

		let outcome = if !can_forward { ForwardOutcome::ForwardOutcomeNoResources }
		else if htlc_protected { ForwardOutcome::ForwardOutcomeEndorsed }
		else { ForwardOutcome::ForwardOutcomeUnendorsed };

		return Ok(ForwardDecision {
			reputation_check,
			forward_outcome: outcome,
		});
	}

	fn resolve_inflight(&mut self, resolved_htlc: ResolvedHTLC, in_flight_htlc: InFlightHTLC) -> Result<bool, ()> {
		
		if in_flight_htlc.outgoing_decision == ForwardOutcome::ForwardOutcomeNoResources {
			return Err(());
		}

		if resolved_htlc.success {
			self.revenue.add(in_flight_htlc.proposed_htlc.forwarding_fee() as f64);
		}

		//TODO: is that a bug ?
		self.resource_buckets.remove_htlc(in_flight_htlc.outgoing_decision == ForwardOutcome::ForwardOutcomeEndorsed,
			in_flight_htlc.proposed_htlc.outgoing_amount_msat);

		return Ok(true);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use std::time::Instant;

	#[test]
	fn test_target_channel_tracker() {
		let manager_config = ManagerConfig::default();
		let chan_info = ChannelInfo {
			in_flight_htlc_limit: 200,
			in_flight_liquidity_limit: 100_000,
		};

		let bucket_resource_manager = BucketResourceManager::new(chan_info.in_flight_liquidity_limit, chan_info.in_flight_htlc_limit, manager_config.protected_percentage);

		let decaying_average_start = DecayingAverageStart {
			last_update: Instant::now(),
			value: 0.0,
		};

		let target_channel_tracker = TargetChannelTracker::new(manager_config, chan_info, decaying_average_start, &mut bucket_resource_manager.unwrap());
	}
}
