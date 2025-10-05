/// Vector clock implementation for causality tracking
///
/// Vector clocks allow us to determine the causal relationship between
/// events in a distributed system without synchronized clocks.

use std::collections::HashMap;
use std::cmp::Ordering;
use serde::{Deserialize, Serialize};
use crate::EndpointId;

/// A vector clock tracks logical time across multiple endpoints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    /// Clock values for each endpoint
    clocks: HashMap<EndpointId, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Create a vector clock with an initial value for the local endpoint
    pub fn with_local(endpoint_id: EndpointId, initial_value: u64) -> Self {
        let mut clocks = HashMap::new();
        clocks.insert(endpoint_id, initial_value);
        Self { clocks }
    }

    /// Get the clock value for a specific endpoint
    pub fn get(&self, endpoint_id: &EndpointId) -> u64 {
        self.clocks.get(endpoint_id).copied().unwrap_or(0)
    }

    /// Increment the clock for a specific endpoint
    pub fn increment(&mut self, endpoint_id: &EndpointId) -> u64 {
        let value = self.clocks.entry(endpoint_id.clone()).or_insert(0);
        *value += 1;
        *value
    }

    /// Update the clock value for a specific endpoint
    pub fn update(&mut self, endpoint_id: EndpointId, value: u64) {
        self.clocks.insert(endpoint_id, value);
    }

    /// Merge another vector clock into this one, taking the maximum of each component
    pub fn merge(&mut self, other: &VectorClock) {
        for (endpoint_id, &other_value) in &other.clocks {
            let our_value = self.clocks.entry(endpoint_id.clone()).or_insert(0);
            *our_value = (*our_value).max(other_value);
        }
    }

    /// Check if this vector clock happens before another
    ///
    /// Returns true if all components of self are <= the corresponding
    /// components of other, and at least one is strictly less.
    pub fn happens_before(&self, other: &VectorClock) -> bool {
        let mut all_less_or_equal = true;
        let mut exists_strictly_less = false;

        // Check all our components
        for (endpoint_id, &our_value) in &self.clocks {
            let their_value = other.get(endpoint_id);
            if our_value > their_value {
                all_less_or_equal = false;
                break;
            }
            if our_value < their_value {
                exists_strictly_less = true;
            }
        }

        // Check if they have any components we don't
        for endpoint_id in other.clocks.keys() {
            if !self.clocks.contains_key(endpoint_id) && other.get(endpoint_id) > 0 {
                exists_strictly_less = true;
            }
        }

        all_less_or_equal && exists_strictly_less
    }

    /// Check if this vector clock happens after another
    pub fn happens_after(&self, other: &VectorClock) -> bool {
        other.happens_before(self)
    }

    /// Check if two vector clocks are concurrent (neither happens before the other)
    pub fn concurrent_with(&self, other: &VectorClock) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }

    /// Compare two vector clocks
    pub fn compare(&self, other: &VectorClock) -> ClockOrdering {
        if self == other {
            ClockOrdering::Equal
        } else if self.happens_before(other) {
            ClockOrdering::Before
        } else if self.happens_after(other) {
            ClockOrdering::After
        } else {
            ClockOrdering::Concurrent
        }
    }

    /// Get all endpoint IDs in this vector clock
    pub fn endpoints(&self) -> impl Iterator<Item = &EndpointId> {
        self.clocks.keys()
    }

    /// Get the total number of endpoints tracked
    pub fn len(&self) -> usize {
        self.clocks.len()
    }

    /// Check if the vector clock is empty
    pub fn is_empty(&self) -> bool {
        self.clocks.is_empty()
    }

    /// Create a copy of this clock with the specified endpoint incremented
    pub fn incremented(&self, endpoint_id: &EndpointId) -> Self {
        let mut copy = self.clone();
        copy.increment(endpoint_id);
        copy
    }

    /// Get a summary string for debugging
    pub fn summary(&self) -> String {
        let mut parts: Vec<String> = self.clocks
            .iter()
            .map(|(id, value)| format!("{}:{}", &id.0[..8], value))
            .collect();
        parts.sort();
        format!("[{}]", parts.join(", "))
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// The ordering relationship between two vector clocks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockOrdering {
    /// The first clock happens before the second
    Before,
    /// The first clock happens after the second
    After,
    /// The clocks are equal
    Equal,
    /// The clocks are concurrent (no causal relationship)
    Concurrent,
}

impl ClockOrdering {
    /// Check if there's a definite ordering (not concurrent)
    pub fn is_ordered(&self) -> bool {
        !matches!(self, ClockOrdering::Concurrent)
    }

    /// Check if the clocks are concurrent
    pub fn is_concurrent(&self) -> bool {
        matches!(self, ClockOrdering::Concurrent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(name: &str) -> EndpointId {
        EndpointId::from_str(name)
    }

    #[test]
    fn test_vector_clock_basic() {
        let mut clock = VectorClock::new();
        assert!(clock.is_empty());

        let ep1 = endpoint("node1");
        assert_eq!(clock.get(&ep1), 0);

        clock.increment(&ep1);
        assert_eq!(clock.get(&ep1), 1);

        clock.increment(&ep1);
        assert_eq!(clock.get(&ep1), 2);
    }

    #[test]
    fn test_vector_clock_merge() {
        let ep1 = endpoint("node1");
        let ep2 = endpoint("node2");

        let mut clock1 = VectorClock::new();
        clock1.update(ep1.clone(), 5);
        clock1.update(ep2.clone(), 3);

        let mut clock2 = VectorClock::new();
        clock2.update(ep1.clone(), 3);
        clock2.update(ep2.clone(), 7);

        clock1.merge(&clock2);
        assert_eq!(clock1.get(&ep1), 5); // max(5, 3) = 5
        assert_eq!(clock1.get(&ep2), 7); // max(3, 7) = 7
    }

    #[test]
    fn test_happens_before() {
        let ep1 = endpoint("node1");
        let ep2 = endpoint("node2");

        let mut clock1 = VectorClock::new();
        clock1.update(ep1.clone(), 1);
        clock1.update(ep2.clone(), 2);

        let mut clock2 = VectorClock::new();
        clock2.update(ep1.clone(), 1);
        clock2.update(ep2.clone(), 3);

        assert!(clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
        assert!(clock2.happens_after(&clock1));
    }

    #[test]
    fn test_concurrent() {
        let ep1 = endpoint("node1");
        let ep2 = endpoint("node2");

        let mut clock1 = VectorClock::new();
        clock1.update(ep1.clone(), 2);
        clock1.update(ep2.clone(), 1);

        let mut clock2 = VectorClock::new();
        clock2.update(ep1.clone(), 1);
        clock2.update(ep2.clone(), 2);

        assert!(clock1.concurrent_with(&clock2));
        assert!(clock2.concurrent_with(&clock1));
        assert_eq!(clock1.compare(&clock2), ClockOrdering::Concurrent);
    }

    #[test]
    fn test_equal_clocks() {
        let ep1 = endpoint("node1");

        let mut clock1 = VectorClock::new();
        clock1.update(ep1.clone(), 5);

        let mut clock2 = VectorClock::new();
        clock2.update(ep1.clone(), 5);

        assert_eq!(clock1, clock2);
        assert!(!clock1.happens_before(&clock2));
        assert!(!clock2.happens_before(&clock1));
        assert!(!clock1.concurrent_with(&clock2));
        assert_eq!(clock1.compare(&clock2), ClockOrdering::Equal);
    }
}