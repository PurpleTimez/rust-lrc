
use std::time::Instant;
use crate::resources::resource_bucketing::ErrBucketing;
use crate::resources::reputation_tracker::ErrReputation;

/// An interface representing an entity that tracks the reputation of
/// channel peers based on HTLC forwarding behavior.
pub trait LocalResourceManager {
	/// This updates the reputation manager to reflect that a proposed HTLC has been forwarded.
	///
	/// It requires the forwarding restrictions of the outgoing channel to implement bucketing appropriately.
	fn forward_htlc(&mut self, proposed_htlc: ProposedHTLC, chan_info: ChannelInfo) -> Result<ForwardDecision, ()>;
	/// This updates the reputation manager to reflect that an in-flight htlc has been resolved. It returns
	/// the in-flight HTLC as tracked by the manager. It will error if the HTLC is not found.
	///
	/// Note, that this API expects resolution to be reported for *all* HTLCs, even if the decision to forward
	/// the HTLC was that we have no resources for the forward - this function must still be used to indicate
	/// that the HTLC has been cleared from our state (as it would have been locked in our incoming link).
	fn resolve_htlc(&mut self, resolved_htlc: ResolvedHTLC) -> Result<InFlightHTLC, ()>;
}

/// This contains the action that should be taken for forwarding a HTLC and debugging details of the values used.
#[derive(Clone)]
pub struct ForwardDecision {
	/// This contains the numerical values used in making a reputation decision. 
	pub(crate) reputation_check: ReputationCheck,
	/// This is the action that the caller should take.
	pub(crate) forward_outcome: ForwardOutcome,
}

#[derive(Clone)]
pub(crate) struct IncomingReputation {
	/// Represents the reputation that the forwarding channel has accrued over time.
	pub(crate) incoming_revenue: f64,
	/// Represents the outstanding risk of all of the forwarding party's currently in flight HTLCs.
	pub(crate) in_flight_risk: f64,
}

/// This provides the reputation scores that are used to make a forwarding decision for a HTLC.
///
/// These are surfaced for the sake of debugging and simulation, and wouldn't be used much in a production
/// implementation.
#[derive(Clone)]
pub(crate) struct ReputationCheck {
	/// Represents the reputation that has been built up by the incoming link, and any outstanding
	/// risk that it poses to us.
	pub(crate) incoming_reputation: IncomingReputation,
	/// Represents the cost of using the outgoing link, evaluated based on how valuable it has been
	/// to us in the past.
	pub(crate) outgoing_revenue: f64,
	/// Represents the risk of newly proposed HTLC, should it be used to jam our channel for its full
	/// expiry time.
	pub(crate) htlc_risk: f64,
}

impl ReputationCheck {
	/// Returns a boolean indicating whether a HTLC meets the reputation bar to be forwarded with endorsment.
	pub(crate) fn sufficient_reputation(&self) -> bool {
		return self.incoming_reputation.incoming_revenue > self.outgoing_revenue + self.incoming_reputation.in_flight_risk + self.htlc_risk;
	}
}

/// This represents the various forwarding outcomes for a proposed HTLC forward.
#[derive(Clone, PartialEq)]
pub(crate) enum ForwardOutcome {
	/// This means that a HTLC should be dropped because the resource bucket that it qualifies for is full.
	ForwardOutcomeNoResources,
	/// This means that the HTLC should be forwarded but not endorsed.
	ForwardOutcomeUnendorsed,
	/// This means that the HTLC should be forwarded with a positive endorsment signal.
	ForwardOutcomeEndorsed,
}

/// This implements basic resource bucketing for local resource conservation.
pub trait ResourceBucketer {
	/// This poses a HTLC to the resource manager for addition to its appropriate bucket.
	///
	/// If there is space for the HTLC, this call will update internal state and return true.
	/// If the bucket is full, the resource manaager will return false and its state will remain unchanged.
	fn add_htlc(&self, protected: bool, htlc_amount_msat: u64) -> bool;
	/// This updates the resource manager to remove an in-flight HTLC from its appropriate bucket.
	///
	/// Note that this must *only* be called for HTLCs that were added with a true response.
	fn remove_htlc(&self, protected: bool, htlc_amount_msat: u64) -> Result<bool, ErrBucketing>;
}

/// This is an interface that represents the tracking of reputation for links forwarding HTLCs.
pub trait ReputationMonitor {
	/// This updates the reputation monitor for an incoming link to reflect that it currently has an outstanding
	/// forwarded HTLC.
	fn add_inflight(&mut self, proposed_htlc: ProposedHTLC, outgoing_decision: ForwardOutcome) -> Result<bool, ErrReputation>;
	/// This updates the reputation monitor to resolve a previously in-flight HTLC.
	fn resolve_inflight(&mut self, resolved_htlc: ResolvedHTLC) -> Result<InFlightHTLC, ErrReputation>;
	/// This returns the details of a reputation monitor's current standing.
	fn incoming_reputation(&mut self) -> IncomingReputation;
}

