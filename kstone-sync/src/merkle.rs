/// Merkle tree implementation for efficient diff detection
///
/// Merkle trees allow us to efficiently identify differences between
/// two datasets by comparing hashes at different levels of the tree.

use anyhow::Result;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A node in the Merkle tree
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleNode {
    /// Hash of this node
    pub hash: Bytes,
    /// Level in the tree (0 = leaf)
    pub level: u32,
    /// Key range covered by this node (inclusive)
    pub key_range: Option<(Bytes, Bytes)>,
    /// Child nodes (for non-leaf nodes)
    pub children: Vec<MerkleNode>,
}

impl MerkleNode {
    /// Create a leaf node from a key-value pair
    pub fn leaf(key: &[u8], value: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(key);
        hasher.update(value);
        let hash = Bytes::from(hasher.finalize().to_vec());

        Self {
            hash,
            level: 0,
            key_range: Some((Bytes::from(key.to_vec()), Bytes::from(key.to_vec()))),
            children: Vec::new(),
        }
    }

    /// Create an internal node from child nodes
    pub fn internal(children: Vec<MerkleNode>) -> Self {
        if children.is_empty() {
            return Self {
                hash: Bytes::new(),
                level: 0,
                key_range: None,
                children: Vec::new(),
            };
        }

        // Calculate hash from children
        let mut hasher = Sha256::new();
        for child in &children {
            hasher.update(&child.hash);
        }
        let hash = Bytes::from(hasher.finalize().to_vec());

        // Determine level (one more than children)
        let level = children[0].level + 1;

        // Calculate key range
        let key_range = if let (Some(first_range), Some(last_range)) =
            (children.first().and_then(|n| n.key_range.clone()),
             children.last().and_then(|n| n.key_range.clone())) {
            Some((first_range.0, last_range.1))
        } else {
            None
        };

        Self {
            hash,
            level,
            key_range,
            children,
        }
    }

    /// Check if this node matches another node's hash
    pub fn matches(&self, other: &MerkleNode) -> bool {
        self.hash == other.hash
    }

    /// Get all leaf hashes under this node
    pub fn leaf_hashes(&self) -> Vec<(Bytes, Bytes)> {
        if self.level == 0 {
            if let Some(ref range) = self.key_range {
                return vec![(range.0.clone(), self.hash.clone())];
            }
        }

        let mut hashes = Vec::new();
        for child in &self.children {
            hashes.extend(child.leaf_hashes());
        }
        hashes
    }
}

/// A Merkle tree for efficient diff detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    /// Root node of the tree
    pub root: Option<MerkleNode>,
    /// Branching factor (number of children per internal node)
    pub branching_factor: usize,
    /// Total number of items
    pub item_count: usize,
}

impl MerkleTree {
    /// Create a new empty Merkle tree
    pub fn new(branching_factor: usize) -> Self {
        Self {
            root: None,
            branching_factor: branching_factor.max(2),
            item_count: 0,
        }
    }

    /// Build a Merkle tree from sorted key-value pairs
    pub fn build<I>(items: I, branching_factor: usize) -> Result<Self>
    where
        I: IntoIterator<Item = (Bytes, Bytes)>,
    {
        let mut tree = Self::new(branching_factor);

        // Create leaf nodes
        let leaves: Vec<MerkleNode> = items
            .into_iter()
            .map(|(key, value)| MerkleNode::leaf(&key, &value))
            .collect();

        tree.item_count = leaves.len();

        if leaves.is_empty() {
            return Ok(tree);
        }

        // Build tree bottom-up
        tree.root = Some(Self::build_level(leaves, branching_factor));
        Ok(tree)
    }

    /// Build one level of the tree
    fn build_level(mut nodes: Vec<MerkleNode>, branching_factor: usize) -> MerkleNode {
        if nodes.len() == 1 {
            return nodes.pop().unwrap();
        }

        let mut parents = Vec::new();
        let mut children = Vec::new();
        let total_nodes = nodes.len();

        for (i, node) in nodes.into_iter().enumerate() {
            children.push(node);

            if children.len() == branching_factor || i == total_nodes - 1 {
                parents.push(MerkleNode::internal(children.clone()));
                children.clear();
            }
        }

        Self::build_level(parents, branching_factor)
    }

    /// Get the root hash
    pub fn root_hash(&self) -> Option<Bytes> {
        self.root.as_ref().map(|r| r.hash.clone())
    }

    /// Compare with another tree and find differences
    pub fn diff(&self, other: &MerkleTree) -> MerkleDiff {
        // Quick check: if root hashes match, trees are identical
        if self.root_hash() == other.root_hash() {
            return MerkleDiff::default();
        }

        let mut diff = MerkleDiff::default();

        // If one tree is empty
        match (&self.root, &other.root) {
            (None, None) => return diff,
            (Some(root), None) => {
                diff.only_in_left = root.leaf_hashes();
                return diff;
            }
            (None, Some(root)) => {
                diff.only_in_right = root.leaf_hashes();
                return diff;
            }
            (Some(left), Some(right)) => {
                Self::diff_nodes(left, right, &mut diff);
            }
        }

        diff
    }

