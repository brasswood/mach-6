use selectors::matching::TimingStats;

#[derive(Clone, Debug, Default)]
pub struct Samples<T>(Vec<T>); 

impl<T> Samples<T> {
    pub fn from_vec(value: Vec<T>) -> Self {
        Self(value)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn mean(&self) -> <T as Mean>::Output
    where
        T: Mean,
    {
        T::mean(self.as_slice())
    }

    pub fn stddev(&self) -> <T as StdDev>::Output
    where
        T: Mean + StdDev,
    {
        let mean = self.mean();
        T::stddev(self.as_slice(), &mean)
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.0.iter()
    }

    pub fn into_iter(self) -> std::vec::IntoIter<T> {
        self.0.into_iter()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn first(&self) -> Option<&T> {
        self.0.first()
    }

}

impl<T> From<Vec<T>> for Samples<T> {
    fn from(value: Vec<T>) -> Samples<T> {
        Samples(value)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OnlineDurationStats {
    num_samples: usize,
    mean_cycles: f64,
    /// The running sum of squared deviations from the mean
    m2_cycles: f64,
}

impl OnlineDurationStats {
    pub fn push(&mut self, sample: tsc_timer::Duration) {
        let x = sample.cycles() as f64;
        self.num_samples += 1;
        let delta = x - self.mean_cycles;
        self.mean_cycles += delta / self.num_samples as f64;
        let delta2 = x - self.mean_cycles;
        self.m2_cycles += delta * delta2;
    }

    pub fn mean(&self) -> tsc_timer::Duration {
        assert!(self.num_samples != 0, "tried to compute online mean with no samples");
        tsc_timer::Duration::from_cycles(self.mean_cycles.round() as u64)
    }

    pub fn stddev(&self) -> tsc_timer::Duration {
        assert!(self.num_samples != 0, "tried to compute online stddev with no samples");
        let variance = self.m2_cycles / self.num_samples as f64;
        tsc_timer::Duration::from_cycles(variance.sqrt().round() as u64)
    }
}

pub trait Mean {
    type Output;

    fn mean(samples: &[Self]) -> Self::Output
    where
        Self: Sized;
}

pub trait StdDev: Mean {
    type Output;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output
    where
        Self: Sized;
}

impl Mean for tsc_timer::Duration {
    type Output = tsc_timer::Duration;

    fn mean(samples: &[Self]) -> Self::Output {
        assert!(!samples.is_empty(), "tried to compute mean of empty sample set");
        let total: tsc_timer::Duration = samples.iter().copied().fold(tsc_timer::Duration::default(), |acc, d| acc + d);
        total / samples.len() as u64
    }
}

impl StdDev for tsc_timer::Duration {
    type Output = tsc_timer::Duration;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output {
        assert!(
            !samples.is_empty(),
            "tried to compute standard deviation of empty sample set"
        );
        let variance = samples
            .iter()
            .map(|sample| {
                let delta = sample.cycles() as f64 - mean.cycles() as f64;
                delta * delta
            })
            .sum::<f64>()
            / samples.len() as f64;
        tsc_timer::Duration::from_cycles(variance.sqrt().round() as u64)
    }
}

impl Mean for TimingStats {
    type Output = TimingStats;

    fn mean(samples: &[Self]) -> Self::Output {
        assert!(!samples.is_empty(), "tried to compute mean of empty sample set");
        let iter = samples.iter().copied();
        let sum = iter
            .reduce(|l, r| l + r)
            .expect("tried to compute mean of empty sample set");
        sum / samples.len() as u64
    }
}

impl StdDev for TimingStats {
    type Output = TimingStats;

    fn stddev(samples: &[Self], mean: &<Self as Mean>::Output) -> <Self as StdDev>::Output {
        assert!(
            !samples.is_empty(),
            "tried to compute standard deviation of empty sample set"
        );

        let stddev = |project: fn(&TimingStats) -> tsc_timer::Duration| {
            let variance = samples
                .iter()
                .map(|sample| {
                    let delta = project(sample).cycles() as f64 - project(mean).cycles() as f64;
                    delta * delta
                })
                .sum::<f64>()
                / samples.len() as f64;
            tsc_timer::Duration::from_cycles(variance.sqrt().round() as u64)
        };

        TimingStats {
            updating_bloom_filter: stddev(|sample| sample.updating_bloom_filter),
            checking_style_sharing: stddev(|sample| sample.checking_style_sharing),
            querying_selector_map: stddev(|sample| sample.querying_selector_map),
            fast_rejecting: stddev(|sample| sample.fast_rejecting),
            slow_rejecting: stddev(|sample| sample.slow_rejecting),
            slow_accepting: stddev(|sample| sample.slow_accepting),
            inserting_into_sharing_cache: stddev(|sample| sample.inserting_into_sharing_cache),
            _time_inside_buckets: stddev(|sample| sample._time_inside_buckets),
        }
    }
}

