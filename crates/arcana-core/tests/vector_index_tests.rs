//! Tests for the in-memory VectorIndex (cosine similarity search).

use arcana_core::embeddings::VectorIndex;
use uuid::Uuid;

#[test]
fn empty_index_search() {
    let index = VectorIndex::new(3);
    assert!(index.is_empty());
    assert_eq!(index.len(), 0);

    let results = index.search(&[1.0, 0.0, 0.0], 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn upsert_and_len() {
    let index = VectorIndex::new(3);
    let id = Uuid::new_v4();
    index.upsert(id, vec![1.0, 0.0, 0.0]).unwrap();
    assert_eq!(index.len(), 1);
    assert!(!index.is_empty());
}

#[test]
fn upsert_same_id_overwrites() {
    let index = VectorIndex::new(3);
    let id = Uuid::new_v4();
    index.upsert(id, vec![1.0, 0.0, 0.0]).unwrap();
    index.upsert(id, vec![0.0, 1.0, 0.0]).unwrap();
    assert_eq!(index.len(), 1);

    // Should now match [0, 1, 0] better than [1, 0, 0]
    let results = index.search(&[0.0, 1.0, 0.0], 1).unwrap();
    assert_eq!(results[0].0, id);
    assert!((results[0].1 - 1.0).abs() < 1e-6);
}

#[test]
fn dimension_mismatch_upsert() {
    let index = VectorIndex::new(3);
    let err = index.upsert(Uuid::new_v4(), vec![1.0, 0.0]).unwrap_err();
    assert!(err.to_string().contains("dimension mismatch"));
}

#[test]
fn dimension_mismatch_search() {
    let index = VectorIndex::new(3);
    index.upsert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]).unwrap();
    let err = index.search(&[1.0, 0.0], 1).unwrap_err();
    assert!(err.to_string().contains("dimension mismatch"));
}

#[test]
fn remove_entity() {
    let index = VectorIndex::new(3);
    let id = Uuid::new_v4();
    index.upsert(id, vec![1.0, 0.0, 0.0]).unwrap();
    assert_eq!(index.len(), 1);

    index.remove(&id);
    assert_eq!(index.len(), 0);

    // Removing nonexistent is a no-op
    index.remove(&Uuid::new_v4());
}

#[test]
fn cosine_similarity_exact_match() {
    let index = VectorIndex::new(3);
    let id = Uuid::new_v4();
    index.upsert(id, vec![1.0, 0.0, 0.0]).unwrap();

    let results = index.search(&[1.0, 0.0, 0.0], 1).unwrap();
    assert_eq!(results.len(), 1);
    assert!((results[0].1 - 1.0).abs() < 1e-6); // Cosine similarity = 1.0
}

#[test]
fn cosine_similarity_orthogonal() {
    let index = VectorIndex::new(3);
    let id = Uuid::new_v4();
    index.upsert(id, vec![1.0, 0.0, 0.0]).unwrap();

    let results = index.search(&[0.0, 1.0, 0.0], 1).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].1.abs() < 1e-6); // Cosine similarity = 0.0
}

#[test]
fn nearest_neighbour_ranking() {
    let index = VectorIndex::new(3);
    let close = Uuid::new_v4();
    let far = Uuid::new_v4();
    let mid = Uuid::new_v4();

    // Query will be [1, 0, 0]
    index.upsert(close, vec![0.9, 0.1, 0.0]).unwrap(); // Very similar
    index.upsert(mid, vec![0.5, 0.5, 0.0]).unwrap(); // Somewhat similar
    index.upsert(far, vec![0.0, 0.0, 1.0]).unwrap(); // Orthogonal

    let results = index.search(&[1.0, 0.0, 0.0], 3).unwrap();
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, close);
    assert_eq!(results[1].0, mid);
    assert_eq!(results[2].0, far);

    // Scores should be descending
    assert!(results[0].1 > results[1].1);
    assert!(results[1].1 > results[2].1);
}

#[test]
fn top_k_limits_results() {
    let index = VectorIndex::new(3);
    for _ in 0..10 {
        index.upsert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]).unwrap();
    }

    let results = index.search(&[1.0, 0.0, 0.0], 3).unwrap();
    assert_eq!(results.len(), 3);
}

#[test]
fn zero_vector_query_returns_empty() {
    let index = VectorIndex::new(3);
    index.upsert(Uuid::new_v4(), vec![1.0, 0.0, 0.0]).unwrap();

    let results = index.search(&[0.0, 0.0, 0.0], 5).unwrap();
    assert!(results.is_empty());
}

#[test]
fn high_dimensional_vectors() {
    let dims = 1536; // OpenAI text-embedding-3-small dimension
    let index = VectorIndex::new(dims);

    let mut v1 = vec![0.0f32; dims];
    v1[0] = 1.0;
    let mut v2 = vec![0.0f32; dims];
    v2[1] = 1.0;

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    index.upsert(id1, v1).unwrap();
    index.upsert(id2, v2).unwrap();

    let mut query = vec![0.0f32; dims];
    query[0] = 0.9;
    query[1] = 0.1;

    let results = index.search(&query, 2).unwrap();
    assert_eq!(results[0].0, id1); // Closer to [1, 0, ...]
}
