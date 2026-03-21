use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use uuid::Uuid;

/// A simple in-process vector index backed by a flat scan.
///
/// For production use, replace the inner store with a proper ANN library
/// (e.g., usearch, hnswlib, or an external vector DB). This implementation
/// is intentionally minimal: O(n) linear scan, suitable for ≤100k vectors.
pub struct VectorIndex {
    /// Map from entity ID to its embedding vector.
    vectors: RwLock<HashMap<Uuid, Vec<f32>>>,
    dimensions: usize,
}

impl VectorIndex {
    pub fn new(dimensions: usize) -> Self {
        Self {
            vectors: RwLock::new(HashMap::new()),
            dimensions,
        }
    }

    /// Insert or update the embedding for an entity.
    pub fn upsert(&self, id: Uuid, embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.dimensions {
            anyhow::bail!(
                "embedding dimension mismatch: expected {}, got {}",
                self.dimensions,
                embedding.len()
            );
        }
        self.vectors.write().unwrap().insert(id, embedding);
        Ok(())
    }

    /// Remove an entity's embedding.
    pub fn remove(&self, id: &Uuid) {
        self.vectors.write().unwrap().remove(id);
    }

    /// Return the `k` nearest neighbours to `query` by cosine similarity.
    ///
    /// Returns a sorted list of `(id, similarity)` in descending order.
    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(Uuid, f32)>> {
        if query.len() != self.dimensions {
            anyhow::bail!(
                "query dimension mismatch: expected {}, got {}",
                self.dimensions,
                query.len()
            );
        }

        let query_norm = l2_norm(query);
        if query_norm == 0.0 {
            return Ok(vec![]);
        }

        let store = self.vectors.read().unwrap();
        let mut scores: Vec<(Uuid, f32)> = store
            .iter()
            .map(|(id, vec)| {
                let dot = dot_product(query, vec);
                let vec_norm = l2_norm(vec);
                let cosine = if vec_norm == 0.0 {
                    0.0
                } else {
                    dot / (query_norm * vec_norm)
                };
                (*id, cosine)
            })
            .collect();

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores.truncate(k);
        Ok(scores)
    }

    /// Number of indexed vectors.
    pub fn len(&self) -> usize {
        self.vectors.read().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Retrieve the stored embedding for an entity, or an error if not found.
    pub fn get(&self, id: Uuid) -> Result<Vec<f32>> {
        self.vectors
            .read()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("no embedding found for entity {id}"))
    }

    /// Returns all pairs `(id_a, id_b, similarity)` where cosine similarity >= threshold.
    /// Each pair is returned once with `id_a < id_b` by UUID ordering.
    /// Used for redundancy/dedup detection.
    pub fn pairs_above_threshold(&self, threshold: f32) -> Vec<(Uuid, Uuid, f32)> {
        let store = self.vectors.read().unwrap();
        let entries: Vec<(&Uuid, &Vec<f32>)> = store.iter().collect();
        let mut results = Vec::new();

        for i in 0..entries.len() {
            let norm_a = l2_norm(entries[i].1);
            if norm_a == 0.0 { continue; }
            for j in (i + 1)..entries.len() {
                let norm_b = l2_norm(entries[j].1);
                if norm_b == 0.0 { continue; }
                let sim = dot_product(entries[i].1, entries[j].1) / (norm_a * norm_b);
                if sim >= threshold {
                    let (a, b) = if entries[i].0 < entries[j].0 {
                        (*entries[i].0, *entries[j].0)
                    } else {
                        (*entries[j].0, *entries[i].0)
                    };
                    results.push((a, b, sim));
                }
            }
        }

        results
    }

    /// Save the index to disk as a bincode-serialized snapshot.
    pub fn save(&self, path: &Path) -> Result<()> {
        let store = self.vectors.read().unwrap();
        let snapshot = IndexSnapshot {
            dimensions: self.dimensions,
            entries: store
                .iter()
                .map(|(id, vec)| (id.to_string(), vec.clone()))
                .collect(),
        };
        let bytes = bincode::serialize(&snapshot)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Load an index from a bincode-serialized snapshot on disk.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let snapshot: IndexSnapshot = bincode::deserialize(&bytes)?;
        let mut map = HashMap::new();
        for (id_str, vec) in snapshot.entries {
            let id: Uuid = id_str.parse()?;
            map.insert(id, vec);
        }
        Ok(Self {
            dimensions: snapshot.dimensions,
            vectors: RwLock::new(map),
        })
    }
}

#[derive(Serialize, Deserialize)]
struct IndexSnapshot {
    dimensions: usize,
    entries: Vec<(String, Vec<f32>)>,
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_neighbour_basic() {
        let index = VectorIndex::new(3);
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        index.upsert(a, vec![1.0, 0.0, 0.0]).unwrap();
        index.upsert(b, vec![0.0, 1.0, 0.0]).unwrap();

        let results = index.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results[0].0, a);
        assert!((results[0].1 - 1.0).abs() < 1e-6);
    }
}
