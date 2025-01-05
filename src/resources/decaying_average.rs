
use core::time::{Duration};
use std::time::Instant;
use std::ops::Sub;

#[derive(PartialEq, Eq, Debug, Clone)]
enum ErrDecayingAverage {
	TimeAdditionError,
}

pub(crate) struct DecayingAverageStart {
	pub(crate) last_update: Instant,
	pub(crate) value: f64,
}

pub(crate) struct DecayingAverage {
	last_update: Instant,
	value: f64,
	decay_rate: f64,
}

fn calculate_decay_rate(period: Duration) -> f64 {
	return (0.5 as f64).powf(2 as f64 / period.as_secs() as f64);
}

impl DecayingAverage {
	pub(crate) fn new(period: Duration, start_value: DecayingAverageStart) -> Self {

		let mut last_update = Instant::now();
		let mut value = 0.0;

		//TODO: complete check ?
		if start_value.value != 0.0 {
			last_update = start_value.last_update;
			value = start_value.value;
		}

		return DecayingAverage {
			last_update: last_update,
			value: value,
			decay_rate: calculate_decay_rate(period),
		}
	}

	fn update(&mut self, update_time: Instant) {
		let last_update_diff = update_time.sub(update_time);

		if last_update_diff == Duration::from_secs(0) {
			return;
		}

		self.value = self.value * self.decay_rate.powf(last_update_diff.as_secs() as f64);
		self.last_update = update_time;
	}

	pub(crate) fn add(&mut self, value: f64) {
		self.add_time(value, Instant::now());
	}

	pub(crate) fn get_value(&mut self) -> f64 {
		self.update(Instant::now());
		return self.value;
	}

	/// We cannot add a value at a specific timestamp that is before our last update.
	fn add_time(&mut self, value: f64, specific_timestamp: Instant) -> Result<bool, ErrDecayingAverage> {
		if !self.last_update.checked_duration_since(specific_timestamp).is_none() {
			return Err(ErrDecayingAverage::TimeAdditionError);
		}

		self.update(specific_timestamp);
		self.value += value;
		self.last_update = specific_timestamp;

		return Ok(true);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::thread::sleep;

	#[test]
	fn test_decaying_average() {
		let decaying_average_start = DecayingAverageStart {
			last_update: Instant::now(),
			value: 0.0,
		};
		let decaying_average = DecayingAverage::new(Duration::from_secs(0), decaying_average_start);
	}

	#[test]
	fn test_decaying_average_add_time() {
		let decaying_average_start = DecayingAverageStart {
			last_update: Instant::now(),
			value: 0.0,
		};
		let mut decaying_average = DecayingAverage::new(Duration::from_secs(0), decaying_average_start);
		sleep(Duration::new(1, 0));
		let ret = decaying_average.add_time(1.0, Instant::now());	
		assert_eq!(ret.is_ok(), true);
	}
}
