
use core::time::Duration;
use std::time::Instant;
use std::collections::HashMap;
use std::ops::Sub;

use crate::resources::decaying_average::{DecayingAverage, DecayingAverageStart};
use crate::resources::interface::{Endorsement, ForwardOutcome, InFlightHTLC, IncomingReputation, ProposedHTLC, ReputationMonitor, ResolvedHTLC};


#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ErrReputation {
	ResolutionNotFound,
}

pub struct ReputationTracker
{
	revenue: DecayingAverage,
	in_flight_htlcs: HashMap<u32, InFlightHTLC>,
	block_time: f64,
	resolution_period: Duration,
}

impl ReputationTracker {
	fn new() -> Self {

		let decaying_average_start = DecayingAverageStart {
			last_update: Instant::now(),
			value: 0.0,
		};
		//TODO: reputationWindows
		let decaying_average = DecayingAverage::new(Duration::from_secs(0), decaying_average_start);

		ReputationTracker {
			revenue: decaying_average,
			in_flight_htlcs: HashMap::new(),
			block_time: 60.0 * 10.0,
			resolution_period: Duration::from_secs(90),
		}
	}
}

impl ReputationTracker {
	pub(crate) fn outstanding_risk(block_time: f64, proposed_htlc: ProposedHTLC, resolution_period: Duration) -> f64 {
		return (proposed_htlc.forwarding_fee() as f64 * proposed_htlc.cltv_expiry_delta as f64 * block_time * 60.0) /
			resolution_period.as_secs() as f64
	}

	/// Returns the total outstanding risk of the incoming in-flight HTLCs from a specific channel.
	fn in_flight_htlc_risk(&self) -> f64 {
		let mut chan_in_flight_risk = 0.0;

		for (_, val) in self.in_flight_htlcs.iter() {
			if val.proposed_htlc.incoming_endorsed != Endorsement::EndorsementTrue {
				continue;
			}
			chan_in_flight_risk += Self::outstanding_risk(self.block_time, val.proposed_htlc.clone(), self.resolution_period);
		}
		return chan_in_flight_risk;
	}

	fn effective_fees(&self, resolution_period: Duration, timestamp_settled: Instant, htlc: InFlightHTLC, success: bool) -> f64 {
		
		let resolution_time = timestamp_settled.sub(htlc.timestamp_added).as_secs();
		let resolution_period_sec = resolution_period.as_secs();
		let fee = htlc.proposed_htlc.forwarding_fee() as f64;

		//TODO: is code correct ?
		let opportunity_cost = ((resolution_time - resolution_period_sec) / resolution_period_sec) as f64 * fee as f64;

		if htlc.proposed_htlc.incoming_endorsed == Endorsement::EndorsementTrue && success { return (fee - opportunity_cost) as f64; }
		if htlc.proposed_htlc.incoming_endorsed == Endorsement::EndorsementTrue { return (-1 as f64 * opportunity_cost) as f64; }
		if success { if resolution_time <= resolution_period_sec { return fee as f64; } else { return 0.0 } }

		return 0.0;
	}
}

impl ReputationMonitor for ReputationTracker {
	fn add_inflight(&mut self, proposed_htlc: ProposedHTLC, outgoing_decision: ForwardOutcome) -> Result<bool, ErrReputation> {

		let in_flight_htlc = InFlightHTLC {
			timestamp_added: Instant::now(),
			outgoing_decision, 
			proposed_htlc: proposed_htlc.clone(),
		};

		self.in_flight_htlcs.insert(proposed_htlc.incoming_index, in_flight_htlc);

		return Ok(true);
	}

	fn resolve_inflight(&mut self, resolved_htlc: ResolvedHTLC) -> Result<InFlightHTLC, ErrReputation> {

		if let Some(in_flight_htlc) = self.in_flight_htlcs.remove(&resolved_htlc.incoming_index) {
			let effective_fees = self.effective_fees(self.resolution_period, resolved_htlc.timestamp_settled, in_flight_htlc.clone(), resolved_htlc.success);
			
			self.revenue.add(effective_fees);
				
			return Ok(in_flight_htlc);
		}
		return Err(ErrReputation::ResolutionNotFound);
	}

	fn incoming_reputation(&mut self) -> IncomingReputation {
		return IncomingReputation {
			incoming_revenue: self.revenue.get_value(),
			in_flight_risk: self.in_flight_htlc_risk(),
		}
	}
}


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_reputation_tracker() {
		let reputation_tracker = ReputationTracker::new();
	}
}
