use anyhow::Result;
use std::collections::HashMap;
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
