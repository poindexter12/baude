/// A timing stage recorded during startup.
#[derive(Clone, Debug)]
pub struct TimingStage {
    pub name: &'static str,
    pub duration_ms: u128,
}

/// Startup timing for the daemon.
#[derive(Clone, Debug)]
pub struct StartupTiming {
    pub stages: Vec<TimingStage>,
    pub total_ms: u128,
}

impl StartupTiming {
    /// Create a new timing with the total duration.
    pub fn new(total_ms: u128) -> Self {
        StartupTiming {
            stages: Vec::new(),
            total_ms,
        }
    }

    /// Add a timing stage.
    #[allow(dead_code)]
    pub fn add_stage(&mut self, name: &'static str, duration_ms: u128) {
        self.stages.push(TimingStage { name, duration_ms });
    }

    /// Convert stages to a HashMap for JSON serialization.
    pub fn to_hashmap(&self) -> std::collections::HashMap<String, u128> {
        let mut map = std::collections::HashMap::new();
        for stage in &self.stages {
            map.insert(stage.name.to_string(), stage.duration_ms);
        }
        map.insert("total".to_string(), self.total_ms);
        map
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_stage_creation() {
        let stage = TimingStage {
            name: "config_load",
            duration_ms: 5,
        };
        assert_eq!(stage.name, "config_load");
        assert_eq!(stage.duration_ms, 5);
    }

    #[test]
    fn startup_timing_to_hashmap() {
        let mut timing = StartupTiming::new(100);
        timing.add_stage("config_load", 20);
        timing.add_stage("state_load", 30);
        timing.add_stage("listener_bound", 50);

        let map = timing.to_hashmap();
        assert_eq!(map.get("config_load"), Some(&20));
        assert_eq!(map.get("state_load"), Some(&30));
        assert_eq!(map.get("listener_bound"), Some(&50));
        assert_eq!(map.get("total"), Some(&100));
    }
}