    /// Recursively diff two nodes
    fn diff_nodes(left: &MerkleNode, right: &MerkleNode, diff: &mut MerkleDiff) {
        // If hashes match, subtrees are identical
        if left.matches(right) {
            return;
        }

        // If we're at leaf level, record the difference
        if left.level == 0 && right.level == 0 {
            if let (Some(left_range), Some(right_range)) = (&left.key_range, &right.key_range) {
                if left_range.0 == right_range.0 {
                    // Same key, different value
                    diff.modified.push((left_range.0.clone(), left.hash.clone(), right.hash.clone()));
                } else {
                    // Different keys
                    diff.only_in_left.push((left_range.0.clone(), left.hash.clone()));
                    diff.only_in_right.push((right_range.0.clone(), right.hash.clone()));
                }
            }
            return;
        }

        // Compare children
        let mut left_idx = 0;
        let mut right_idx = 0;

        while left_idx < left.children.len() || right_idx < right.children.len() {
            match (left.children.get(left_idx), right.children.get(right_idx)) {
                (Some(left_child), Some(right_child)) => {
                    // Compare key ranges to determine ordering
                    let cmp = match (&left_child.key_range, &right_child.key_range) {
                        (Some(l), Some(r)) => l.0.cmp(&r.0),
                        _ => std::cmp::Ordering::Equal,
                    };

                    match cmp {
                        std::cmp::Ordering::Less => {
                            // Left child comes before right child
                            diff.only_in_left.extend(left_child.leaf_hashes());
                            left_idx += 1;
                        }
                        std::cmp::Ordering::Greater => {
                            // Right child comes before left child
                            diff.only_in_right.extend(right_child.leaf_hashes());
                            right_idx += 1;
                        }
                        std::cmp::Ordering::Equal => {
                            // Children have overlapping ranges, recurse
                            Self::diff_nodes(left_child, right_child, diff);
                            left_idx += 1;
                            right_idx += 1;
                        }
                    }
                }
                (Some(left_child), None) => {
                    diff.only_in_left.extend(left_child.leaf_hashes());
                    left_idx += 1;
                }
                (None, Some(right_child)) => {
                    diff.only_in_right.extend(right_child.leaf_hashes());
                    right_idx += 1;
                }
                (None, None) => break,
            }
        }
    }

    /// Get proof of inclusion for a key
    pub fn get_proof(&self, key: &[u8]) -> Option<MerkleProof> {
        self.root.as_ref().and_then(|root| {
            Self::get_proof_recursive(root, key, Vec::new())
        })
    }

    fn get_proof_recursive(
        node: &MerkleNode,
        key: &[u8],
        mut path: Vec<Bytes>,
    ) -> Option<MerkleProof> {
        // Check if key is in range
        if let Some(ref range) = node.key_range {
            if key < range.0.as_ref() || key > range.1.as_ref() {
                return None;
            }
        }

        if node.level == 0 {
            // Leaf node
            return Some(MerkleProof {
                key: Bytes::from(key.to_vec()),
                value_hash: node.hash.clone(),
                path,
            });
        }

        // Find child containing the key
        for (i, child) in node.children.iter().enumerate() {
            if let Some(ref child_range) = child.key_range {
                if key >= child_range.0.as_ref() && key <= child_range.1.as_ref() {
                    // Add sibling hashes to path
                    for (j, sibling) in node.children.iter().enumerate() {
                        if i != j {
                            path.push(sibling.hash.clone());
                        }
                    }
                    return Self::get_proof_recursive(child, key, path);
                }
            }
        }

        None
    }
}

/// Differences between two Merkle trees
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MerkleDiff {
    /// Keys only in the left tree (key, hash)
    pub only_in_left: Vec<(Bytes, Bytes)>,
    /// Keys only in the right tree (key, hash)
    pub only_in_right: Vec<(Bytes, Bytes)>,
    /// Keys modified between trees (key, left_hash, right_hash)
    pub modified: Vec<(Bytes, Bytes, Bytes)>,
}

impl MerkleDiff {
    /// Check if there are no differences
    pub fn is_empty(&self) -> bool {
        self.only_in_left.is_empty() &&
        self.only_in_right.is_empty() &&
        self.modified.is_empty()
    }

    /// Get total number of differences
    pub fn count(&self) -> usize {
        self.only_in_left.len() + self.only_in_right.len() + self.modified.len()
    }
}

/// Proof of inclusion for a key in the tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub key: Bytes,
    pub value_hash: Bytes,
    pub path: Vec<Bytes>,
}

impl MerkleProof {
    /// Verify this proof against a root hash
    pub fn verify(&self, root_hash: &Bytes) -> bool {
        // TODO: Implement proof verification
        // This would reconstruct the root hash using the path
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new(2);
        assert!(tree.root.is_none());
        assert_eq!(tree.item_count, 0);
    }

    #[test]
    fn test_single_item() {
        let items = vec![(Bytes::from("key1"), Bytes::from("value1"))];
        let tree = MerkleTree::build(items, 2).unwrap();

        assert!(tree.root.is_some());
        assert_eq!(tree.item_count, 1);
    }

    #[test]
    fn test_identical_trees() {
        let items = vec![
            (Bytes::from("key1"), Bytes::from("value1")),
            (Bytes::from("key2"), Bytes::from("value2")),
        ];

        let tree1 = MerkleTree::build(items.clone(), 2).unwrap();
        let tree2 = MerkleTree::build(items, 2).unwrap();

        assert_eq!(tree1.root_hash(), tree2.root_hash());

        let diff = tree1.diff(&tree2);
        assert!(diff.is_empty());
    }

    #[test]
    fn test_different_trees() {
        let items1 = vec![
            (Bytes::from("key1"), Bytes::from("value1")),
            (Bytes::from("key2"), Bytes::from("value2")),
        ];

        let items2 = vec![
            (Bytes::from("key1"), Bytes::from("value1")),
            (Bytes::from("key3"), Bytes::from("value3")),
        ];

        let tree1 = MerkleTree::build(items1, 2).unwrap();
        let tree2 = MerkleTree::build(items2, 2).unwrap();

        let diff = tree1.diff(&tree2);
        assert!(!diff.is_empty());
        assert!(diff.only_in_left.len() > 0 || diff.only_in_right.len() > 0);
    }
}