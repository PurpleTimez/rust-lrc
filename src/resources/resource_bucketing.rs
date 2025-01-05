

use std::sync::Mutex;

use crate::resources::interface::ResourceBucketer;

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum ErrBucketing {
	ProtocolLimits,
	ProtectedPercentage,
	NoInFlightLiquidity,
	NoHTLCSlotsOccupied,
}

/// A resource manager that reserves a percentage of resources
/// for HTLCs that are protected.
pub struct BucketResourceManager
{
	mut_bucket_resource_manager: Mutex<MutBucketResourceManager>
}

struct MutBucketResourceManager {
	/// General liquidity available.
	general_liquidity_msat: u64,
	/// General slots available.
	general_slots: u64,
	/// In flight liquidity locked up.
	in_flight_liquidity_msat: u64,
	/// In flight HTLCs slots locked up.
	in_flight_slots: u64,
}

impl BucketResourceManager {
	pub(crate) fn new(total_liquidity_msat: u64, total_slots: u64, protected_percentage: u64) -> Result<Self, ErrBucketing> {
		if total_slots > 483 {
			return Err(ErrBucketing::ProtocolLimits);
		}

		if protected_percentage > 100 {
			return Err(ErrBucketing::ProtectedPercentage);
		}

		let protected_liquidity = total_liquidity_msat * protected_percentage / 100;
		let protected_slots = total_slots * protected_percentage / 100;

		Ok(BucketResourceManager {
			mut_bucket_resource_manager: Mutex::new(
				MutBucketResourceManager {
					general_liquidity_msat: total_liquidity_msat - protected_liquidity,
					general_slots: total_slots - protected_slots,
					in_flight_liquidity_msat: 0,
					in_flight_slots: 0,
				}
			)
		})
	}
}

impl ResourceBucketer for BucketResourceManager {
	fn add_htlc(&self, protected: bool, htlc_amount_msat: u64) -> bool {
		if protected {
			return true;
		}

		if let Ok(ref mut mut_brm) = self.mut_bucket_resource_manager.lock() {
			if mut_brm.in_flight_liquidity_msat + htlc_amount_msat > mut_brm.general_liquidity_msat {
				return false;
			}

			if mut_brm.in_flight_slots+1 > mut_brm.general_slots {
				return false;
			}

			mut_brm.in_flight_liquidity_msat += htlc_amount_msat;
			mut_brm.in_flight_slots += 1;
		}

		return true;
	}

	fn remove_htlc(&self, protected: bool, htlc_amount_msat: u64) -> Result<bool, ErrBucketing> 
	{
		if protected {
			return Ok(true);
		}

		if let Ok(ref mut mut_brm) = self.mut_bucket_resource_manager.lock() {
			if mut_brm.in_flight_liquidity_msat < htlc_amount_msat {
				return Err(ErrBucketing::NoInFlightLiquidity);
			}

			if mut_brm.in_flight_slots == 0 {
				return Err(ErrBucketing::NoHTLCSlotsOccupied);
			}

			mut_brm.in_flight_liquidity_msat -= htlc_amount_msat;
			mut_brm.in_flight_slots -= 1;
		}

		return Ok(true);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_bucket_resource_manager() {
		let bucket_resource_manager_one = BucketResourceManager::new(100_000, 300, 50);
		assert_eq!(bucket_resource_manager_one.is_ok(), true);
		let bucket_resource_manager_two = BucketResourceManager::new(100_000, 500, 50);
		assert_eq!(bucket_resource_manager_two.is_err(), true);
	}

	#[test]
	fn test_bucket_resource_manager_update_htlc() {
		let mut bucket_resource_manager = BucketResourceManager::new(100_000, 300, 50).unwrap();

		bucket_resource_manager.add_htlc(false, 5_000);
		let ret = bucket_resource_manager.remove_htlc(false, 5_000);
		assert_eq!(ret.is_ok(), true);
	}
}