/// This is an interface that represents the tracking of forwarding revenues for targeted outgoing links.
pub trait TargetMonitor {
	/// This proposes the addition of a HTLC to the outgoing channel, returning a forwarding decision for the HTLC based
	/// on its endorsment and the reputation of the incoming link.
	fn add_inflight(&mut self, incoming_reputation: IncomingReputation, proposed_htlc: ProposedHTLC) -> Result<ForwardDecision, ()>;
	/// This removes a HTLC from the outgoing channel.
	fn resolve_inflight(&mut self, resolved_htlc: ResolvedHTLC, in_flight_htlc: InFlightHTLC) -> Result<bool, ()>;
}

/// This represents the endorsment signaling that is passed along with a HTLC.
#[derive(Clone, PartialEq)]
pub(crate) enum Endorsement {
	/// This indicates that the TLV was not present.
	EndorsementNone,
	/// This indicates that the TLV was present with a zero value.
	EndorsementFalse,
	/// This indicates that the TLV was present with a non-zero value.
	EndorsementTrue,
}

impl Endorsement {
	fn new_endorsement_signal(endorse: bool) -> Self {
		if endorse {
			return Endorsement::EndorsementTrue;
		}

		return Endorsement::EndorsementFalse;
	}
}

/// This provides information about a HTLC has been locked in on our incoming channel, but not yet forwarded.
#[derive(Clone)]
pub(crate) struct ProposedHTLC {
	/// The channel that has sent this HTLC to the local node for forwarding.
	pub(crate) incoming_channel: u64,
	/// This is the outgoing channel that the sending node has requested.
	pub(crate) outgoing_channel: u64,
	/// This is the HTLC index on the incoming channel.
	pub(crate) incoming_index: u32,
	/// This indicates whether the incoming channel forwarded this HTLC as endorsed.
	pub(crate) incoming_endorsed: Endorsement,
	/// This is the amount of the HTLC on the incoming channel.
	incoming_amount_msat: u64,
	/// This is the amount of the HTLC on the outgoing channel.
	pub(crate) outgoing_amount_msat: u64,
	/// This is difference between the block height at which the HTLC was forwarded
	/// and its outgoing CLTV expiry.
	pub(crate) cltv_expiry_delta: u32,
}

impl ProposedHTLC {
	pub(crate) fn forwarding_fee(&self) -> u64 {
		return self.incoming_amount_msat - self.outgoing_amount_msat;
	}
}

/// This tracks a HTLC forward that is currently in flight.
#[derive(Clone)]
pub(crate) struct InFlightHTLC {
	/// This is the time at which the incoming HTLC was added to the incoming channel.
	pub(crate) timestamp_added: Instant,
	/// This indicates what resource allocation was assigned to the outgoing HTLC.
	pub(crate) outgoing_decision: ForwardOutcome,
	/// This contains the original details of the HTLC that was forwarded to us.
	pub(crate) proposed_htlc: ProposedHTLC,
}

/// This summarizes the resolution of an in-flight HTLC.
#[derive(Clone)]
pub(crate) struct ResolvedHTLC {
	/// This is the time at which a HTLC was resolved.
	pub(crate) timestamp_settled: Instant,
	/// This is the short channel ID of the channel that originally forwarded the incoming HTLC.
	pub(crate) incoming_index: u32,
	/// This is the HTLC ID on the outgoing link. Note that HTLCs that fail locally won't have this value assigned.
	pub(crate) incoming_channel: u64,
	/// RThis is the HTLC ID on the outgoing link. Note that HTLCs that fail locally won't have this value assigned.
	outgoing_index: u32,
	/// This is the short channel ID of the channel that forwarded the outgoing HTLC.
	pub(crate) outgoing_channel: u64,
	/// This is true if the HTLC was fulfilled.
	pub(crate) success: bool,
}

/// This represents a HTLC that our node has previously forwarded.
struct ForwardedHTLC {
	/// This contains the original forwarding details of the HTLC.
	in_flight_htlc: InFlightHTLC,
	/// This contains the details of the HTLC's resolution if it has been finally settled or faield.
	resolution: ResolvedHTLC,
}

/// This provides information about a channel's routing restrictions.
pub(crate) struct ChannelInfo {
	/// Total number of HTLCs allowed in-flight.
	pub(crate) in_flight_htlc_limit: u64,
	/// Total amouhnt of liquidity allowed in-flight.
	pub(crate) in_flight_liquidity_limit: u64,
}
